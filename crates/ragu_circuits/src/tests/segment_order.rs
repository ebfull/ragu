//! Property tests verifying that `metrics::eval` and `rx::eval` produce
//! segments in the same DFS order with matching multiplication-gate counts.

use proptest::prelude::*;
use ragu_arithmetic::Coeff;
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue},
    gadgets::Bound,
    routines::{Prediction, Routine},
};
use ragu_pasta::Fp;

use crate::Circuit;

/// Maximum number of wire allocations generated at any one point in a scope.
const MAX_ALLOCS: usize = 6;
/// Maximum number of child routine calls per scope.
const MAX_CHILDREN: usize = 4;
/// Maximum nesting depth of the generated routine tree.
const MAX_DEPTH: u32 = 4;
/// Maximum total number of `RoutineTree` entries across the whole tree.
const MAX_TREE_SIZE: u32 = 30;

/// A `RoutineTree` describes the structure of a single routine scope: how many
/// wire allocations happen before any sub-routine is called, and what
/// sub-routines are called — each paired with a post-call allocation count.
///
/// The execution order within one scope is:
///
/// ```text
/// ├─ [pre_allocs]  alloc alloc …
/// ├─ call children[0]
/// │   └─ (children[0] scope, recursively)
/// ├─ [children[0].post_allocs]  alloc alloc …
/// ├─ call children[1]
/// │   └─ (children[1] scope, recursively)
/// ├─ [children[1].post_allocs]  alloc alloc …
/// ⋮
/// └─ call children[n]
///    [children[n].post_allocs]  alloc alloc …
/// ```
#[derive(Debug, Clone)]
struct RoutineTree {
    /// Wire allocations emitted before the first child routine call.
    pre_allocs: usize,
    /// Child routines in call order. Each child is paired with
    /// `post_allocs`: the number of wire allocations emitted in this scope
    /// immediately after that child returns.
    children: Vec<(RoutineTree, usize)>,
}

fn arb_tree() -> impl Strategy<Value = RoutineTree> {
    let leaf = (0usize..MAX_ALLOCS).prop_map(|n| RoutineTree {
        pre_allocs: n,
        children: vec![],
    });
    leaf.prop_recursive(MAX_DEPTH, MAX_TREE_SIZE, MAX_CHILDREN as u32, |inner| {
        (
            0usize..MAX_ALLOCS,
            proptest::collection::vec((inner, 0usize..MAX_ALLOCS), 0..MAX_CHILDREN),
        )
            .prop_map(|(pre_allocs, children)| RoutineTree {
                pre_allocs,
                children,
            })
    })
}

#[derive(Clone)]
struct TreeRoutine(RoutineTree);

impl Routine<Fp> for TreeRoutine {
    type Input = ();
    type Output = ();
    type Aux<'dr> = ();

    fn execute<'dr, D: Driver<'dr, F = Fp>>(
        &self,
        dr: &mut D,
        _input: Bound<'dr, D, Self::Input>,
        _aux: DriverValue<D, Self::Aux<'dr>>,
    ) -> Result<Bound<'dr, D, Self::Output>> {
        for _ in 0..self.0.pre_allocs {
            dr.alloc(|| Ok(Coeff::One))?;
        }
        for (child, post_allocs) in &self.0.children {
            dr.routine(TreeRoutine(child.clone()), ())?;
            for _ in 0..*post_allocs {
                dr.alloc(|| Ok(Coeff::One))?;
            }
        }
        Ok(())
    }

    fn predict<'dr, D: Driver<'dr, F = Fp>>(
        &self,
        _dr: &mut D,
        _input: &Bound<'dr, D, Self::Input>,
    ) -> Result<Prediction<Bound<'dr, D, Self::Output>, DriverValue<D, Self::Aux<'dr>>>> {
        Ok(Prediction::Unknown(D::just(|| ())))
    }
}

struct TreeCircuit(RoutineTree);

impl Circuit<Fp> for TreeCircuit {
    type Instance<'source> = ();
    type Witness<'source> = ();
    type Output = ();
    type Aux<'source> = ();

    fn instance<'dr, 'source: 'dr, D: Driver<'dr, F = Fp>>(
        &self,
        _dr: &mut D,
        _instance: DriverValue<D, Self::Instance<'source>>,
    ) -> Result<Bound<'dr, D, Self::Output>>
    where
        Self: 'dr,
    {
        Ok(())
    }

    fn witness<'dr, 'source: 'dr, D: Driver<'dr, F = Fp>>(
        &self,
        dr: &mut D,
        _witness: DriverValue<D, Self::Witness<'source>>,
    ) -> Result<(
        Bound<'dr, D, Self::Output>,
        DriverValue<D, Self::Aux<'source>>,
    )>
    where
        Self: 'dr,
    {
        for _ in 0..self.0.pre_allocs {
            dr.alloc(|| Ok(Coeff::One))?;
        }
        for (child, post_allocs) in &self.0.children {
            dr.routine(TreeRoutine(child.clone()), ())?;
            for _ in 0..*post_allocs {
                dr.alloc(|| Ok(Coeff::One))?;
            }
        }
        Ok(((), D::just(|| ())))
    }
}

proptest! {
    /// Checks that `metrics::eval` and `rx::eval` produce the same number of
    /// segments in the same DFS order, with matching multiplication-gate counts.
    ///
    /// The two evaluators are implemented independently; this test verifies
    /// that both traverse routine call trees in identical DFS order and
    /// agree on how many multiplication gates each segment contains.
    #[test]
    fn segment_dfs_order(tree in arb_tree()) {
        let circuit = TreeCircuit(tree);

        let metrics = crate::metrics::eval::<Fp, _>(&circuit)
            .map_err(|e| TestCaseError::fail(format!("metrics: {e:?}")))?;
        let (trace, _) = crate::rx::eval::<Fp, _>(&circuit, ())
            .map_err(|e| TestCaseError::fail(format!("rx: {e:?}")))?;

        prop_assert_eq!(
            metrics.segments.len(),
            trace.segments.len(),
            "segment count mismatch"
        );

        for (i, (m, t)) in metrics.segments.iter().zip(trace.segments.iter()).enumerate() {
            prop_assert_eq!(
                m.num_multiplication_constraints,
                t.a.len(),
                "segment {}: mul count mismatch (metrics={}, trace={})",
                i,
                m.num_multiplication_constraints,
                t.a.len(),
            );
        }
    }
}
