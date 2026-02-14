//! Witness evaluation for the $r(X)$ polynomial.
//!
//! The [`eval`] function in this module processes witness data for a
//! particular [`Circuit`] and produces raw gate values as a [`Witness`].
//! The [`Witness`] is later assembled into a [`structured::Polynomial`]
//! by the registry.

use ff::Field;
use ragu_arithmetic::Coeff;
use ragu_core::{
    Error, Result,
    drivers::{Driver, DriverTypes},
    gadgets::Bound,
    maybe::{Always, Maybe, MaybeKind},
    routines::Routine,
};
use ragu_primitives::GadgetExt;

use alloc::vec::Vec;

use super::{Circuit, DriverScope, Rank, registry, routine_with_scope, structured};

/// Witness data produced by evaluating a circuit.
///
/// Pass to [`Registry::assemble`](crate::registry::Registry::assemble)
/// to obtain the corresponding [`structured::Polynomial`].
pub struct Witness<F> {
    /// Left input wires.
    pub(crate) a: Vec<F>,

    /// Right input wires.
    pub(crate) b: Vec<F>,

    /// Output wires.
    pub(crate) c: Vec<F>,
}

impl<F> Witness<F> {
    pub(crate) fn new() -> Self {
        Self {
            a: Vec::new(),
            b: Vec::new(),
            c: Vec::new(),
        }
    }
}

impl<F: Field> Witness<F> {
    /// Assembles this witness into a [`structured::Polynomial`] using
    /// a default [`Key`](registry::Key), without registry
    /// optimizations.
    ///
    /// This is a convenience for tests that need a polynomial from a
    /// witness but don't have (or need) a full
    /// [`Registry`](registry::Registry).
    pub fn assemble_trivial<R: Rank>(&self) -> Result<structured::Polynomial<F, R>> {
        self.assemble_with_key(&registry::Key::default())
    }

    /// Assembles this witness into a [`structured::Polynomial`] using
    /// the provided registry [`Key`](registry::Key).
    pub(crate) fn assemble_with_key<R: Rank>(
        &self,
        key: &registry::Key<F>,
    ) -> Result<structured::Polynomial<F, R>> {
        if self.a.len() + 1 > R::n() {
            return Err(Error::MultiplicationBoundExceeded(R::n()));
        }

        let mut rx = structured::Polynomial::<F, R>::new();
        {
            let view = rx.forward();

            // `ONE` gate at position 0: key * key_inv = 1
            view.a.push(key.value());
            view.b.push(key.inverse());
            view.c.push(F::ONE);

            view.a.extend_from_slice(&self.a);
            view.b.extend_from_slice(&self.b);
            view.c.extend_from_slice(&self.c);
        }
        Ok(rx)
    }
}

struct Evaluator<'a, F: Field> {
    witness: &'a mut Witness<F>,
    available_b: Option<usize>,
}

impl<F: Field> DriverScope<Option<usize>> for Evaluator<'_, F> {
    fn scope(&mut self) -> &mut Option<usize> {
        &mut self.available_b
    }
}

impl<F: Field> DriverTypes for Evaluator<'_, F> {
    type ImplField = F;
    type ImplWire = ();
    type MaybeKind = Always<()>;
    type LCadd = ();
    type LCenforce = ();
}

impl<'a, F: Field> Driver<'a> for Evaluator<'a, F> {
    type F = F;
    type Wire = ();
    const ONE: Self::Wire = ();

    fn alloc(&mut self, value: impl Fn() -> Result<Coeff<Self::F>>) -> Result<Self::Wire> {
        // Packs two allocations into one multiplication gate when possible,
        // enabling consecutive allocations to share gates.
        if let Some(index) = self.available_b.take() {
            let a = self.witness.a[index];
            let b = value()?;
            self.witness.b[index] = b.value();
            self.witness.c[index] = a * b.value();
            Ok(())
        } else {
            let index = self.witness.a.len();
            self.mul(|| Ok((value()?, Coeff::Zero, Coeff::Zero)))?;
            self.available_b = Some(index);
            Ok(())
        }
    }

    fn mul(
        &mut self,
        values: impl Fn() -> Result<(Coeff<Self::F>, Coeff<Self::F>, Coeff<Self::F>)>,
    ) -> Result<((), (), ())> {
        let (a, b, c) = values()?;
        self.witness.a.push(a.value());
        self.witness.b.push(b.value());
        self.witness.c.push(c.value());

        Ok(((), (), ()))
    }

    fn add(&mut self, _: impl Fn(Self::LCadd) -> Self::LCadd) -> Self::Wire {}

    fn enforce_zero(&mut self, _: impl Fn(Self::LCenforce) -> Self::LCenforce) -> Result<()> {
        Ok(())
    }

    fn routine<Ro: Routine<Self::F> + 'a>(
        &mut self,
        routine: Ro,
        input: Bound<'a, Self, Ro::Input>,
    ) -> Result<Bound<'a, Self, Ro::Output>> {
        routine_with_scope(self, routine, input)
    }
}

/// Evaluates the witness for a circuit, producing a [`Witness`]
/// and auxiliary data.
///
/// The returned [`Witness`] can be assembled into a polynomial via
/// [`Registry::assemble`](crate::registry::Registry::assemble).
pub fn eval<'w, F: Field, C: Circuit<F>>(
    circuit: &C,
    witness: C::Witness<'w>,
) -> Result<(Witness<F>, C::Aux<'w>)> {
    let mut gates = Witness::new();
    let aux = {
        let mut dr = Evaluator {
            witness: &mut gates,
            available_b: None,
        };
        let (io, aux) = circuit.witness(&mut dr, Always::maybe_just(|| witness))?;
        io.write(&mut dr, &mut ())?;

        aux.take()
    };
    Ok((gates, aux))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::SquareCircuit;
    use ragu_pasta::Fp;

    #[test]
    fn test_rx() {
        let circuit = SquareCircuit { times: 10 };
        let witness: Fp = Fp::from(3);
        let (gates, _aux) = eval::<Fp, _>(&circuit, witness).unwrap();
        for i in 0..gates.a.len() {
            assert_eq!(gates.a[i] * gates.b[i], gates.c[i]);
        }
    }
}
