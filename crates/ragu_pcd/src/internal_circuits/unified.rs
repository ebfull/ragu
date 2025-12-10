//! Unified instance/output interface
//!
//! Many internal circuits involved in the core protocol will share a common set
//! of public inputs so that k(Y) does not need to be evaluated many times, and
//! to make it easier to reconfigure the roles of individual circuits later.

use arithmetic::Cycle;
use ragu_circuits::polynomials::Rank;
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue},
    gadgets::{Gadget, Kind},
    maybe::Maybe,
};
use ragu_primitives::{Element, Point, io::Write};

use crate::proof::Proof;

#[allow(type_alias_bounds)]
pub type OutputKind<C: Cycle> = Kind![C::CircuitField; Output<'_, _, C>];

#[derive(Gadget, Write)]
pub struct Output<'dr, D: Driver<'dr>, C: Cycle> {
    #[ragu(gadget)]
    pub nested_preamble_commitment: Point<'dr, D, C::NestedCurve>,
    #[ragu(gadget)]
    pub w: Element<'dr, D>,
    #[ragu(gadget)]
    pub c: Element<'dr, D>,
    #[ragu(gadget)]
    pub mu: Element<'dr, D>,
    #[ragu(gadget)]
    pub nu: Element<'dr, D>,

    /// This is used to ensure k(Y) has a zero coefficient for the linear term.
    #[ragu(gadget)]
    zero: Element<'dr, D>,
}

pub struct Instance<C: Cycle> {
    pub nested_preamble_commitment: C::NestedCurve,
    pub w: C::CircuitField,
    pub c: C::CircuitField,
    pub mu: C::CircuitField,
    pub nu: C::CircuitField,
}

/// An entry in the shared public inputs for an internal circuit.
pub struct Slot<'a, 'dr, D: Driver<'dr>, T, C: Cycle> {
    value: Option<T>,
    alloc: fn(&mut D, &DriverValue<D, &'a Instance<C>>) -> Result<T>,
    _marker: core::marker::PhantomData<&'dr ()>,
}

impl<'a, 'dr, D: Driver<'dr>, T: Clone, C: Cycle> Slot<'a, 'dr, D, T, C> {
    pub(super) fn new(alloc: fn(&mut D, &DriverValue<D, &'a Instance<C>>) -> Result<T>) -> Self {
        Slot {
            value: None,
            alloc,
            _marker: core::marker::PhantomData,
        }
    }

    pub fn get(&mut self, dr: &mut D, instance: &DriverValue<D, &'a Instance<C>>) -> Result<T> {
        assert!(self.value.is_none(), "Slot::get: slot already filled");
        let value = (self.alloc)(dr, instance)?;
        self.value = Some(value.clone());
        Ok(value)
    }

    pub fn set(&mut self, value: T) {
        assert!(self.value.is_none(), "Slot::set: slot already filled");
        self.value = Some(value);
    }

    fn take(self, dr: &mut D, instance: &DriverValue<D, &'a Instance<C>>) -> Result<T> {
        self.value
            .map(Result::Ok)
            .unwrap_or_else(|| (self.alloc)(dr, instance))
    }
}

pub struct OutputBuilder<'a, 'dr, D: Driver<'dr>, C: Cycle> {
    pub nested_preamble_commitment: Slot<'a, 'dr, D, Point<'dr, D, C::NestedCurve>, C>,
    pub w: Slot<'a, 'dr, D, Element<'dr, D>, C>,
    pub c: Slot<'a, 'dr, D, Element<'dr, D>, C>,
    pub mu: Slot<'a, 'dr, D, Element<'dr, D>, C>,
    pub nu: Slot<'a, 'dr, D, Element<'dr, D>, C>,
}

impl<'dr, D: Driver<'dr>, C: Cycle> Output<'dr, D, C> {
    // TODO: Expose a gadget for the "trailing zero element" pattern to simplify values() counting.
    /// Allocate an Output from a proof reference.
    pub fn alloc_from_proof<R: Rank>(
        dr: &mut D,
        proof: DriverValue<D, &Proof<C, R>>,
    ) -> Result<Self>
    where
        D: Driver<'dr, F = C::CircuitField>,
    {
        let nested_preamble_commitment = Point::alloc(
            dr,
            proof.view().map(|p| p.preamble.nested_preamble_commitment),
        )?;
        let w = Element::alloc(dr, proof.view().map(|p| p.internal_circuits.w))?;
        let c = Element::alloc(dr, proof.view().map(|p| p.internal_circuits.c))?;
        let mu = Element::alloc(dr, proof.view().map(|p| p.internal_circuits.mu))?;
        let nu = Element::alloc(dr, proof.view().map(|p| p.internal_circuits.nu))?;

        Ok(Output {
            nested_preamble_commitment,
            w,
            c,
            mu,
            nu,
            zero: Element::zero(dr),
        })
    }
}

impl<'a, 'dr, D: Driver<'dr, F = C::CircuitField>, C: Cycle> OutputBuilder<'a, 'dr, D, C> {
    pub fn new() -> Self {
        macro_rules! point_slot {
            ($field:ident) => {
                Slot::new(|dr, i: &DriverValue<D, &'a Instance<C>>| {
                    Point::alloc(dr, i.view().map(|i| i.$field))
                })
            };
        }
        macro_rules! element_slot {
            ($field:ident) => {
                Slot::new(|dr, i: &DriverValue<D, &'a Instance<C>>| {
                    Element::alloc(dr, i.view().map(|i| i.$field))
                })
            };
        }
        OutputBuilder {
            nested_preamble_commitment: point_slot!(nested_preamble_commitment),
            w: element_slot!(w),
            c: element_slot!(c),
            mu: element_slot!(mu),
            nu: element_slot!(nu),
        }
    }

    pub fn finish(
        self,
        dr: &mut D,
        instance: &DriverValue<D, &'a Instance<C>>,
    ) -> Result<Output<'dr, D, C>> {
        Ok(Output {
            nested_preamble_commitment: self.nested_preamble_commitment.take(dr, instance)?,
            w: self.w.take(dr, instance)?,
            c: self.c.take(dr, instance)?,
            mu: self.mu.take(dr, instance)?,
            nu: self.nu.take(dr, instance)?,
            zero: Element::zero(dr),
        })
    }
}
