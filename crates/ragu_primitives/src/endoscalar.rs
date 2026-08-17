//! Implements logic for endoscaling, as introduced in
//! [Halo](https://eprint.iacr.org/2019/1021).
//!
//! An endoscalar is the catchy name for a small binary string that is used to
//! perform elliptic curve scalar multiplication on curves that have an
//! efficient endomorphism attached. By producing endoscalars as challenges and
//! applying an appropriate algorithm, points on an elliptic curve can be
//! multiplied by equally "random" challenge scalars more efficiently within a
//! circuit than an arbitrary scalar.
//!
//! An endoscalar is extracted from a field element by taking the lower 128
//! bits of its canonical representative, which is well-defined only for
//! packable elements (see [`PackableElement`]). This module provides the
//! extraction, an implementation of the scaling operation for curves which
//! support the endomorphism, an implementation of the algorithm for
//! recovering the effective scalar that an endoscalar maps to for a
//! particular prime field, and [`EndoscalarChallenge`], which produces a
//! challenge validated for extraction by rejection sampling.

use ragu_arithmetic::{
    Coeff, CurveAffine, Packable,
    ff::{Field, PrimeField, PrimeFieldBits, WithSmallOrderMulGroup},
};
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue},
    gadgets::Gadget,
    maybe::Maybe,
};

use crate::{
    Boolean, Element, NonzeroBank, PackableElement, Point,
    promotion::Demoted,
    vec::{CollectFixed, ConstLen, FixedVec},
};

/// Represents a challenge used to scale elliptic curve points.
#[derive(Gadget)]
pub struct Endoscalar<'dr, D: Driver<'dr>> {
    /// The bits of this endoscalar in little-endian order.
    #[ragu(gadget)]
    bits: FixedVec<Demoted<'dr, D, Boolean<'dr, D>>, ConstLen<{ u128::BITS as usize }>>,

    /// Witness data for the represented endoscalar in compact representation.
    #[ragu(value)]
    value: DriverValue<D, u128>,
}

impl<'dr, D: Driver<'dr>> Endoscalar<'dr, D> {
    /// Allocates an endoscalar with the provided witness input value.
    ///
    /// # Soundness
    ///
    /// Any satisfying assignment makes each stored bit represent `0` or `1`.
    /// Nothing ties those bits to `value`, which is witness input: witness
    /// generation decomposes it in little-endian order, but callers needing the
    /// endoscalar bound to a specific field element must enforce that relation
    /// themselves (see [`from_packable`](Self::from_packable)).
    pub fn alloc(dr: &mut D, value: DriverValue<D, u128>) -> Result<Self> {
        let bits = (0..u128::BITS as usize)
            .map(|i| {
                let bit = Boolean::alloc(
                    dr,
                    &mut (),
                    value.as_ref().map(|v| (*v >> i) & 1u128 == 1u128),
                )?;
                Demoted::new(&bit)
            })
            .try_collect_fixed()?;

        Ok(Endoscalar { bits, value })
    }

    /// Returns an iterator over the bits in this endoscalar, little endian order.
    pub fn bits(&self) -> impl Iterator<Item = Boolean<'dr, D>> {
        let mut bits = self
            .value
            .as_ref()
            .map(|v| (0..(u128::BITS as usize)).map(move |i| (*v >> i) & 1u128 == 1u128));

        self.bits.iter().map(move |demoted_bit| {
            demoted_bit.promote(bits.as_mut().map(|bits| bits.next().unwrap()))
        })
    }

    /// Extracts an endoscalar from a packable element, taking the lower 128
    /// bits of its canonical representative.
    ///
    /// This adds no constraints: the returned endoscalar's bits are the first
    /// 128 stored bits of `packable`, whose decomposition [`PackableElement`]
    /// already constrains.
    ///
    /// # Soundness
    ///
    /// Any satisfying assignment makes each returned bit represent the
    /// corresponding low bit of the element's canonical representative.
    pub fn from_packable(packable: &PackableElement<'dr, D>) -> Result<Self>
    where
        D::F: PrimeFieldBits,
    {
        const { assert!(D::F::CAPACITY >= u128::BITS) };
        let bits = packable.bits()[..u128::BITS as usize]
            .iter()
            .map(Demoted::new)
            .try_collect_fixed()?;
        let value = packable.value().map(extract_endoscalar);

        Ok(Endoscalar { bits, value })
    }

    /// Scale a point by the endoscalar.
    ///
    /// Endoscalars in this library are $2n = 128$ bits long, and this algorithm
    /// is proven to be injective for all prime fields of size greater than
    /// $4(2^n - 1)^2$, which is comfortably safe for the Pasta fields because
    /// they are larger than $1361129467683753853705924477137396432900$. See
    /// `qa/lean/Ragu/Contrib/EndoscalarProof.lean`.
    ///
    /// # Exceptional Cases
    ///
    /// The incomplete point additions used by this method require distinct
    /// x-coordinates at every addition step. The method uses an unchecked
    /// [`NonzeroBank`] and relies on the no-collision argument above for the
    /// supported curve/endoscalar setting.
    ///
    /// # Soundness
    ///
    /// Under the no-collision assumption above, any satisfying assignment makes
    /// the returned point represent `p` scaled by this endoscalar.
    ///
    /// # Errors
    ///
    /// Returns a witness-generation error if witness input falls into an
    /// incomplete-addition exceptional case.
    pub fn group_scale<C: CurveAffine<Base = D::F>>(
        &self,
        dr: &mut D,
        p: &Point<'dr, D, C>,
    ) -> Result<Point<'dr, D, C>> {
        // Soundness: every `add_incomplete` and `double_and_add_incomplete`
        // call below requires `x_1 != x_0`. Appendix C of the Halo paper
        // (<https://eprint.iacr.org/2019/1021>) proves no such collision occurs
        // for endoscalars well beyond 128 bits on the Pasta curves Ragu uses,
        // so the bank is created in unchecked mode.
        //
        // TODO(ebfull): The no-collision argument above is a property of the
        // curve / endoscalar interaction that the `Cycle` API should attest to
        // at compile time, so callers can verify it holds for their choice of
        // curve rather than relying on this ad-hoc local justification.
        let mut bank = NonzeroBank::new_unchecked();

        let mut acc = p.endo(dr).add_incomplete(dr, p, &mut bank)?.double(dr)?;
        let mut bits = self.bits();

        // Each iteration consumes a pair of bits; u128::BITS is even.
        for _ in 0..(u128::BITS as usize / 2) {
            let negate_bit = bits.next().unwrap();
            let endo_bit = bits.next().unwrap();

            let q = p
                .conditional_negate(dr, &negate_bit)?
                .conditional_endo(dr, &endo_bit)?;
            acc = acc.double_and_add_incomplete(dr, &q, &mut bank)?;
        }

        Ok(acc)
    }

    /// Lifts this endoscalar to a field element (scales $1$ by the endoscalar).
    ///
    /// # Soundness
    ///
    /// Any satisfying assignment makes the returned element represent the
    /// effective scalar for this endoscalar.
    pub fn lift(&self, dr: &mut D) -> Result<Element<'dr, D>>
    where
        D::F: WithSmallOrderMulGroup<3>,
    {
        let mut constant_term = (D::F::ZETA + D::F::ONE).double();
        let coeffs = [
            -D::F::from(2),
            D::F::ZETA - D::F::ONE,
            (D::F::ONE - D::F::ZETA).double(),
        ];

        let mut acc = Element::zero(dr);
        let mut bits = self.bits();

        // Each iteration consumes a pair of bits; u128::BITS is even.
        for _ in 0..(u128::BITS as usize / 2) {
            let n = bits.next().unwrap();
            let e = bits.next().unwrap();
            let ne = n.and(dr, &e)?;

            acc = acc.double(dr);
            constant_term = constant_term.double();
            constant_term += D::F::ONE;

            let n = n.element().scale(dr, Coeff::Arbitrary(coeffs[0]));
            let e = e.element().scale(dr, Coeff::Arbitrary(coeffs[1]));
            let ne = ne.element().scale(dr, Coeff::Arbitrary(coeffs[2]));

            acc = acc.add(dr, &n);
            acc = acc.add(dr, &e);
            acc = acc.add(dr, &ne);
        }

        let tmp = Element::constant(dr, constant_term);
        acc = acc.add(dr, &tmp);

        Ok(acc)
    }
}

/// Lifts an endoscalar to a field element (computes the effective scalar).
///
/// This implements [Algorithm 2, \[BGH19\]](https://eprint.iacr.org/2019/1021)
/// and is the native counterpart to [`Endoscalar::lift`].
pub fn lift_endoscalar<F: WithSmallOrderMulGroup<3>>(endo: u128) -> F {
    let mut acc = (F::ZETA + F::ONE).double();
    for i in 0..(u128::BITS as usize / 2) {
        let bits = endo >> (i << 1);
        let mut tmp = F::ONE;
        if bits & 0b01u128 != 0u128 {
            tmp = -tmp;
        }
        if bits & 0b10u128 != 0u128 {
            tmp *= F::ZETA;
        }
        acc = acc.double() + tmp;
    }
    acc
}

/// Extracts an endoscalar from a packable field element.
///
/// The endoscalar is the lower 128 bits of the element's canonical
/// representative, in little-endian order. This is the native counterpart to
/// [`Endoscalar::from_packable`]; callers obtain a [`Packable`] via
/// [`Packable::new`].
pub fn extract_endoscalar<F: PrimeFieldBits>(value: Packable<F>) -> u128 {
    const { assert!(F::CAPACITY >= u128::BITS) };
    value
        .bits()
        .take(u128::BITS as usize)
        .enumerate()
        .fold(0u128, |acc, (i, bit)| acc | (u128::from(bit) << i))
}

/// A transcript challenge validated for endoscalar extraction.
///
/// Wraps a field element whose canonical representative fits in
/// `F::CAPACITY` bits, so [`extract_endoscalar`] is well-defined for it. The
/// endoscalar is extracted once at construction and returned by
/// [`endoscalar`](Self::endoscalar).
///
/// This is a native-only type, not a [`Gadget`]: a gadget must never carry a
/// contract over its witness. The in-circuit counterpart is
/// [`PackableElement`], which enforces the same range by constraint; this
/// type enforces it by construction, because [`sample`](Self::sample) is the
/// only constructor and never returns an out-of-range challenge.
#[derive(Clone, Copy, Debug)]
pub struct EndoscalarChallenge<F: PrimeFieldBits> {
    /// The accepted challenge element, validated as packable.
    packable: Packable<F>,

    /// The endoscalar extracted from `packable` at construction.
    endoscalar: u128,
}

impl<F: PrimeFieldBits> EndoscalarChallenge<F> {
    /// Produces a validated endoscalar challenge by rejection sampling.
    ///
    /// Each call to `produce` returns one candidate element together with a
    /// payload of side state derived alongside it. A packable candidate is
    /// accepted and returned with its payload; a non-packable candidate is
    /// discarded, and `produce` is called again for a fresh one. The payload
    /// lets the caller recover state that must correspond to the accepted
    /// candidate.
    ///
    /// With uniformly random candidates, each attempt succeeds with
    /// overwhelming probability (about $1 - 2^{-129}$ over the Pasta fields).
    ///
    /// # Errors
    ///
    /// An error from `produce` propagates immediately without retrying; the
    /// loop retries only on the expected non-packable outcome.
    pub fn sample<T>(mut produce: impl FnMut() -> Result<(F, T)>) -> Result<(Self, T)> {
        loop {
            let (value, payload) = produce()?;
            if let Some(packable) = Packable::new(value) {
                return Ok((
                    EndoscalarChallenge {
                        packable,
                        endoscalar: extract_endoscalar(packable),
                    },
                    payload,
                ));
            }
        }
    }

    /// Returns the endoscalar extracted from the accepted challenge element.
    pub fn endoscalar(&self) -> u128 {
        self.endoscalar
    }

    /// Returns the accepted challenge element.
    pub fn element(&self) -> F {
        *self.packable
    }
}

#[cfg(test)]
mod tests {
    use ragu_arithmetic::{
        CurveAffine, CurveExt,
        ff::{Field, PrimeFieldBits, WithSmallOrderMulGroup},
        group::{CurveAffine as _, Group},
        rand::RngExt,
    };
    use ragu_core::{Error, Result};
    use ragu_pasta::{EpAffine, Fp};

    use super::{
        Element, Endoscalar, EndoscalarChallenge, Maybe, Packable, PackableElement, Point,
    };
    use crate::{NotPackableError, Simulator, allocator::Standard};

    pub struct EndoscalarTest {
        pub value: u128,
    }

    impl EndoscalarTest {
        /// Implements [Algorithm 1, \[BGH19\]](https://eprint.iacr.org/2019/1021).
        pub fn scale<C: CurveAffine>(&self, p: &C) -> C {
            let p = p.to_curve();
            let mut acc = (p.endo() + p).double();
            for bits in (0..(u128::BITS as usize / 2)).map(|i| self.value >> (i << 1)) {
                let mut s = p;
                if bits & 0b01u128 != 0u128 {
                    s = -s;
                }
                if bits & 0b10u128 != 0u128 {
                    s = s.endo();
                }

                acc = (acc + s) + acc;
            }
            acc.into()
        }

        /// Implements [Algorithm 2, \[BGH19\]](https://eprint.iacr.org/2019/1021).
        pub fn lift<F: WithSmallOrderMulGroup<3>>(&self) -> F {
            super::lift_endoscalar(self.value)
        }
    }

    pub fn extract<F: PrimeFieldBits>(value: F) -> EndoscalarTest {
        EndoscalarTest {
            value: super::extract_endoscalar(
                Packable::new(value).expect("test input must be packable"),
            ),
        }
    }

    #[test]
    #[allow(clippy::useless_conversion)]
    fn test_endoscaling_consistency() {
        use ragu_arithmetic::group::CurveAffine as _;
        use ragu_pasta::{EpAffine, Fq};

        let p = EpAffine::generator();
        let e = EndoscalarTest {
            value: 206786806484900909362154774549736492353u128,
        };
        let scaled = e.scale(&p);
        let expected: EpAffine = (p * e.lift::<Fq>()).into();

        assert_eq!(scaled, expected);
    }

    #[test]
    fn test_extract() -> Result<()> {
        let p = EpAffine::generator();
        let r = Fp::random(&mut ragu_arithmetic::rand::rng());
        let extracted = extract(r).value;

        Simulator::<Fp>::simulate((r, extracted, p), |dr, witness| {
            let (r, extracted, p) = witness.cast();
            let p = Point::alloc(dr, p)?;
            let allocator = &mut Standard::new();
            let r = Element::alloc(dr, allocator, r)?;
            let r = PackableElement::new(dr, allocator, r)?;
            let my_extracted = Endoscalar::from_packable(&r)?;
            let allocated = Endoscalar::alloc(dr, extracted)?;

            assert_eq!(my_extracted.value.snag(), allocated.value.snag());

            let a = my_extracted.group_scale(dr, &p)?;
            let b = allocated.group_scale(dr, &p)?;
            assert_eq!(a.value().take(), b.value().take());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn test_extraction_rejects_non_packable() {
        let result = Simulator::<Fp>::simulate(-Fp::ONE, |dr, witness| {
            let allocator = &mut Standard::new();
            let elem = Element::alloc(dr, allocator, witness)?;
            let packable = PackableElement::new(dr, allocator, elem)?;
            Endoscalar::from_packable(&packable)?;
            Ok(())
        });

        let Err(err) = result else {
            panic!("witness generation must fail for a non-packable value");
        };
        assert_eq!(
            err.invalid_witness_source::<NotPackableError>(),
            Some(&NotPackableError)
        );
    }

    #[test]
    fn test_sample_grinds_until_in_range() -> Result<()> {
        // Feed one non-packable candidate followed by a packable one: `sample`
        // must reject the first, accept the second, and return the payload
        // produced alongside the accepted candidate.
        let in_range = Fp::from(42);
        let candidates = [-Fp::ONE, in_range];
        let mut calls = 0;

        let (challenge, payload) = EndoscalarChallenge::sample(|| {
            let candidate = candidates[calls];
            calls += 1;
            Ok((candidate, calls))
        })?;

        assert_eq!(calls, 2, "expected exactly one rejection");
        assert_eq!(payload, 2, "accepted candidate's payload must be returned");
        assert_eq!(challenge.element(), in_range);
        assert_eq!(
            challenge.endoscalar(),
            super::extract_endoscalar(Packable::new(in_range).expect("42 is packable")),
        );

        Ok(())
    }

    #[test]
    fn test_sample_propagates_produce_error() {
        // A genuine error from `produce` must surface immediately instead of
        // being retried: the loop retries only on non-packable candidates.
        let mut calls = 0;
        let result: Result<(EndoscalarChallenge<Fp>, ())> = EndoscalarChallenge::sample(|| {
            calls += 1;
            Err(Error::GateBoundExceeded { limit: 1 })
        });

        assert!(matches!(result, Err(Error::GateBoundExceeded { limit: 1 })));
        assert_eq!(calls, 1, "produce error must not be retried");
    }

    #[test]
    fn test_endoscaling() -> Result<()> {
        let p = EpAffine::generator();
        let r: u128 = ragu_arithmetic::rand::rng().random();
        let expected = EndoscalarTest { value: r }.scale(&p);

        Simulator::simulate((p, r), |dr, witness| {
            let (p, r) = witness.cast();
            let p = Point::alloc(dr, p.clone())?;
            let r = Endoscalar::alloc(dr, r.clone())?;

            dr.reset();
            assert_eq!(r.group_scale(dr, &p)?.value().take(), expected);
            assert_eq!(dr.num_gates(), 7 * (1 + (u128::BITS as usize / 2)));

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn test_endoscalar_lift() -> Result<()> {
        let r: u128 = ragu_arithmetic::rand::rng().random();
        let expected: Fp = EndoscalarTest { value: r }.lift();

        Simulator::<Fp>::simulate(r, |dr, witness| {
            let r = Endoscalar::alloc(dr, witness)?;
            let s = r.lift(dr)?;

            assert_eq!(*s.value().take(), expected);

            Ok(())
        })?;

        Ok(())
    }
}
