use core::ops::Deref;

use crate::ff::{PrimeField, PrimeFieldBits};

/// A prime field element whose canonical representative fits within
/// the field's [`CAPACITY`](PrimeField::CAPACITY), i.e. lies in the
/// range $[0, 2^{F::\mathrm{CAPACITY}})$.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packable<F: PrimeField>(F);

impl<F: PrimeFieldBits> Packable<F> {
    /// Wraps `value` if its canonical representative fits in `F::CAPACITY`
    /// bits.
    ///
    /// Returns `None` when `value` is greater than or equal to
    /// $2^{F::\mathrm{CAPACITY}}$.
    pub fn new(value: F) -> Option<Self> {
        let bits = value.to_le_bits();
        bits[F::CAPACITY as usize..]
            .not_any()
            .then_some(Self(value))
    }

    /// Returns the `F::CAPACITY` bits of this element in little-endian order.
    pub fn bits(&self) -> impl ExactSizeIterator<Item = bool> {
        self.0.to_le_bits().into_iter().take(F::CAPACITY as usize)
    }
}

impl<F: PrimeField> Packable<F> {
    /// Consumes this wrapper and returns the underlying field element.
    pub fn into_inner(self) -> F {
        self.0
    }
}

impl<F: PrimeField> Deref for Packable<F> {
    type Target = F;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Packable;
    use crate::{
        ff::{Field, PrimeField},
        pasta_curves::Fp,
    };

    fn capacity_bound<F: PrimeField>() -> F {
        let mut bound = F::ONE;
        for _ in 0..F::CAPACITY {
            bound = bound.double();
        }
        bound
    }

    #[test]
    fn accepts_capacity_range_boundaries() {
        let bound = capacity_bound::<Fp>();

        assert!(Packable::new(Fp::ZERO).is_some());
        assert!(Packable::new(bound - Fp::ONE).is_some());
    }

    #[test]
    fn rejects_values_outside_capacity_range() {
        let bound = capacity_bound::<Fp>();

        assert!(Packable::new(bound).is_none());
        assert!(Packable::new(-Fp::ONE).is_none());
    }

    #[test]
    fn exposes_underlying_field_element() {
        let value = Fp::from(42);
        let packable = Packable::new(value).expect("small values fit within field capacity");

        assert_eq!(*packable, value);
        assert_eq!(packable.into_inner(), value);
    }

    #[test]
    fn exposes_exactly_capacity_bits() {
        let bound = capacity_bound::<Fp>();
        let packable = Packable::new(bound - Fp::ONE).expect("capacity maximum is packable");
        let mut bits = packable.bits();

        assert_eq!(bits.len(), Fp::CAPACITY as usize);
        assert!(bits.all(|bit| bit));
    }
}
