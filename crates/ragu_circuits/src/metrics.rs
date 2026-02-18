//! Circuit constraint analysis, metrics collection, and routine identity
//! fingerprinting.
//!
//! This module provides constraint system analysis by simulating circuit
//! execution without computing actual values, counting the number of
//! multiplication and linear constraints a circuit requires. It simultaneously
//! computes Schwartz–Zippel fingerprints for each routine invocation via the
//! merged [`Counter`] driver, which combines constraint counting with identity
//! evaluation in a single DFS traversal.
//!
//! # Fingerprinting
//!
//! A routine's fingerprint is the tuple `(TypeId(Input), TypeId(Output),
//! eval)`. The [`TypeId`] pairs cheaply narrow equivalence candidates by type;
//! the scalar confirms structural equivalence via random evaluation
//! (Schwartz–Zippel).
//!
//! The fingerprint is wrapped in [`RoutineIdentity`], an enum that
//! distinguishes the root circuit body ([`Root`](RoutineIdentity::Root)) from
//! actual routine invocations ([`Routine`](RoutineIdentity::Routine)).
//! `RoutineIdentity` deliberately does **not** implement comparison or hashing
//! traits, forcing callers to explicitly handle the root variant rather than
//! accidentally including it in equivalence maps.
//!
//! The fingerprint is computed by assigning three independent geometric
//! sequences to the $a$, $b$, $c$ wires and accumulating constraint values via
//! Horner's rule. If two routines produce the same fingerprint, they are
//! structurally equivalent with overwhelming probability.
//!
//! [`TypeId`]: core::any::TypeId

use ff::{Field, PrimeField};
use ragu_arithmetic::Coeff;
use ragu_core::{
    Result,
    drivers::{Driver, DriverTypes, FromDriver, emulator::Emulator},
    gadgets::{Bound, GadgetKind},
    maybe::Empty,
    routines::Routine,
};
use ragu_primitives::GadgetExt;

use alloc::vec::Vec;
use core::any::TypeId;

use super::s::common::{WireEval, WireEvalSum};
use super::{Circuit, DriverScope};

/// The structural identity of a routine record.
///
/// Distinguishes the root circuit body from actual routine invocations. The
/// root cannot be floated or memoized, so it has no fingerprint — callers must
/// handle it explicitly.
///
/// This type deliberately does **not** implement [`PartialEq`], [`Eq`],
/// [`Hash`], or ordering traits. Code that builds equivalence maps over
/// fingerprints must match on the [`Routine`](RoutineIdentity::Routine) variant
/// and handle [`Root`](RoutineIdentity::Root) separately.
#[derive(Clone, Copy, Debug)]
pub enum RoutineIdentity {
    /// The root circuit body (record 0). Cannot be floated or memoized.
    Root,
    /// An actual routine invocation with a Schwartz–Zippel fingerprint.
    Routine(RoutineFingerprint),
}

/// A Schwartz–Zippel fingerprint for a routine invocation's constraint
/// structure.
///
/// Two routines share a fingerprint when they have matching [`TypeId`] pairs
/// and matching evaluation scalars. The scalar is the low 64 bits of the field
/// element produced by running the routine's synthesis on the `Counter`
/// driver.
///
/// [`TypeId`]: core::any::TypeId
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoutineFingerprint {
    input_kind: TypeId,
    output_kind: TypeId,
    fingerprint: u64,
}

impl RoutineFingerprint {
    /// Constructs a [`RoutineFingerprint`] from a routine's `Input`/`Output`
    /// type ids and a field element evaluation.
    fn of<F: PrimeField, Ro: Routine<F>>(eval: F) -> Self {
        Self {
            input_kind: TypeId::of::<Ro::Input>(),
            output_kind: TypeId::of::<Ro::Output>(),
            fingerprint: ragu_arithmetic::low_u64(eval),
        }
    }

    /// Returns the raw scalar component of the fingerprint.
    #[cfg(test)]
    pub(crate) fn scalar(&self) -> u64 {
        self.fingerprint
    }
}

/// A record of a routine's constraint counts and structural identity.
pub struct RoutineRecord {
    /// The number of multiplication constraints in this routine.
    pub num_multiplication_constraints: usize,

    /// The number of linear constraints in this routine.
    pub num_linear_constraints: usize,

    /// The structural identity of this routine invocation.
    // TODO: consumed by the floor planner (not yet implemented)
    #[allow(dead_code)]
    pub identity: RoutineIdentity,
}

/// A summary of a circuit's constraint topology.
///
/// Captures constraint counts and per-routine records by simulating circuit
/// execution without computing actual values.
pub struct CircuitMetrics {
    /// The number of linear constraints, including those for public inputs.
    pub num_linear_constraints: usize,

    /// The number of multiplication constraints, including those used for allocations.
    pub num_multiplication_constraints: usize,

    /// The degree of the public input polynomial.
    // TODO(ebfull): not sure if we'll need this later
    #[allow(dead_code)]
    pub degree_ky: usize,

    /// Per-routine constraint records in synthesis order.
    // TODO: consumed by the floor planner (not yet implemented)
    #[allow(dead_code)]
    pub routines: Vec<RoutineRecord>,
}

/// Per-routine state that is saved and restored across routine boundaries.
///
/// Contains both the constraint counting record index and the identity
/// evaluation state (geometric sequence runners and Horner accumulator).
struct CounterScope<F> {
    /// Stashed $b$ wire from paired allocation (see [`Driver::alloc`]).
    available_b: Option<WireEval<F>>,

    /// Index into [`Counter::records`] for the current routine.
    current_record: usize,

    /// Running monomial for $a$ wires: $x_0^{i+1}$ at gate $i$.
    current_a: F,

    /// Running monomial for $b$ wires: $x_1^{i+1}$ at gate $i$.
    current_b: F,

    /// Running monomial for $c$ wires: $x_2^{i+1}$ at gate $i$.
    current_c: F,

    /// Horner accumulator for the fingerprint evaluation result.
    result: F,
}

/// A [`Driver`] that simultaneously counts constraints and computes routine
/// identity fingerprints via Schwartz–Zippel evaluation.
///
/// Assigns three independent geometric sequences (bases $x_0, x_1, x_2$) to
/// the $a$, $b$, $c$ wires and accumulates constraint values via Horner's rule
/// over $y$. When entering a routine, the identity state is saved and reset so
/// that each routine is fingerprinted independently of its calling context; the
/// child fingerprint is then folded into the parent's Horner accumulation.
struct Counter<F> {
    scope: CounterScope<F>,
    num_linear_constraints: usize,
    num_multiplication_constraints: usize,
    records: Vec<RoutineRecord>,

    /// When false, `mul` and `enforce_zero` advance geometric sequences and
    /// accumulate Horner results but do not increment constraint counts. Used
    /// during input wire remapping in [`routine`](Driver::routine).
    counting: bool,

    /// Base for the $a$-wire geometric sequence.
    x0: F,

    /// Base for the $b$-wire geometric sequence.
    x1: F,

    /// Base for the $c$-wire geometric sequence.
    x2: F,

    /// Multiplier for Horner accumulation, applied per [`enforce_zero`] call.
    ///
    /// [`enforce_zero`]: ragu_core::drivers::Driver::enforce_zero
    y: F,

    /// Evaluation of the `ONE` wire ($c$ wire from gate 0).
    ///
    /// Passed to [`WireEvalSum::new`] so that [`WireEval::One`] variants can be
    /// resolved during linear combination accumulation.
    one: F,
}

impl<F: PrimeField> Counter<F> {
    /// Creates a new counter with fixed NUMS constants.
    fn new() -> Self {
        let x0 = F::from(2);
        let x1 = F::from(3);
        let x2 = F::from(5);
        let y = F::from(7);

        Self {
            scope: CounterScope {
                available_b: None,
                current_record: 0,
                current_a: x0,
                current_b: x1,
                current_c: x2,
                result: F::ZERO,
            },
            num_linear_constraints: 0,
            num_multiplication_constraints: 0,
            records: alloc::vec![RoutineRecord {
                num_multiplication_constraints: 0,
                num_linear_constraints: 0,
                identity: RoutineIdentity::Root,
            }],
            counting: true,
            x0,
            x1,
            x2,
            y,
            one: x2, // c wire of gate 0
        }
    }
}

impl<F: Field> DriverScope<CounterScope<F>> for Counter<F> {
    fn scope(&mut self) -> &mut CounterScope<F> {
        &mut self.scope
    }
}

impl<F: Field> DriverTypes for Counter<F> {
    type MaybeKind = Empty;
    type ImplField = F;
    type ImplWire = WireEval<F>;
    type LCadd = WireEvalSum<F>;
    type LCenforce = WireEvalSum<F>;
}

impl<'dr, F: PrimeField> Driver<'dr> for Counter<F> {
    type F = F;
    type Wire = WireEval<F>;
    const ONE: Self::Wire = WireEval::One;

    /// Allocates a wire using paired allocation.
    fn alloc(&mut self, _: impl Fn() -> Result<Coeff<Self::F>>) -> Result<Self::Wire> {
        if let Some(wire) = self.scope.available_b.take() {
            Ok(wire)
        } else {
            let (a, b, _) = self.mul(|| unreachable!())?;
            self.scope.available_b = Some(b);
            Ok(a)
        }
    }

    /// Consumes a multiplication gate: increments constraint counts and returns
    /// wire values from three independent geometric sequences, advancing each
    /// by its base.
    fn mul(
        &mut self,
        _: impl Fn() -> Result<(Coeff<F>, Coeff<F>, Coeff<F>)>,
    ) -> Result<(Self::Wire, Self::Wire, Self::Wire)> {
        if self.counting {
            self.num_multiplication_constraints += 1;
            self.records[self.scope.current_record].num_multiplication_constraints += 1;
        }

        let a = self.scope.current_a;
        let b = self.scope.current_b;
        let c = self.scope.current_c;

        self.scope.current_a *= self.x0;
        self.scope.current_b *= self.x1;
        self.scope.current_c *= self.x2;

        Ok((WireEval::Value(a), WireEval::Value(b), WireEval::Value(c)))
    }

    /// Computes a linear combination of wire evaluations.
    fn add(&mut self, lc: impl Fn(Self::LCadd) -> Self::LCadd) -> Self::Wire {
        WireEval::Value(lc(WireEvalSum::new(self.one)).value)
    }

    /// Increments linear constraint count and applies one Horner step:
    /// `result = result * y + coefficient`.
    fn enforce_zero(&mut self, lc: impl Fn(Self::LCenforce) -> Self::LCenforce) -> Result<()> {
        if self.counting {
            self.num_linear_constraints += 1;
            self.records[self.scope.current_record].num_linear_constraints += 1;
        }

        self.scope.result *= self.y;
        self.scope.result += lc(WireEvalSum::new(self.one)).value;

        Ok(())
    }

    fn routine<Ro: Routine<Self::F> + 'dr>(
        &mut self,
        routine: Ro,
        input: Bound<'dr, Self, Ro::Input>,
    ) -> Result<Bound<'dr, Self, Ro::Output>> {
        // Push new record with placeholder identity.
        self.records.push(RoutineRecord {
            num_multiplication_constraints: 0,
            num_linear_constraints: 0,
            identity: RoutineIdentity::Root,
        });
        let record = self.records.len() - 1;

        // Save parent scope and reset to fresh identity state.
        let saved = core::mem::replace(
            &mut self.scope,
            CounterScope {
                available_b: None,
                current_record: record,
                current_a: self.x0,
                current_b: self.x1,
                current_c: self.x2,
                result: F::ZERO,
            },
        );

        // Map input wires from parent's binding to fresh wires in the reset
        // scope. Counting is disabled because these gates exist solely to seed
        // the geometric sequences for fingerprinting.
        self.counting = false;
        let new_input = Ro::Input::map_gadget(&input, self)?;
        self.counting = true;

        // Predict and execute.
        let mut dummy = Emulator::wireless();
        let dummy_input = Ro::Input::map_gadget(&new_input, &mut dummy)?;
        let aux = routine.predict(&mut dummy, &dummy_input)?.into_aux();
        let output = routine.execute(self, new_input, aux)?;

        // Extract fingerprint from the child's Horner accumulator.
        let child_result = self.scope.result;
        self.records[record].identity =
            RoutineIdentity::Routine(RoutineFingerprint::of::<F, Ro>(child_result));

        // Restore parent scope.
        self.scope = saved;

        // Fold child fingerprint into parent's Horner accumulation.
        self.scope.result *= self.y;
        self.scope.result += child_result;

        Ok(output)
    }
}

/// Allows [`Counter`] to receive input wires from any driver with the same
/// field type. Each source wire is mapped to a fresh allocation on the counter,
/// producing linearly independent wire values for the input gadget.
impl<'dr, F: PrimeField, D: Driver<'dr, F = F>> FromDriver<'dr, '_, D> for Counter<F> {
    type NewDriver = Self;

    fn convert_wire(&mut self, _: &D::Wire) -> Result<WireEval<F>> {
        self.alloc(|| unreachable!())
    }
}

/// Computes the [`RoutineIdentity`] for a single routine invocation.
///
/// Creates a fresh [`Counter`], maps the caller's `input` gadget into the
/// counter (allocating fresh wires for each input wire), then predicts and
/// executes the routine. The full subtree of nested routine calls is captured
/// because the counter's [`routine`](Driver::routine) implementation
/// recursively fingerprints children and folds their results.
///
/// # Arguments
///
/// - `routine`: The routine to fingerprint.
/// - `input`: The caller's input gadget, bound to driver `D`.
#[cfg(test)]
pub(crate) fn fingerprint_routine<'dr, F, D, Ro>(
    routine: &Ro,
    input: &Bound<'dr, D, Ro::Input>,
) -> Result<RoutineIdentity>
where
    F: PrimeField,
    D: Driver<'dr, F = F>,
    Ro: Routine<F>,
{
    let mut counter = Counter::<F>::new();

    // Map input from the caller's driver to Counter wires.
    let new_input = Ro::Input::map_gadget(input, &mut counter)?;

    // Predict (on a wireless emulator) then execute on the counter.
    let mut dummy = Emulator::wireless();
    let dummy_input = Ro::Input::map_gadget(&new_input, &mut dummy)?;
    let aux = routine.predict(&mut dummy, &dummy_input)?.into_aux();
    routine.execute(&mut counter, new_input, aux)?;

    Ok(RoutineIdentity::Routine(RoutineFingerprint::of::<F, Ro>(
        counter.scope.result,
    )))
}

/// Evaluates the constraint topology of a circuit.
pub fn eval<F: PrimeField, C: Circuit<F>>(circuit: &C) -> Result<CircuitMetrics> {
    let mut collector = Counter::<F>::new();
    let mut degree_ky = 0usize;

    // ONE gate
    collector.mul(|| Ok((Coeff::One, Coeff::One, Coeff::One)))?;

    // Registry key constraint
    collector.enforce_zero(|lc| lc)?;

    // Circuit synthesis
    let (io, _) = circuit.witness(&mut collector, Empty)?;
    io.write(&mut collector, &mut degree_ky)?;

    // Public output constraints
    for _ in 0..degree_ky {
        collector.enforce_zero(|lc| lc)?;
    }

    // ONE constraint
    collector.enforce_zero(|lc| lc)?;

    let record_mul: usize = collector
        .records
        .iter()
        .map(|r| r.num_multiplication_constraints)
        .sum();
    let record_lin: usize = collector
        .records
        .iter()
        .map(|r| r.num_linear_constraints)
        .sum();
    assert_eq!(record_mul, collector.num_multiplication_constraints);
    assert_eq!(record_lin, collector.num_linear_constraints);

    Ok(CircuitMetrics {
        num_linear_constraints: collector.num_linear_constraints,
        num_multiplication_constraints: collector.num_multiplication_constraints,
        degree_ky,
        routines: collector.records,
    })
}
