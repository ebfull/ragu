//! TODO(ebfull): Staging circuits.

use ff::Field;
use ragu_core::Result;

use alloc::{boxed::Box, vec::Vec};

use crate::{
    CircuitObject,
    polynomials::{Rank, structured, unstructured},
};

/// Represents a particular stage of witness construction.
pub trait StagingCircuit<F: Field, R: Rank> {
    /// The parent stage for this stage. This is set to `() = Self` for the base
    /// stage.
    type Parent: StagingCircuit<F, R>;

    /// Returns the number of values that are allocated in a stage.
    fn values() -> usize;

    /// Returns the number of multiplication gates used for allocations.
    fn size() -> usize {
        (Self::values() + 1) / 2
    }

    /// Returns the number of multiplication gates to skip before starting this
    /// stage, not including the ONE gate which is skipped in all stages.
    fn skip() -> usize {
        Self::Parent::skip() + Self::Parent::size()
    }

    /// Computes the $r(X)$ polynomial for this stage, used as a component of a
    /// larger $r(X)$ polynomial.
    fn rx(values: &[F]) -> structured::Polynomial<F, R> {
        assert_eq!(values.len(), Self::values());

        let mut values = values.iter().cloned();
        let mut rx = structured::Polynomial::new();
        {
            let rx = rx.forward();

            // ONE is not set.
            rx.a.push(F::ZERO);
            rx.b.push(F::ZERO);
            rx.c.push(F::ZERO);

            for _ in 0..Self::skip() {
                rx.a.push(F::ZERO);
                rx.b.push(F::ZERO);
                rx.c.push(F::ZERO);
            }

            for _ in 0..Self::size() {
                let a = values.next().unwrap_or(F::ZERO);
                let b = values.next().unwrap_or(F::ZERO);
                let c = a * b;
                // ONE
                rx.a.push(a);
                rx.b.push(b);
                rx.c.push(c);
            }

            assert!(
                values.next().is_none(),
                "staging circuit rx should consume all values"
            );
        }

        rx
    }

    /// Converts this staging circuit into a circuit object.
    ///
    /// Staging circuits do not behave like normal circuits because they do not
    /// have a `ONE` wire and are used solely for partial witness commitments.
    /// As a result, they must be computed differently.
    fn into_object<'a>() -> Result<Box<dyn CircuitObject<F, R> + 'a>> {
        Ok(Box::new(StagingObject::new(Self::skip(), Self::size())?))
    }
}

impl<F: Field, R: Rank> StagingCircuit<F, R> for () {
    type Parent = ();

    fn values() -> usize {
        0
    }

    fn skip() -> usize {
        0
    }
}

#[derive(Clone)]
struct StagingObject<R: Rank> {
    skip: usize,
    size: usize,
    _marker: core::marker::PhantomData<R>,
}

impl<R: Rank> StagingObject<R> {
    /// Creates a new staging circuit polynomial with the given `skip` and
    /// `size` values. Witnesses that satisfy this circuit will have all
    /// non-`ONE` multiplication gate wires enforced to equal zero except
    /// between the `skip..size` gates.
    fn new(skip: usize, size: usize) -> Result<Self> {
        if skip + size + 1 > R::n() {
            return Err(ragu_core::Error::MultiplicationBoundExceeded(R::n()));
        }

        Ok(Self {
            skip,
            size,
            _marker: core::marker::PhantomData,
        })
    }
}

impl<F: Field, R: Rank> CircuitObject<F, R> for StagingObject<R> {
    fn sxy(&self, x: F, y: F) -> F {
        // Bound is enforced in `StagingObject::new`.
        assert!(self.skip + self.size < R::n());
        let reserved: usize = R::n() - self.skip - self.size - 1;

        if x == F::ZERO || y == F::ZERO {
            // If either x or y is zero, the polynomial evaluates to zero. This
            // is unlike standard circuits because the constant term is not used
            // to store the `ONE` wire.
            return F::ZERO;
        }

        let x_inv = x.invert().expect("x is not zero");
        let y2 = y.square();
        let y3 = y * y2;
        let x_y3 = x * y3;
        let xinv_y3 = x_inv * y3;

        let block = |end: usize, len: usize| -> F {
            let w = y * x.pow_vartime([(4 * R::n() - 2 - end) as u64]);
            let v = y2 * x.pow_vartime([(2 * R::n() + 1 + end) as u64]);
            let u = y3 * x.pow_vartime([(2 * R::n() - 2 - end) as u64]);

            let plus = arithmetic::geosum::<F>(x_y3, len);
            let minus = arithmetic::geosum::<F>(xinv_y3, len);

            w * plus + v * minus + u * plus
        };

        let c1 = block(self.skip - 1, self.skip);
        let c2 = block(R::n() - 2, reserved);

        y.pow_vartime([(3 * reserved) as u64]) * c1 + c2
    }

    fn sx(&self, x: F) -> unstructured::Polynomial<F, R> {
        // Bound is enforced in `StagingObject::new`.
        assert!(self.skip + self.size < R::n());
        let reserved: usize = R::n() - self.skip - self.size - 1;

        if x == F::ZERO {
            return unstructured::Polynomial::new();
        }

        let mut coeffs = Vec::with_capacity(R::num_coeffs());
        {
            let x_inv = x.invert().expect("x is not zero");
            let xn = x.pow_vartime([R::n() as u64]); // xn = x^n
            let xn2 = xn.square(); // xn2 = x^(2n)
            let mut u = xn2 * x_inv; // x^(2n - 1)
            let mut v = xn2; // x^(2n)
            let xn4 = xn2.square(); // x^(4n)
            let mut w = xn4 * x_inv; // x^(4n - 1)

            let mut alloc = || {
                let out = (u, v, w);
                u *= x_inv;
                v *= x;
                w *= x_inv;
                out
            };

            let mut enforce_zero = |out: (F, F, F)| {
                coeffs.push(out.0);
                coeffs.push(out.1);
                coeffs.push(out.2);
            };

            alloc(); // ONE

            for _ in 0..self.skip {
                enforce_zero(alloc());
            }
            for _ in 0..self.size {
                alloc();
            }
            for _ in 0..reserved {
                enforce_zero(alloc());
            }
        }

        coeffs.push(F::ZERO); // The constant term is always zero.
        coeffs.reverse();

        unstructured::Polynomial::from_coeffs(coeffs)
    }

    fn sy(&self, y: F) -> structured::Polynomial<F, R> {
        // Bound is enforced in `StagingObject::new`.
        assert!(self.skip + self.size < R::n());
        let reserved: usize = R::n() - self.skip - self.size - 1;

        let mut poly = structured::Polynomial::new();
        if y == F::ZERO {
            return poly;
        }

        let mut yq = y.pow_vartime([(3 * (reserved + self.skip)) as u64]);
        let y_inv = y.invert().expect("y is not zero");

        {
            let poly = poly.backward();

            // ONE
            poly.a.push(F::ZERO);
            poly.b.push(F::ZERO);
            poly.c.push(F::ZERO);

            for _ in 0..self.skip {
                poly.a.push(yq);
                yq *= y_inv;
                poly.b.push(yq);
                yq *= y_inv;
                poly.c.push(yq);
                yq *= y_inv;
            }
            for _ in 0..self.size {
                poly.a.push(F::ZERO);
                poly.b.push(F::ZERO);
                poly.c.push(F::ZERO);
            }
            for _ in 0..reserved {
                poly.a.push(yq);
                yq *= y_inv;
                poly.b.push(yq);
                yq *= y_inv;
                poly.c.push(yq);
                yq *= y_inv;
            }
        }

        poly
    }
}

#[cfg(test)]
mod tests {
    use ff::Field;
    use proptest::prelude::*;
    use ragu_core::{
        Result,
        drivers::{Coeff, Driver, LinearExpression, Witness},
    };
    use ragu_pasta::Fp;
    use rand::thread_rng;

    use crate::{CircuitExt, CircuitObject, polynomials::Rank};

    use super::{StagingCircuit, StagingObject};

    impl<F: Field, R: Rank> crate::Circuit<F> for StagingObject<R> {
        type Instance<'source> = ();
        type Witness<'source> = ();
        type Output<'dr, D: Driver<'dr, F = F>> = ();
        type Aux<'source> = ();

        fn instance<'dr, 'source: 'dr, D: Driver<'dr, F = F>>(
            &self,
            _: &mut D,
            _: Witness<D, Self::Instance<'source>>,
        ) -> Result<Self::Output<'dr, D>> {
            Ok(())
        }

        fn witness<'dr, 'source: 'dr, D: Driver<'dr, F = F>>(
            &self,
            dr: &mut D,
            _: Witness<D, Self::Witness<'source>>,
        ) -> Result<(Self::Output<'dr, D>, Witness<D, Self::Aux<'source>>)>
        where
            Self: 'dr,
        {
            let reserved = self.skip + self.size + 1;
            assert!(reserved <= R::n());

            for _ in 0..self.skip {
                let (a, b, c) = dr.mul(|| Ok((Coeff::Zero, Coeff::Zero, Coeff::Zero)))?;
                dr.enforce_zero(|lc| lc.add(&a))?;
                dr.enforce_zero(|lc| lc.add(&b))?;
                dr.enforce_zero(|lc| lc.add(&c))?;
            }

            for _ in 0..self.size {
                dr.mul(|| Ok((Coeff::Zero, Coeff::Zero, Coeff::Zero)))?;
            }

            for _ in 0..(R::n() - reserved) {
                let (a, b, c) = dr.mul(|| Ok((Coeff::Zero, Coeff::Zero, Coeff::Zero)))?;
                dr.enforce_zero(|lc| lc.add(&a))?;
                dr.enforce_zero(|lc| lc.add(&b))?;
                dr.enforce_zero(|lc| lc.add(&c))?;
            }

            Ok(((), D::just(|| ())))
        }
    }

    type R = crate::polynomials::R<7>;

    #[test]
    fn test_staging_valid() -> Result<()> {
        struct MyStage1;
        struct MyStage2;

        impl StagingCircuit<Fp, R> for MyStage1 {
            type Parent = ();

            fn values() -> usize {
                13
            }
        }

        impl StagingCircuit<Fp, R> for MyStage2 {
            type Parent = MyStage1;

            fn values() -> usize {
                20
            }
        }

        let rx1 = MyStage1::rx(
            &(0..13)
                .map(|_| Fp::random(thread_rng()))
                .collect::<Vec<_>>(),
        );
        let rx2 = MyStage2::rx(
            &(0..20)
                .map(|_| Fp::random(thread_rng()))
                .collect::<Vec<_>>(),
        );

        let circ1 = MyStage1::into_object()?;
        let circ2 = MyStage2::into_object()?;

        let z = Fp::random(thread_rng());
        let y = Fp::random(thread_rng());

        {
            let mut rhs = rx1.clone();
            rhs.dilate(z);
            rhs.add_assign(&circ1.sy(y));
            rhs.add_assign(&R::tz(z));
            assert_eq!(rx1.revdot(&rhs), Fp::ZERO);
        }

        assert_eq!(rx1.revdot(&circ1.sy(y)), Fp::ZERO);
        assert_eq!(rx2.revdot(&circ2.sy(y)), Fp::ZERO);
        assert!(rx1.revdot(&circ2.sy(y)) != Fp::ZERO);
        assert!(rx2.revdot(&circ1.sy(y)) != Fp::ZERO);

        Ok(())
    }

    proptest! {
        #[test]
        fn test_exy_proptest(skip in 0..R::n(), num in 0..R::n()) {
            prop_assume!(skip + 1 + num <= R::n());

            let staging_object = StagingObject::<R>::new(skip, num).unwrap();
            let comparison_object = staging_object.clone().into_object::<R>().unwrap();

            let check = |x: Fp, y: Fp| {
                let xn_minus_1 = x.pow_vartime([(4 * R::n() - 1) as u64]);

                // This adjusts for the single "ONE" constraint which is always skipped
                // in staging witnesses.
                let sxy = comparison_object.sxy(x, y) - xn_minus_1;
                let mut sx = comparison_object.sx(x);
                {
                    sx[0] -= xn_minus_1;
                }
                let mut sy = comparison_object.sy(y);
                {
                    let sy = sy.backward();
                    sy.c[0] -= Fp::ONE;
                }

                prop_assert_eq!(sy.eval(x), sxy);
                prop_assert_eq!(sx.eval(y), sxy);
                prop_assert_eq!(staging_object.sxy(x, y), sxy);
                prop_assert_eq!(staging_object.sx(x).eval(y), sxy);
                prop_assert_eq!(staging_object.sy(y).eval(x), sxy);

                Ok(())
            };

            let x = Fp::random(thread_rng());
            let y = Fp::random(thread_rng());
            check(x, y)?;
            check(Fp::ZERO, y)?;
            check(x, Fp::ZERO)?;
            check(Fp::ZERO, Fp::ZERO)?;

        }
    }
}
