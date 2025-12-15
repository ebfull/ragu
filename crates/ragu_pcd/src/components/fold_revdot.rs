//! Operations and utilities for reasoning about folded revdot claims.

use ragu_core::{Result, drivers::Driver};
use ragu_primitives::{
    Element,
    vec::{ConstLen, FixedVec, Len},
};

use core::marker::PhantomData;

/// The parameters $(m, n)$ that dictate the multi-layer revdot reduction.
///
/// The first layer involves $n$ instances of size-$m$ revdot reductions, and
/// the second layer reduces these into a single revdot using a single size-$m$
/// revdot reduction.
///
/// The parameters here collapse as much as $m \cdot n$ claims into a single
/// claim using roughly $f(m, n) = n * (2m^2 + m + 2) + 2n^2 + n + 2$
/// multiplication constraints.
pub trait Parameters: 'static + Send + Sync + Clone + Copy + Default {
    type N: Len;
    type M: Len;
}

/// Default parameters for native revdot folding (N=3, M=1).
#[derive(Clone, Copy, Default)]
pub struct NativeParameters;

impl Parameters for NativeParameters {
    type N = ConstLen<3>;
    type M = ConstLen<1>;
}

/// Represents the number of "error" terms produced during a folding operation
/// of many `revdot` claims.
///
/// Given $m$ claims being folded, the error terms are defined as the
/// off-diagonal entries of an $m \times m$ matrix, which by definition has $m *
/// (m - 1)$ terms.
///
/// See the book entry on [folding revdot
/// claims](https://tachyon.z.cash/_ragu_INTERNAL_ONLY_H83J19XK1/design/structured.html#folding)
/// for more information.
pub struct ErrorTermsLen<L: Len>(PhantomData<L>);

impl<L: Len> Len for ErrorTermsLen<L> {
    fn len() -> usize {
        let n = L::len();
        // n * (n - 1) = n² - n
        n * n - n
    }
}

/// Compute the folded revdot claim `c` from the error terms and ky values.
pub fn compute_c<'dr, D: Driver<'dr>, P: Parameters>(
    dr: &mut D,
    mu: &Element<'dr, D>,
    nu: &Element<'dr, D>,
    error_terms: &FixedVec<Element<'dr, D>, ErrorTermsLen<P::N>>,
    ky_values: &FixedVec<Element<'dr, D>, P::N>,
) -> Result<Element<'dr, D>> {
    let munu = mu.mul(dr, nu)?;
    let mu_inv = mu.invert(dr)?;

    let mut error_terms = error_terms.iter();
    let mut ky_values = ky_values.iter();

    let mut result = Element::zero(dr);
    let mut row_power = Element::one();

    let n = P::N::len();
    for i in 0..n {
        let mut col_power = row_power.clone();
        for j in 0..n {
            let term = if i == j {
                ky_values.next().expect("should exist")
            } else {
                error_terms.next().expect("should exist")
            };

            let contribution = col_power.mul(dr, term)?;
            result = result.add(dr, &contribution);
            col_power = col_power.mul(dr, &munu)?;
        }
        row_power = row_power.mul(dr, &mu_inv)?;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff::Field;
    use ragu_core::{drivers::emulator::Emulator, maybe::Maybe};
    use ragu_pasta::Fp;
    use ragu_primitives::{Simulator, vec::CollectFixed};
    use rand::rngs::OsRng;

    /// Test parameters with N=3, M=1.
    #[derive(Clone, Copy, Default)]
    struct TestParams3;
    impl Parameters for TestParams3 {
        type N = ConstLen<3>;
        type M = ConstLen<1>;
    }

    /// Test parameters with configurable N.
    #[derive(Clone, Copy, Default)]
    struct TestParams<const N: usize>;
    impl<const N: usize> Parameters for TestParams<N> {
        type N = ConstLen<N>;
        type M = ConstLen<1>;
    }

    #[test]
    fn test_revdot_folding() -> Result<()> {
        type P = TestParams3;
        let n = <P as Parameters>::N::len();

        let a: Vec<Fp> = (0..n).map(|_| Fp::random(OsRng)).collect();
        let b: Vec<Fp> = (0..n).map(|_| Fp::random(OsRng)).collect();

        let mut ky = vec![];
        let mut error = vec![];

        for (i, a) in a.iter().enumerate() {
            for (j, b) in b.iter().enumerate() {
                if i == j {
                    ky.push(a * b);
                } else {
                    error.push(a * b);
                }
            }
        }

        let mu = Fp::random(OsRng);
        let nu = Fp::random(OsRng);
        let mu_inv = mu.invert().unwrap();

        let expected_c = arithmetic::eval(a.iter(), mu_inv) * arithmetic::eval(b.iter(), mu * nu);

        // Run routine with Emulator.
        let dr = &mut Emulator::execute();

        let mu = Element::constant(dr, mu);
        let nu = Element::constant(dr, nu);

        let error_terms = error
            .iter()
            .map(|&v| Element::constant(dr, v))
            .collect_fixed()
            .unwrap();

        let ky_values = ky
            .iter()
            .map(|&v| Element::constant(dr, v))
            .collect_fixed()
            .unwrap();

        let result = compute_c::<_, P>(dr, &mu, &nu, &error_terms, &ky_values)?;
        let computed_c = result.value().take();

        assert_eq!(
            *computed_c, expected_c,
            "C routine computed value doesn't match expected"
        );

        Ok(())
    }

    #[test]
    fn test_compute_c_constraints() -> Result<()> {
        fn measure<P: Parameters>() -> Result<usize> {
            let sim = Simulator::simulate((), |dr, _| {
                let mu = Element::constant(dr, Fp::random(OsRng));
                let nu = Element::constant(dr, Fp::random(OsRng));
                let error_terms = (0..ErrorTermsLen::<P::N>::len())
                    .map(|_| Element::constant(dr, Fp::random(OsRng)))
                    .collect_fixed()?;
                let ky_values = (0..P::N::len())
                    .map(|_| Element::constant(dr, Fp::random(OsRng)))
                    .collect_fixed()?;

                compute_c::<_, P>(dr, &mu, &nu, &error_terms, &ky_values)?;
                Ok(())
            })?;

            Ok(sim.num_multiplications())
        }

        // Formula: 2*m^2 + m + 2
        assert_eq!(measure::<TestParams<5>>()?, 57);
        assert_eq!(measure::<TestParams<15>>()?, 467);
        assert_eq!(measure::<TestParams<30>>()?, 1832);
        assert_eq!(measure::<TestParams<60>>()?, 7262);

        Ok(())
    }

    #[test]
    fn test_multireduce() -> Result<()> {
        fn measure<PM: Parameters, PN: Parameters>() -> Result<usize> {
            let rng = OsRng;
            let sim = Simulator::simulate(rng, |dr, mut rng| {
                let mu = Element::alloc(dr, rng.view_mut().map(Fp::random))?;
                let nu = Element::alloc(dr, rng.view_mut().map(Fp::random))?;
                let error_terms = (0..ErrorTermsLen::<PM::N>::len())
                    .map(|_| Element::alloc(dr, rng.view_mut().map(Fp::random)))
                    .try_collect_fixed()?;
                let ky_values = (0..PM::N::len())
                    .map(|_| Element::alloc(dr, rng.view_mut().map(Fp::random)))
                    .try_collect_fixed()?;

                let mut collapsed = vec![];
                for _ in 0..PN::N::len() {
                    let v = compute_c::<_, PM>(dr, &mu, &nu, &error_terms, &ky_values)?;
                    collapsed.push(v);
                }
                let collapsed = FixedVec::new(collapsed)?;
                let error_terms = (0..ErrorTermsLen::<PN::N>::len())
                    .map(|_| Element::alloc(dr, rng.view_mut().map(Fp::random)))
                    .try_collect_fixed()?;

                compute_c::<_, PN>(dr, &mu, &nu, &error_terms, &collapsed)?;

                Ok(())
            })?;

            let num = sim.num_multiplications();

            // N * cost(M) + cost(N) where cost(x) = 2x² + x + 2
            let expected = |m: usize, n: usize| {
                let cost = |x: usize| 2 * x * x + x + 2;
                n * cost(m) + cost(n)
            };

            assert_eq!(num, expected(PM::N::len(), PN::N::len()));

            Ok(sim.num_multiplications())
        }

        assert_eq!(measure::<TestParams<2>, TestParams<2>>()?, 36);
        assert_eq!(measure::<TestParams<3>, TestParams<7>>()?, 268);
        assert_eq!(measure::<TestParams<6>, TestParams<11>>()?, 1135);
        assert_eq!(measure::<TestParams<5>, TestParams<10>>()?, 782);
        assert_eq!(measure::<TestParams<10>, TestParams<10>>()?, 2332);

        Ok(())
    }
}
