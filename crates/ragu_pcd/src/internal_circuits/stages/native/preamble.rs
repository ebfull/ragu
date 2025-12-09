use alloc::vec::Vec;
use arithmetic::Cycle;
use ff::PrimeField;
use ragu_circuits::{mesh::omega_j, polynomials::Rank, staging};
use ragu_core::{
    Error, Result,
    drivers::{Driver, DriverValue, emulator::Emulator},
    gadgets::{Gadget, GadgetKind, Kind},
    maybe::{Always, Maybe, MaybeKind},
};
use ragu_primitives::{
    Element, GadgetExt, Point,
    vec::{ConstLen, FixedVec},
};

use core::marker::PhantomData;

use crate::{header::Header as HeaderTrait, proof::Pcd, step::padded};

pub const STAGING_ID: usize = crate::internal_circuits::NATIVE_PREAMBLE_STAGING_ID;

type HeaderVec<'dr, D, const HEADER_SIZE: usize> = FixedVec<Element<'dr, D>, ConstLen<HEADER_SIZE>>;

/// Headers from a single proof's k(Y) polynomial.
pub struct ProofHeaders<F, const HEADER_SIZE: usize> {
    pub right_header: [F; HEADER_SIZE],
    pub left_header: [F; HEADER_SIZE],
    pub output_header: [F; HEADER_SIZE],
}

pub struct Witness<F, G, const HEADER_SIZE: usize> {
    pub left: ProofHeaders<F, HEADER_SIZE>,
    pub right: ProofHeaders<F, HEADER_SIZE>,
    pub left_circuit_id: F,
    pub right_circuit_id: F,
    pub left_w: F,
    pub left_c: F,
    pub left_mu: F,
    pub left_nu: F,
    pub right_w: F,
    pub right_c: F,
    pub right_mu: F,
    pub right_nu: F,
    pub left_nested_preamble_commitment: G,
    pub right_nested_preamble_commitment: G,
}

#[derive(Gadget)]
pub struct UnifiedInstance<'dr, D: Driver<'dr>, G: arithmetic::CurveAffine> {
    /// Nested preamble commitment from this proof.
    #[ragu(gadget)]
    pub nested_preamble_commitment: Point<'dr, D, G>,
    #[ragu(gadget)]
    pub w: Element<'dr, D>,
    #[ragu(gadget)]
    pub c: Element<'dr, D>,
    #[ragu(gadget)]
    pub mu: Element<'dr, D>,
    #[ragu(gadget)]
    pub nu: Element<'dr, D>,
}

#[derive(Gadget)]
pub struct ProofInputs<'dr, D: Driver<'dr>, G: arithmetic::CurveAffine, const HEADER_SIZE: usize> {
    /// Right header.
    #[ragu(gadget)]
    pub right_header: HeaderVec<'dr, D, HEADER_SIZE>,
    /// Left header.
    #[ragu(gadget)]
    pub left_header: HeaderVec<'dr, D, HEADER_SIZE>,
    /// Output header.
    #[ragu(gadget)]
    pub output_header: HeaderVec<'dr, D, HEADER_SIZE>,
    /// Circuit ID for this proof.
    #[ragu(gadget)]
    pub circuit_id: Element<'dr, D>,
    /// Unified instance data.
    #[ragu(gadget)]
    pub unified: UnifiedInstance<'dr, D, G>,
}

/// Output of the native preamble stage.
#[derive(Gadget)]
pub struct Output<'dr, D: Driver<'dr>, G: arithmetic::CurveAffine, const HEADER_SIZE: usize> {
    /// Inputs from the left proof.
    #[ragu(gadget)]
    pub left: ProofInputs<'dr, D, G, HEADER_SIZE>,
    /// Inputs from the right proof.
    #[ragu(gadget)]
    pub right: ProofInputs<'dr, D, G, HEADER_SIZE>,
}

impl<F: PrimeField, G: Copy, const HEADER_SIZE: usize> Witness<F, G, HEADER_SIZE> {
    /// Create a witness from two PCDs.
    ///
    /// This extracts data directly from the proofs and computes output headers
    /// using the encoder pattern, eliminating the need for manual k(Y)
    /// reconstruction.
    pub fn from_pcds<'source, C, R, HL, HR>(
        left: &Pcd<'source, C, R, HL>,
        right: &Pcd<'source, C, R, HR>,
    ) -> Result<Self>
    where
        C: Cycle<CircuitField = F, NestedCurve = G>,
        R: Rank,
        HL: HeaderTrait<F>,
        HR: HeaderTrait<F>,
    {
        let left_headers = ProofHeaders {
            right_header: vec_to_array(&left.proof.application.right_header)?,
            left_header: vec_to_array(&left.proof.application.left_header)?,
            output_header: encode_output_header::<F, HL, HEADER_SIZE>(left.data.clone())?,
        };

        let right_headers = ProofHeaders {
            right_header: vec_to_array(&right.proof.application.right_header)?,
            left_header: vec_to_array(&right.proof.application.left_header)?,
            output_header: encode_output_header::<F, HR, HEADER_SIZE>(right.data.clone())?,
        };

        Ok(Witness {
            left: left_headers,
            right: right_headers,
            left_circuit_id: omega_j(left.proof.application.circuit_id as u32),
            right_circuit_id: omega_j(right.proof.application.circuit_id as u32),
            // Unified instance data from left proof
            left_w: left.proof.internal_circuits.w,
            left_c: left.proof.internal_circuits.c,
            left_mu: left.proof.internal_circuits.mu,
            left_nu: left.proof.internal_circuits.nu,
            // Unified instance data from right proof
            right_w: right.proof.internal_circuits.w,
            right_c: right.proof.internal_circuits.c,
            right_mu: right.proof.internal_circuits.mu,
            right_nu: right.proof.internal_circuits.nu,
            // Nested preamble commitments
            left_nested_preamble_commitment: left.proof.preamble.nested_preamble_commitment,
            right_nested_preamble_commitment: right.proof.preamble.nested_preamble_commitment,
        })
    }
}

fn vec_to_array<F: Copy, const N: usize>(v: &[F]) -> Result<[F; N]> {
    v.try_into().map_err(|_| Error::VectorLengthMismatch {
        expected: N,
        actual: v.len(),
    })
}

/// Encode header data into a fixed-size field element array.
fn encode_output_header<F: PrimeField, H: HeaderTrait<F>, const HEADER_SIZE: usize>(
    data: H::Data<'_>,
) -> Result<[F; HEADER_SIZE]> {
    let mut emulator = Emulator::execute();
    let gadget = H::encode(&mut emulator, Always::maybe_just(|| data))?;
    let padded = padded::for_header::<H, HEADER_SIZE, _>(&mut emulator, gadget)?;

    let mut elements = Vec::with_capacity(HEADER_SIZE);
    padded.write(&mut emulator, &mut elements)?;

    let values: Vec<F> = elements.into_iter().map(|e| *e.value().take()).collect();
    vec_to_array(&values)
}

pub struct Stage<C: Cycle, R, const HEADER_SIZE: usize> {
    _marker: PhantomData<(C, R)>,
}

impl<C: Cycle, R: Rank, const HEADER_SIZE: usize> staging::Stage<C::CircuitField, R>
    for Stage<C, R, HEADER_SIZE>
{
    type Parent = ();
    type Witness<'source> = &'source Witness<C::CircuitField, C::NestedCurve, HEADER_SIZE>;
    type OutputKind = Kind![C::CircuitField; Output<'_, _, C::NestedCurve, HEADER_SIZE>];

    fn values() -> usize {
        // 2 proofs * (3 headers * HEADER_SIZE + 1 circuit_id + 6 unified instance fields)
        2 * (3 * HEADER_SIZE + 1 + 6)
    }

    fn witness<'dr, 'source: 'dr, D: Driver<'dr, F = C::CircuitField>>(
        dr: &mut D,
        witness: DriverValue<D, Self::Witness<'source>>,
    ) -> Result<<Self::OutputKind as GadgetKind<C::CircuitField>>::Rebind<'dr, D>>
    where
        Self: 'dr,
    {
        fn alloc_header<'dr, D: Driver<'dr>, const HEADER_SIZE: usize>(
            dr: &mut D,
            data: DriverValue<D, &[D::F; HEADER_SIZE]>,
        ) -> Result<FixedVec<Element<'dr, D>, ConstLen<HEADER_SIZE>>> {
            let mut v = Vec::with_capacity(HEADER_SIZE);
            for i in 0..HEADER_SIZE {
                v.push(Element::alloc(dr, data.view().map(|d| d[i]))?);
            }
            Ok(FixedVec::new(v).expect("length"))
        }

        // Allocate left proof inputs
        let left = ProofInputs {
            right_header: alloc_header(dr, witness.view().map(|w| &w.left.right_header))?,
            left_header: alloc_header(dr, witness.view().map(|w| &w.left.left_header))?,
            output_header: alloc_header(dr, witness.view().map(|w| &w.left.output_header))?,
            circuit_id: Element::alloc(dr, witness.view().map(|w| w.left_circuit_id))?,
            unified: UnifiedInstance {
                nested_preamble_commitment: Point::alloc(
                    dr,
                    witness.view().map(|w| w.left_nested_preamble_commitment),
                )?,
                w: Element::alloc(dr, witness.view().map(|w| w.left_w))?,
                c: Element::alloc(dr, witness.view().map(|w| w.left_c))?,
                mu: Element::alloc(dr, witness.view().map(|w| w.left_mu))?,
                nu: Element::alloc(dr, witness.view().map(|w| w.left_nu))?,
            },
        };

        // Allocate right proof inputs
        let right = ProofInputs {
            right_header: alloc_header(dr, witness.view().map(|w| &w.right.right_header))?,
            left_header: alloc_header(dr, witness.view().map(|w| &w.right.left_header))?,
            output_header: alloc_header(dr, witness.view().map(|w| &w.right.output_header))?,
            circuit_id: Element::alloc(dr, witness.view().map(|w| w.right_circuit_id))?,
            unified: UnifiedInstance {
                nested_preamble_commitment: Point::alloc(
                    dr,
                    witness.view().map(|w| w.right_nested_preamble_commitment),
                )?,
                w: Element::alloc(dr, witness.view().map(|w| w.right_w))?,
                c: Element::alloc(dr, witness.view().map(|w| w.right_c))?,
                mu: Element::alloc(dr, witness.view().map(|w| w.right_mu))?,
                nu: Element::alloc(dr, witness.view().map(|w| w.right_nu))?,
            },
        };

        Ok(Output { left, right })
    }
}
