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
    drivers::{Driver, DriverTypes, emulator::Emulator},
    gadgets::{Bound, GadgetKind},
    maybe::{Always, Maybe, MaybeKind},
    routines::Routine,
};
use ragu_primitives::GadgetExt;

use alloc::{vec, vec::Vec};

use super::{Circuit, DriverScope, Rank, registry, structured};

/// One contiguous group of multiplication gates.
///
/// Segment 0 is the root segment and holds the placeholder `ONE` gate at
/// position 0. Routine calls create additional segments (see
/// [`Evaluator::routine`]).
pub(crate) struct Segment<F> {
    pub(crate) a: Vec<F>,
    pub(crate) b: Vec<F>,
    pub(crate) c: Vec<F>,
}

/// Witness data produced by evaluating a circuit.
///
/// Pass to [`Registry::assemble`](crate::registry::Registry::assemble)
/// to obtain the corresponding [`structured::Polynomial`].
pub struct Witness<F> {
    /// Per-routine gate groups. Segment 0 is the root; segments 1+ are
    /// created by [`Driver::routine`] calls.
    pub(crate) segments: Vec<Segment<F>>,
}

impl<F: Field> Witness<F> {
    pub(crate) fn new() -> Self {
        // Segment 0 starts with a zeroed placeholder for the ONE gate.
        // assemble_with_key overwrites position 0 with the actual key values.
        Self {
            segments: vec![Segment {
                a: vec![F::ZERO],
                b: vec![F::ZERO],
                c: vec![F::ZERO],
            }],
        }
    }

    fn push_segment(&mut self) {
        self.segments.push(Segment {
            a: Vec::new(),
            b: Vec::new(),
            c: Vec::new(),
        });
    }

    fn num_gates(&self) -> usize {
        self.segments.iter().map(|s| s.a.len()).sum()
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
        if self.num_gates() > R::n() {
            return Err(Error::MultiplicationBoundExceeded(R::n()));
        }

        let mut rx = structured::Polynomial::<F, R>::new();
        {
            let view = rx.forward();

            // Overwrite segment 0 position 0 with actual ONE gate values
            // (replaces zeroed placeholder from Witness::new()).
            view.a.push(key.value());
            view.b.push(key.inverse());
            view.c.push(F::ONE);

            // Remaining gates from segment 0
            view.a.extend_from_slice(&self.segments[0].a[1..]);
            view.b.extend_from_slice(&self.segments[0].b[1..]);
            view.c.extend_from_slice(&self.segments[0].c[1..]);

            // Remaining segments
            for seg in &self.segments[1..] {
                view.a.extend_from_slice(&seg.a);
                view.b.extend_from_slice(&seg.b);
                view.c.extend_from_slice(&seg.c);
            }
        }
        Ok(rx)
    }
}

/// Per-routine state that is saved and restored by [`DriverScope`].
#[derive(Default)]
struct EvalState {
    /// Gate index within the current segment, from paired allocation.
    available_b: Option<usize>,
    /// Index of the segment that receives new gates.
    current_segment: usize,
}

struct Evaluator<'a, F: Field> {
    witness: &'a mut Witness<F>,
    state: EvalState,
}

impl<F: Field> DriverScope<EvalState> for Evaluator<'_, F> {
    fn scope(&mut self) -> &mut EvalState {
        &mut self.state
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
        if let Some(index) = self.state.available_b.take() {
            let seg = &mut self.witness.segments[self.state.current_segment];
            let a = seg.a[index];
            let b = value()?;
            seg.b[index] = b.value();
            seg.c[index] = a * b.value();
            Ok(())
        } else {
            let index = self.witness.segments[self.state.current_segment].a.len();
            self.mul(|| Ok((value()?, Coeff::Zero, Coeff::Zero)))?;
            self.state.available_b = Some(index);
            Ok(())
        }
    }

    fn mul(
        &mut self,
        values: impl Fn() -> Result<(Coeff<Self::F>, Coeff<Self::F>, Coeff<Self::F>)>,
    ) -> Result<((), (), ())> {
        let (a, b, c) = values()?;
        let seg = &mut self.witness.segments[self.state.current_segment];
        seg.a.push(a.value());
        seg.b.push(b.value());
        seg.c.push(c.value());

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
        self.witness.push_segment();
        let seg = self.witness.segments.len() - 1;
        let result = self.with_scope(|this| {
            this.state.current_segment = seg;
            let mut dummy = Emulator::wireless();
            let dummy_input = Ro::Input::map_gadget(&input, &mut dummy)?;
            let aux = routine.predict(&mut dummy, &dummy_input)?.into_aux();
            routine.execute(this, input, aux)
        });

        // TODO: Remove this continuation segment once the wiring
        // polynomial evaluators (sxy, sx, sy) are segment-aware.
        //
        // The intended behavior is for `with_scope` to restore
        // `current_segment` to the parent so that subsequent gates
        // resume in the parent's segment — one segment per routine.
        // The wiring polynomial evaluators do not have segments;
        // they process gates in flat synthesis order. If the parent
        // resumes in its original segment, `assemble_with_key`
        // emits all of the parent's gates (including those that
        // follow this routine in synthesis order) before the
        // routine's gates, reordering them and breaking the
        // polynomial relation.
        //
        // The continuation segment is a compatibility shim: by
        // moving the parent into a fresh segment after each routine,
        // assembly iterates segments sequentially and produces gates
        // in synthesis order. Once the wiring polynomial evaluators
        // process gates in segment order, this extra segment and the
        // `current_segment` override below can be dropped.
        self.witness.push_segment();
        self.state.current_segment = self.witness.segments.len() - 1;

        result
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
            state: EvalState::default(),
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
        for seg in &gates.segments {
            for i in 0..seg.a.len() {
                assert_eq!(seg.a[i] * seg.b[i], seg.c[i]);
            }
        }
    }
}
