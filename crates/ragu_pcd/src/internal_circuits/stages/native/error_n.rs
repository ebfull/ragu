//! Error stage (layer 2) for merge operations.
//!
//! This stage handles the final N-sized revdot claim reduction.

use arithmetic::Cycle;
use ragu_circuits::{polynomials::Rank, staging};
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue},
    gadgets::{Gadget, GadgetKind, Kind},
    maybe::Maybe,
};
use ragu_primitives::{
    Element,
    vec::{CollectFixed, FixedVec, Len},
};

use core::marker::PhantomData;

pub use crate::internal_circuits::InternalCircuitIndex::ErrorNStage as STAGING_ID;

use crate::components::fold_revdot::{ErrorTermsLen, Parameters};

/// Witness data for the error_n stage (layer 2).
///
/// Contains N²-N error terms for the second layer of reduction, plus
/// the N collapsed values from layer 1 folding.
pub struct Witness<C: Cycle, P: Parameters> {
    /// The nu challenge derived from H(nested_error_m_commitment).
    /// This binds the error_n stage to the first layer's commitment.
    pub nu: C::CircuitField,
    /// Error term elements for layer 2.
    pub error_terms: FixedVec<C::CircuitField, ErrorTermsLen<P::N>>,
    /// Collapsed values from layer 1 folding (N values).
    /// These are the outputs of N M-sized revdot reductions.
    pub collapsed: FixedVec<C::CircuitField, P::N>,
}

/// Output gadget for the error_n stage.
#[derive(Gadget)]
pub struct Output<'dr, D: Driver<'dr>, P: Parameters> {
    /// The witnessed nu challenge element.
    #[ragu(gadget)]
    pub nu: Element<'dr, D>,
    /// Error term elements for layer 2.
    #[ragu(gadget)]
    pub error_terms: FixedVec<Element<'dr, D>, ErrorTermsLen<P::N>>,
    /// Collapsed values from layer 1 folding (N values).
    #[ragu(gadget)]
    pub collapsed: FixedVec<Element<'dr, D>, P::N>,
}

/// The error_n stage (layer 2) of the merge witness.
#[derive(Default)]
pub struct Stage<C: Cycle, R, const HEADER_SIZE: usize, P: Parameters> {
    _marker: PhantomData<(C, R, P)>,
}

impl<C: Cycle, R: Rank, const HEADER_SIZE: usize, P: Parameters> staging::Stage<C::CircuitField, R>
    for Stage<C, R, HEADER_SIZE, P>
{
    type Parent = super::error_m::Stage<C, R, HEADER_SIZE, P>;
    type Witness<'source> = &'source Witness<C, P>;
    type OutputKind = Kind![C::CircuitField; Output<'_, _, P>];

    fn values() -> usize {
        // 1 for nu + N² - N error terms + N collapsed values
        1 + ErrorTermsLen::<P::N>::len() + P::N::len()
    }

    fn witness<'dr, 'source: 'dr, D: Driver<'dr, F = C::CircuitField>>(
        &self,
        dr: &mut D,
        witness: DriverValue<D, Self::Witness<'source>>,
    ) -> Result<<Self::OutputKind as GadgetKind<C::CircuitField>>::Rebind<'dr, D>>
    where
        Self: 'dr,
    {
        let nu = Element::alloc(dr, witness.view().map(|w| w.nu))?;
        let error_terms = ErrorTermsLen::<P::N>::range()
            .map(|i| Element::alloc(dr, witness.view().map(|w| w.error_terms[i])))
            .try_collect_fixed()?;
        let collapsed = P::N::range()
            .map(|i| Element::alloc(dr, witness.view().map(|w| w.collapsed[i])))
            .try_collect_fixed()?;

        Ok(Output {
            nu,
            error_terms,
            collapsed,
        })
    }
}
