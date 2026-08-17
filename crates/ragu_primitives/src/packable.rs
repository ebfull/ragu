//! Packable field element gadget.
//!
//! Provides the [`PackableElement`] type, an [`Element`] whose canonical
//! little-endian bit decomposition is computed and constrained in-circuit.
//! Witness generation for a value that is not packable fails with a
//! [`NotPackableError`] source.

use alloc::boxed::Box;
use core::marker::PhantomData;

use ragu_arithmetic::{
    Packable,
    ff::{PrimeField, PrimeFieldBits},
};
use ragu_core::{
    Error, Result,
    convert::WireMap,
    drivers::{Driver, DriverValue},
    gadgets::{Bound, Gadget, GadgetKind, WireEqualizer},
    maybe::Maybe,
};

use crate::{
    Boolean, Element, GadgetExt,
    allocator::Allocator,
    comparison::GadgetEquals,
    consistent::Consistent,
    io::{Buffer, Write},
    multipack,
    vec::{FixedVec, Len},
};

/// An error indicating that an element's witness value is not packable.
///
/// [`PackableElement::new`] boxes this type as the source of
/// [`Error::InvalidWitness`] when the witness value's canonical
/// representative does not fit in [`CAPACITY`](PrimeField::CAPACITY) bits.
/// A caller that grinds candidate inputs detects this condition with
/// [`Error::invalid_witness_source`], resamples, and retries; every other
/// error reports a distinct failure.
///
/// # Examples
///
/// ```
/// use ragu_core::Error;
/// use ragu_primitives::NotPackableError;
///
/// let err = Error::InvalidWitness(Box::new(NotPackableError));
/// assert!(err.invalid_witness_source::<NotPackableError>().is_some());
/// ```
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("element is not packable")]
pub struct NotPackableError;

/// Type-level [`Len`] marker for the field's [`CAPACITY`](PrimeField::CAPACITY).
struct CapacityLen<F: PrimeField>(PhantomData<F>);

impl<F: PrimeField> Len for CapacityLen<F> {
    fn len() -> usize {
        F::CAPACITY as usize
    }
}

/// An [`Element`] constrained to have a canonical representative that fits
/// within the field's [`CAPACITY`](PrimeField::CAPACITY).
///
/// [`PackableElement`] dereferences to its underlying [`Element`]. Every
/// instance stores its canonical little-endian [`Boolean`] decomposition and
/// constrains the packing of those bits to equal the element.
pub struct PackableElement<'dr, D: Driver<'dr, F: PrimeField>> {
    element: Element<'dr, D>,
    bits: FixedVec<Boolean<'dr, D>, CapacityLen<D::F>>,
}

impl<'dr, D: Driver<'dr, F: PrimeField>> Clone for PackableElement<'dr, D> {
    fn clone(&self) -> Self {
        Self {
            element: self.element.clone(),
            bits: self.bits.clone(),
        }
    }
}

impl<'dr, D: Driver<'dr, F: PrimeFieldBits>> PackableElement<'dr, D> {
    /// Constructs a packable element from `element`, computing and storing
    /// its canonical little-endian bit decomposition.
    ///
    /// This costs $D::F::\mathrm{CAPACITY}$ gates and
    /// $2 \cdot D::F::\mathrm{CAPACITY} + 1$ constraints. The [`Boolean`]
    /// allocation spare wires are donated to `allocator`.
    ///
    /// # Soundness
    ///
    /// Any satisfying assignment makes the returned element represent a value
    /// in the range $[0, 2^{D::F::\mathrm{CAPACITY}})$ and binds each stored
    /// bit to the corresponding bit of that value.
    ///
    /// # Completeness
    ///
    /// Witness generation succeeds when `element`'s witness value is
    /// packable.
    ///
    /// # Errors
    ///
    /// Witness generation fails with [`Error::InvalidWitness`] when
    /// `element`'s witness value is not packable. The boxed source is a
    /// [`NotPackableError`] value, which callers that grind candidate inputs
    /// can detect with [`Error::invalid_witness_source`]. Any error
    /// encountered while allocating and constraining the bit decomposition
    /// propagates unchanged.
    pub fn new<A: Allocator<'dr, D>>(
        dr: &mut D,
        allocator: &mut A,
        element: Element<'dr, D>,
    ) -> Result<Self> {
        let packable = D::try_just(|| {
            Packable::new(*element.value().take())
                .ok_or_else(|| Error::InvalidWitness(Box::new(NotPackableError)))
        })?;
        let mut bit_values = packable.as_ref().map(|value| value.bits());
        let bits = FixedVec::try_from_fn(|_| {
            Boolean::alloc(
                dr,
                allocator,
                bit_values
                    .as_mut()
                    .map(|bits| bits.next().expect("Packable::bits returned too few bits")),
            )
        })?;

        let packable = Self { element, bits };
        packable.enforce_packable(dr)?;
        Ok(packable)
    }

    /// Returns the canonical little-endian bit decomposition.
    pub fn bits(&self) -> &[Boolean<'dr, D>] {
        &self.bits
    }

    /// Consumes `self` and returns the underlying [`Element`].
    pub fn into_inner(self) -> Element<'dr, D> {
        self.element
    }

    /// Returns the [`Packable`] witness value represented by this element.
    ///
    /// # Panics
    ///
    /// Panics if the gadget's range constraint was not established before the
    /// gadget was constructed. Public construction paths establish this
    /// constraint.
    pub fn value(&self) -> DriverValue<D, Packable<D::F>> {
        self.element
            .value()
            .map(|value| Packable::new(*value).expect("PackableElement invariant violated"))
    }

    /// Re-emits the relation between the stored bits and element.
    fn enforce_packable(&self, dr: &mut D) -> Result<()> {
        let packed = multipack(dr, &self.bits)?
            .pop()
            .expect("a prime field has positive capacity");
        self.element.enforce_equal(dr, &packed)
    }
}

impl<'dr, D: Driver<'dr, F: PrimeField>> core::ops::Deref for PackableElement<'dr, D> {
    type Target = Element<'dr, D>;

    fn deref(&self) -> &Element<'dr, D> {
        &self.element
    }
}

impl<'dr, D: Driver<'dr, F: PrimeFieldBits>> Consistent<'dr, D> for PackableElement<'dr, D> {
    fn enforce_consistent(&self, dr: &mut D) -> Result<()> {
        self.bits.enforce_consistent(dr)?;
        self.enforce_packable(dr)
    }
}

impl<'dr, D: Driver<'dr, F: PrimeFieldBits>> Gadget<'dr, D> for PackableElement<'dr, D> {
    type Kind = PackableElement<'static, PhantomData<D::F>>;
}

/// Safety: `PackableElement` contains only an `Element` and a `FixedVec` of
/// `Boolean`s, whose rebound forms are `Send` whenever the rebound driver's
/// wires are `Send`.
unsafe impl<F: PrimeFieldBits> GadgetKind<F> for PackableElement<'static, PhantomData<F>> {
    type Rebind<'dr, D: Driver<'dr, F = F>> = PackableElement<'dr, D>;

    fn map_gadget<'src, 'dst, WM: WireMap<F>>(
        this: &Bound<'src, WM::Src, Self>,
        wm: &mut WM,
    ) -> Result<Bound<'dst, WM::Dst, Self>>
    where
        WM::Src: Driver<'src, F = F>,
        WM::Dst: Driver<'dst, F = F>,
    {
        Ok(PackableElement {
            element: this.element.map(wm)?,
            bits: this.bits.map(wm)?,
        })
    }

    fn enforce_conservative_equal_gadget<
        'dr,
        D1: Driver<'dr, F = F>,
        D2: Driver<'dr, F = F, Wire = D1::Wire>,
    >(
        eq: &mut WireEqualizer<'_, 'dr, D1>,
        a: &Bound<'dr, D2, Self>,
        b: &Bound<'dr, D2, Self>,
    ) -> Result<()> {
        eq.enforce_conservative_equal_gadget(&a.element, &b.element)?;
        eq.enforce_conservative_equal_gadget(&a.bits, &b.bits)
    }
}

/// Encodes only the represented element; the bit decomposition is redundant.
impl<F: PrimeFieldBits> Write<F> for PackableElement<'static, PhantomData<F>> {
    fn write_gadget<'dr, D: Driver<'dr, F = F>, B: Buffer<'dr, D>>(
        this: &PackableElement<'dr, D>,
        dr: &mut D,
        buf: &mut B,
    ) -> Result<()> {
        this.element.write(dr, buf)
    }
}

impl<F: PrimeFieldBits> GadgetEquals<F> for PackableElement<'static, PhantomData<F>> {
    fn enforce_equal_gadget<
        'dr,
        D1: Driver<'dr, F = F>,
        D2: Driver<'dr, F = F, Wire = D1::Wire>,
    >(
        dr: &mut D1,
        a: &PackableElement<'dr, D2>,
        b: &PackableElement<'dr, D2>,
    ) -> Result<()> {
        // Soundness: comparing only the element suffices because
        // `enforce_packable` binds the stored bits to the element's canonical
        // decomposition, so equal elements have equal bits in any satisfied
        // assignment.
        a.element.enforce_equal(dr, &b.element)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use ragu_arithmetic::ff::Field;
    use ragu_core::maybe::Maybe;

    use super::*;
    use crate::{Simulator, allocator::Standard};

    type F = ragu_pasta::Fp;
    type Sim = Simulator<F>;

    fn capacity_bound() -> F {
        let mut bound = F::ONE;
        for _ in 0..F::CAPACITY {
            bound = bound.double();
        }
        bound
    }

    #[test]
    fn test_new_accepts_capacity_range_and_preserves_bits() -> Result<()> {
        for value in [F::ZERO, F::from(42), capacity_bound() - F::ONE] {
            let sim = Sim::simulate(value, |dr, witness| {
                let element = Element::constant(dr, *witness.snag());
                let allocator = &mut Standard::new();

                dr.reset();
                let packable = PackableElement::new(dr, allocator, element)?;

                assert_eq!(packable.value().take().into_inner(), value);
                assert_eq!(*packable.wire(), value);
                assert_eq!(packable.bits().len(), F::CAPACITY as usize);
                assert_eq!(
                    packable
                        .bits()
                        .iter()
                        .map(|bit| bit.value().take())
                        .collect::<Vec<_>>(),
                    Packable::new(value).unwrap().bits().collect::<Vec<_>>()
                );
                assert_eq!(packable.num_wires()?, F::CAPACITY as usize + 1);
                assert_eq!(*packable.clone().into_inner().value().take(), value);
                Ok(())
            })?;

            assert_eq!(sim.num_gates(), F::CAPACITY as usize);
            assert_eq!(sim.num_constraints(), 2 * F::CAPACITY as usize + 1);
        }

        Ok(())
    }

    #[test]
    fn test_new_rejects_values_outside_capacity_range() {
        for value in [capacity_bound(), -F::ONE] {
            let result = Sim::simulate(value, |dr, witness| {
                let element = Element::constant(dr, *witness.snag());
                PackableElement::new(dr, &mut Standard::new(), element)?;
                Ok(())
            });

            let Err(err) = result else {
                panic!("witness generation must fail for a non-packable value");
            };
            assert_eq!(
                err.invalid_witness_source::<NotPackableError>(),
                Some(&NotPackableError)
            );
            assert!(matches!(
                &err,
                Error::InvalidWitness(source) if source.is::<NotPackableError>()
            ));
        }
    }

    #[test]
    fn test_grinding_caller_distinguishes_not_packable() -> Result<()> {
        // A grinding caller resamples candidates until one is packable,
        // retrying only on the typed completeness failure.
        let mut accepted = None;
        for value in [capacity_bound(), -F::ONE, F::from(42)] {
            let result = Sim::simulate(value, |dr, witness| {
                let element = Element::constant(dr, *witness.snag());
                PackableElement::new(dr, &mut Standard::new(), element)?;
                Ok(())
            });

            match result {
                Ok(_) => {
                    accepted = Some(value);
                    break;
                }
                Err(err) if err.invalid_witness_source::<NotPackableError>().is_some() => continue,
                Err(err) => return Err(err),
            }
        }
        assert_eq!(accepted, Some(F::from(42)));
        Ok(())
    }

    #[test]
    fn test_new_donates_boolean_spare_wires() -> Result<()> {
        let sim = Sim::simulate(F::from(42), |dr, witness| {
            let element = Element::constant(dr, *witness.snag());
            let allocator = &mut Standard::new();
            PackableElement::new(dr, allocator, element)?;

            dr.reset();
            Element::alloc(dr, allocator, Sim::just(|| F::from(7)))?;
            Ok(())
        })?;

        assert_eq!(sim.num_gates(), 0);
        assert_eq!(sim.num_constraints(), 0);
        Ok(())
    }

    #[test]
    fn test_enforce_consistent_reestablishes_bit_and_packing_constraints() -> Result<()> {
        let sim = Sim::simulate(F::from(42), |dr, witness| {
            let element = Element::constant(dr, *witness.snag());
            let packable = PackableElement::new(dr, &mut Standard::new(), element)?;

            dr.reset();
            packable.enforce_consistent(dr)
        })?;

        assert_eq!(sim.num_gates(), F::CAPACITY as usize);
        assert_eq!(sim.num_constraints(), 3 * F::CAPACITY as usize + 1);
        Ok(())
    }

    #[test]
    fn test_enforce_consistent_rejects_mismatched_stored_bits() {
        let result = Sim::simulate(F::from(42), |dr, witness| {
            let element = Element::constant(dr, *witness.snag());
            let mut packable = PackableElement::new(dr, &mut Standard::new(), element)?;

            dr.reset();
            packable.bits[0] = packable.bits[0].not(dr);
            packable.enforce_consistent(dr)
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_equality_and_serialization_delegate_to_element() -> Result<()> {
        Sim::simulate(F::from(42), |dr, witness| {
            let element = Element::constant(dr, *witness.snag());
            let packable = PackableElement::new(dr, &mut Standard::new(), element)?;
            let other = packable.clone();
            let mut encoded = Vec::new();

            dr.reset();
            packable.enforce_equal(dr, &other)?;
            packable.write(dr, &mut encoded)?;

            assert_eq!(dr.num_gates(), 0);
            assert_eq!(dr.num_constraints(), 1);
            assert_eq!(encoded.len(), 1);
            assert_eq!(*encoded[0].value().take(), F::from(42));
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    #[should_panic(expected = "PackableElement invariant violated")]
    fn test_value_panics_when_internal_invariant_is_violated() {
        Sim::simulate((), |dr, _| {
            let element = Element::constant(dr, F::ZERO);
            let mut packable = PackableElement::new(dr, &mut Standard::new(), element)?;
            packable.element = Element::constant(dr, -F::ONE);
            let _ = packable.value();
            Ok(())
        })
        .unwrap();
    }
}
