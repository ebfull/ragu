#![allow(non_snake_case)]

use ff::Field;
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue, LinearExpression},
    gadgets::{GadgetKind, Kind},
    maybe::Maybe,
};
use ragu_pasta::Fp;
use ragu_primitives::Element;
use rand::thread_rng;

use crate::{
    Circuit, CircuitExt, CircuitIndex, CircuitObject,
    polynomials::{R, Rank},
};

/// Dummy circuit.
pub struct SquareCircuit {
    pub times: usize,
}

impl Circuit<Fp> for SquareCircuit {
    type Instance<'instance> = Fp;
    type Output = Kind![Fp; Element<'_, _>];
    type Witness<'witness> = Fp;
    type Aux<'witness> = ();

    fn instance<'dr, 'instance: 'dr, D: Driver<'dr, F = Fp>>(
        &self,
        dr: &mut D,
        instance: DriverValue<D, Self::Instance<'instance>>,
    ) -> Result<<Self::Output as GadgetKind<Fp>>::Rebind<'dr, D>> {
        Element::alloc(dr, instance)
    }

    fn witness<'dr, 'witness: 'dr, D: Driver<'dr, F = Fp>>(
        &self,
        dr: &mut D,
        witness: DriverValue<D, Self::Witness<'witness>>,
    ) -> Result<(
        <Self::Output as GadgetKind<Fp>>::Rebind<'dr, D>,
        DriverValue<D, Self::Aux<'witness>>,
    )> {
        let mut a = Element::alloc(dr, witness)?;

        for _ in 0..self.times {
            a = a.square(dr)?;
        }

        Ok((a, D::just(|| ())))
    }
}

fn consistency_checks<R: Rank>(circuit: &dyn CircuitObject<Fp, R>) {
    let x = Fp::random(thread_rng());
    let y = Fp::random(thread_rng());
    let k = Fp::random(thread_rng());

    let sxy_eval = circuit.sxy(x, y, k);
    let s0y_eval = circuit.sxy(Fp::ZERO, y, k);
    let sx0_eval = circuit.sxy(x, Fp::ZERO, k);
    let s00_eval = circuit.sxy(Fp::ZERO, Fp::ZERO, k);

    let sxY_poly = circuit.sx(x, k);
    let sXy_poly = circuit.sy(y, k).unstructured();
    let s0Y_poly = circuit.sx(Fp::ZERO, k);
    let sX0_poly = circuit.sy(Fp::ZERO, k).unstructured();

    assert_eq!(sxy_eval, arithmetic::eval(&sXy_poly[..], x));
    assert_eq!(sxy_eval, arithmetic::eval(&sxY_poly[..], y));
    assert_eq!(s0y_eval, arithmetic::eval(&sXy_poly[..], Fp::ZERO));
    assert_eq!(sx0_eval, arithmetic::eval(&sxY_poly[..], Fp::ZERO));
    assert_eq!(s0y_eval, arithmetic::eval(&s0Y_poly[..], y));
    assert_eq!(sx0_eval, arithmetic::eval(&sX0_poly[..], x));
    assert_eq!(s00_eval, arithmetic::eval(&s0Y_poly[..], Fp::ZERO));
    assert_eq!(s00_eval, arithmetic::eval(&sX0_poly[..], Fp::ZERO));
}

#[test]
fn test_simple_circuit() {
    // Simple circuit: prove knowledge of a and b such that a^5 = b^2 and a + b = c
    // and a - b = d where c and d are public inputs.
    struct MySimpleCircuit;

    impl Circuit<Fp> for MySimpleCircuit {
        type Instance<'instance> = (Fp, Fp); // Public inputs: c and d
        type Output = Kind![Fp; (Element<'_, _>, Element<'_, _>)];
        type Witness<'witness> = (Fp, Fp); // Witness: a and b
        type Aux<'witness> = ();

        fn instance<'dr, 'instance: 'dr, D: Driver<'dr, F = Fp>>(
            &self,
            dr: &mut D,
            instance: DriverValue<D, Self::Instance<'instance>>,
        ) -> Result<<Self::Output as GadgetKind<Fp>>::Rebind<'dr, D>> {
            let c = Element::alloc(dr, instance.view().map(|v| v.0))?;
            let d = Element::alloc(dr, instance.view().map(|v| v.1))?;

            Ok((c, d))
        }

        fn witness<'dr, 'witness: 'dr, D: Driver<'dr, F = Fp>>(
            &self,
            dr: &mut D,
            witness: DriverValue<D, Self::Witness<'witness>>,
        ) -> Result<(
            <Self::Output as GadgetKind<Fp>>::Rebind<'dr, D>,
            DriverValue<D, Self::Aux<'witness>>,
        )> {
            let a = Element::alloc(dr, witness.view().map(|w| w.0))?;
            let b = Element::alloc(dr, witness.view().map(|w| w.1))?;

            let a2 = a.square(dr)?;
            let a4 = a2.square(dr)?;
            let a5 = a4.mul(dr, &a)?;

            let b2 = b.square(dr)?;

            dr.enforce_zero(|lc| lc.add(a5.wire()).sub(b2.wire()))?;

            let c = a.add(dr, &b);
            let d = a.sub(dr, &b);

            Ok(((c, d), D::just(|| ())))
        }
    }

    let assignment = MySimpleCircuit
        .rx::<MyRank>(
            (
                Fp::from_raw([
                    1833481853729904510,
                    5119040798866070668,
                    13106006979685074791,
                    104139735293675522,
                ]),
                Fp::from_raw([
                    1114250137190507128,
                    15522336584428696251,
                    4689053926428793931,
                    2277752110332726989,
                ]),
            ),
            Fp::ONE,
        )
        .unwrap()
        .0;

    type MyRank = R<5>;
    let circuit = MySimpleCircuit.into_object::<MyRank>().unwrap();

    consistency_checks(&*circuit);

    let y = Fp::random(thread_rng());
    let z = Fp::random(thread_rng());
    let k = Fp::one();

    let a = assignment.clone();
    let mut b = assignment.clone();
    b.dilate(z);
    b.add_assign(&circuit.sy(y, k));
    b.add_assign(&MyRank::tz(z));

    let expected = arithmetic::eval(
        &MySimpleCircuit
            .ky((
                Fp::from_raw([
                    2947731990920411638,
                    2194633309585215303,
                    17795060906113868723,
                    2381891845626402511,
                ]),
                Fp::from_raw([
                    11756763772759733511,
                    10513277942061441772,
                    8416953053256280859,
                    2438073643388336437,
                ]),
            ))
            .unwrap(),
        y,
    );

    let a = a.unstructured();
    let b = b.unstructured();

    assert_eq!(expected, arithmetic::dot(a.iter(), b.iter().rev()),);
}

#[test]
fn test_omega_j_multiplicative_order() {
    /// Return the 2^k multiplicative order of f (assumes f is a 2^k root of unity).
    fn order<F: Field>(mut f: F) -> usize {
        let mut order = 0;
        while f != F::ONE {
            f = f.square();
            order += 1;
        }
        1 << order
    }
    assert_eq!(CircuitIndex::new(0).into::<Fp>(), Fp::ONE);
    assert_eq!(CircuitIndex::new(1).into::<Fp>(), -Fp::ONE);
    assert_eq!(order(CircuitIndex::new(0).into::<Fp>()), 1);
    assert_eq!(order(CircuitIndex::new(1).into::<Fp>()), 2);
    assert_eq!(order(CircuitIndex::new(2).into::<Fp>()), 4);
    assert_eq!(order(CircuitIndex::new(3).into::<Fp>()), 4);
    assert_eq!(order(CircuitIndex::new(4).into::<Fp>()), 8);
    assert_eq!(order(CircuitIndex::new(5).into::<Fp>()), 8);
    assert_eq!(order(CircuitIndex::new(6).into::<Fp>()), 8);
    assert_eq!(order(CircuitIndex::new(7).into::<Fp>()), 8);
}

#[test]
fn test_omega_j_consistency() -> Result<()> {
    use arithmetic::{Domain, bitreverse};
    use ff::PrimeField;

    for num_circuits in [2usize, 3, 7, 8, 15, 16, 32] {
        let log2_circuits = num_circuits.next_power_of_two().trailing_zeros();
        let domain = Domain::<Fp>::new(log2_circuits);

        for id in 0..num_circuits {
            let omega_from_function = CircuitIndex::new(id).into::<Fp>();

            let bit_reversal_id = bitreverse(id as u32, Fp::S);
            let position = ((bit_reversal_id as u64) >> (Fp::S - log2_circuits)) as usize;
            let omega_from_finalization = domain.omega().pow([position as u64]);

            assert_eq!(
                omega_from_function, omega_from_finalization,
                "Omega mismatch for circuit {} in mesh of size {}",
                id, num_circuits
            );
        }
    }

    Ok(())
}
