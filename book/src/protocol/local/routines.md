# Routines

The [overview](overview.md) presents $s(X, Y)$ as a monolithic bivariate polynomial
encoding a circuit's linear constraints. In practice, circuit code is structured
into [routines] — self-contained sections of synthesis logic whose invocations
occupy contiguous blocks of gates and constraints. This page describes how
routine invocations decompose $s(X, Y)$ into relocatable pieces and how that
decomposition interacts with the [registry](registry.md).

Three levels of terminology distinguish definition from use from layout:

- **Routine**: the type and its configuration — a function definition. A
  routine's Rust type plus any [runtime parameters][params] (depth, threshold,
  etc.) determines what constraints it will produce.
- **Invocation**: a specific call with specific inputs — a call site. Two
  invocations of the same routine with different inputs produce different wire
  values but the same constraint *structure*.
- **Segment**: the polynomial footprint of an invocation — the contiguous range
  of gate indices and $Y$-power slots it occupies in $s(X, Y)$. Like a stack
  frame, each invocation gets its own segment, and nested calls produce nested
  segments.

The rest of this page works from segments upward: first the layout, then
the algebraic decomposition each segment admits, then how segments compose
into subtrees and interact with the registry.

## Segments

A **segment** is the polynomial footprint of a routine invocation: a contiguous
range of gate indices ($X$-monomials) and $Y$-power slots in $s(X, Y)$. The root
circuit code — everything outside routine calls — occupies segment 0. Nested
calls create child segments, indexed in DFS encounter order:

```
Synthesis trace              Seg
├─ c₀ ······················ [0]  (root)
├─ call RoutineA
│   └─ ···················── [1]
├─ c₁ ······················ [0]  (root continues)
├─ call RoutineB
│   ├─ b₀ ·················· [2]
│   ├─ call RoutineC
│   │   └─ ················· [3]
│   └─ b₁ ·················· [2]
└─ c₂ ······················ [0]  (root continues)
```

The **floor plan** assigns each segment a gate offset $m_s$ and a constraint
offset $\ell_s$. Segment 0 is pinned at the origin — gate 0 hosts the ONE wire
($\v{c}_0 = 1$) and the registry key, fixing a known position from which all
drivers seed their monomial evaluations.[^wire-access]

[^wire-access]: A routine's constraints can reference exactly four classes of
    wires: input wires from the parent gadget, internally allocated wires, child
    routine output wires, and the ONE wire at gate 0. Rust ownership enforces
    this boundary — no other external wire is reachable.

The trace polynomial $r(X)$ shares the same segmented layout: each invocation
contributes a contiguous block of $(a, b, c)$ gate values, interface wires
(routine inputs allocated in the parent's scope) live in the parent's trace
segment, and both polynomials are assembled against the same floor plan. The
`rx` driver collects per-segment traces during synthesis; assembly scatters
them to absolute positions afterward.

During synthesis — which may be parallelized — each segment is annotated with a
**DFS path**: the sequence of routine-call indices from root to that segment
(e.g., path $[1, 0]$ means "the root's second routine call, then that routine's
first nested call"). After synthesis completes, segments are lexicographically
sorted by DFS path to recover canonical encounter order.[^dfs-sort] All drivers
traverse the resulting segment tree identically, regardless of the parallelism
strategy used during synthesis.

[^dfs-sort]: The sort is necessary because parallel synthesis may complete
    segments out of order. Lexicographic comparison of the path vectors
    reconstructs the sequential DFS traversal.

### Paired Allocation

A plain `alloc` would call `mul` and waste two of three wires. The **pairing
optimization** packs consecutive allocations into a single gate: the first
`alloc` creates a gate and returns its $\v{a}$-wire, stashing the $\v{b}$-wire
slot; the second `alloc` takes the stashed $\v{b}$-wire, fills in $c = a \cdot
b$, and clears the stash. Two allocations share one gate instead of consuming
two.

Each routine scope resets the pairing state — a child routine starts with no
pending $\v{b}$-wire, regardless of the parent's state. The parent's stashed
slot is preserved across the boundary via the scope save/restore mechanism
(described in [Evaluation Mechanics](#evaluation-mechanics)), so a half-filled
gate from the parent is never corrupted. When a routine exits with an odd
allocation count, its stashed $\v{b}$-wire is dropped — one gate slot wasted per
such invocation.[^pairing-drivers] The floor planner is oblivious to pairing; it
works from the metrics driver's final counts, which already reflect the
optimization.

[^pairing-drivers]: Each driver represents the stashed slot differently — a
    `bool` for the counter, an `Option<usize>` (gate index) for the trace
    evaluator, an `Option<WireEval<F>>` for `sx` and `sxy`. The scope
    save/restore mechanism handles all variants uniformly.

## Affine Decomposition

A segment at floor-plan position $(m, \ell)$ contributes to $s(x, y)$:

$$
y^\ell \left( f(x, y) + \sum_i e_i \cdot g_i(y) \right)
$$

The **structural part** $f(x, y)$ is determined by the routine's constraint
pattern and gate offset $m$. It depends on $x$ through internal wire monomials
evaluated at the point $x$, and on $y$ through Horner accumulation over the
segment's constraints. The **interface evaluations** $e_i$ are scalars from
evaluating parent-scope wires at $X = x$. Each $g_i(y)$ is a $Y$-polynomial whose coefficients are the fixed weights
with which the $i$-th interface wire appears in each constraint — field
constants, so $g_i$ is independent of $x$. The $y^\ell$ factor positions the
segment in the $Y$-dimension.

The set of interface wires depends on the unit of analysis. For a single
segment, interface wires include the routine's input wires *and* child output
wires — the parent's constraints reference those outputs, but the values are
determined by child execution, not by the segment's own constraint pattern. For
a memoized subtree, child outputs become internal to the cached unit and drop
out of the interface, narrowing it to the root routine's inputs alone.

The structural part decomposes by wire type:

$$
f(x, y) = x^{-m} \cdot A(y) + x^{+m} \cdot B(y) + C(y)
$$

$A(y)$ aggregates internal $\v{a}$- and $\v{c}$-wire contributions (from
decreasing-exponent monomials $X^{2n-1-i}$ and $X^{4n-1-i}$), $B(y)$ aggregates
internal $\v{b}$-wire contributions (from increasing-exponent monomials
$X^{2n+i}$), and $C(y)$ captures the ONE wire at $X^{4n-1}$, which is
independent of $m$. The ONE wire is an implicit interface term — its evaluation
and position are fixed across all circuits and placements, so it enters $C(y)$
rather than the explicit interface sum $\sum_i e_i \cdot g_i(y)$. It never
breaks cross-circuit alignment. Each of $A(y)$, $B(y)$, $C(y)$ is a polynomial
in $y$ at the fixed evaluation point $x$, with no dependence on the gate
offset — the $m$-dependence is isolated into the $x^{\pm m}$ factors.

Ownership-enforced isolation makes this decomposition structural, not
conventional. Internal wires cannot escape a routine — Rust ownership ensures
they are consumed within the body. No external constraint can reference an
internal wire, and no internal constraint can reference an external wire except
through the input gadget. Combined with [gadget fungibility][fungibility], the
same routine instance (same type and runtime configuration) with type-equivalent
inputs always produces structurally identical constraint patterns. This is not
an assumption the system can rely on automatically — two invocations that happen
to share structure must be explicitly discovered via
[fingerprinting](routines.md#identity).

## Shifting

**Y-shift** by $\Delta\ell$: multiply the entire contribution by
$y^{\Delta\ell}$ — a single scalar factor.

**X-shift** by $\Delta m$: from the $(A, B, C)$ decomposition,

$$
f\big|_{m + \Delta m} = x^{-\Delta m} \cdot x^{-m} A(y) \;+\; x^{+\Delta m} \cdot x^{+m} B(y) \;+\; C(y)
$$

The two dimensions are independent: changing the gate offset does not affect
constraint indexing, and vice versa.

The `sxy` driver absorbs the gate offset into running $X$-monomials during
synthesis — each segment's monomials are initialized from its absolute gate
position. The $Y$-position is deferred to combination time as a single $y^\ell$
multiplier. This asymmetry — gate offset baked in, constraint offset deferred —
is why the scalar path can share work across placements differing only in
$\ell$, while different gate offsets produce different Horner results even for
the same routine type.

## Evaluation Mechanics

The gate-offset asymmetry extends to a per-driver memoization split. The `sx`
driver writes coefficients at absolute polynomial indices tied to the gate
offset; the `sy` driver initializes its $Y$-power from the absolute constraint
offset. Neither produces a position-independent result, so neither can memoize
across placements. Only the `sxy` driver — whose local Horner accumulator
depends on the routine type and input wire evaluations alone — produces a
position-independent result that can be cached and repositioned with a single
$y^\ell$ multiplier. This asymmetry is why subtree memoization targets the
scalar path.

Entering a routine is structurally a scope jump: save the parent's running
monomial evaluations, $Y$-power counter, Horner accumulator, and pairing state;
jump to the child segment's absolute floor-plan position; execute the child's
body; restore the parent's state on return.[^scope-jump] The $Y$-power counter
is **not** continuous across routine boundaries — it jumps to the segment's
absolute $\ell_s$ on entry and restores the parent's counter on return. This
discontinuity is the operational counterpart of the $y^\ell$ shifting factor
from [Shifting](#shifting): each segment starts its Horner accumulation at its
own absolute constraint offset, not at the next sequential power after the
parent's most recent constraint.

[^scope-jump]: Implemented via `DriverScope::with_scope`, which uses
    `mem::replace` to swap the driver's running state for fresh initial values
    at the child's floor-plan position, and swaps back on return.

The `sxy` driver makes this scope structure explicit through a **compositional
Horner tree**. Each routine maintains two values: a local `result` (Horner
accumulator over its own constraints) and a `sum` (aggregate of positioned child
contributions). The local `result` is position-independent — it depends only on
the routine type and input wire evaluations, not on the segment's placement
$\ell$. At combination time, a parent incorporates a child's contribution as:

$$
\text{sum}_{\text{parent}} \mathrel{+}= y^{\ell_{\text{child}}} \cdot \text{result}_{\text{child}} + \text{sum}_{\text{child}}
$$

The same routine invoked twice with identical inputs produces identical `result`
values; only the positional multiplier $y^{\ell}$ differs. This
position-independence is the foundation for subtree caching on the scalar path.

## Subtrees

When child segments occupy fixed relative positions within a parent, shifting
the root by $\Delta m$ shifts every descendant by the same $\Delta m$.
Cross-segment wire references — a parent's constraint referencing a child's
output, or a child's constraint referencing a parent's input — also shift
uniformly, since relative positions are preserved. The subtree has a uniform
$(A, B, C)$ decomposition: a single relocatable unit whose gate-offset
dependence factors into one pair of scalars $(x^{-m}, x^{+m})$.

**Floating children** break this uniformity. The floor planner may position a
child segment independently, giving the parent's constraints mixed positional
dependencies: its own internal wires scale with $x^{\pm m_R}$ while a child's
output wires scale with $x^{\pm m_C}$. When $m_R \neq m_C$, the structural part
no longer factors into a uniform pair — each independently positioned segment
introduces an additional positional degree of freedom.

This tension drives the floor planner's choice between **vertical memoization**
(rigid subtrees, children at fixed relative offsets — child outputs absorbed
into a single cached unit) and **horizontal alignment** (floating segments,
independently positioned — child outputs remain in the parent's interface). In
terms of the interface set [described above](#affine-decomposition): vertical
memoization narrows the interface by absorbing child outputs into the cached
unit, while horizontal alignment preserves the wider per-segment interface where
child outputs remain external. The two conflict when a routine appears both
inside a rigid subtree and independently elsewhere; the floor planner must
choose which grouping captures more sharing.

## Registry Interaction

Each circuit has its own floor plan. Restricting $m(W, X, Y)$ at $W = \omega^i$
selects the $i$-th circuit's segment structure intact; out-of-domain $W$ blends
all circuits' segments via Lagrange interpolation.

When the same routine type occupies the same floor-plan offsets across circuits:

- On the `wxy` path, the structural scalar is shared and per-circuit cost
  reduces to an interface correction proportional to interface width, not
  routine complexity.
- On the `wx` and `wy` paths, the structural polynomial is computed once and
  scaled by $\sum_{i \in \text{group}} \ell_i(w)$ — one scalar multiplication
  on an entire polynomial, versus $N$ separate polynomial additions without
  alignment.

Without alignment, each circuit's routine contributions land at different
polynomial positions, requiring per-circuit polynomial operations.

## Pipeline Integration

The proof pipeline has two independent paths that converge at assembly.

**Constraint path.** A lightweight driver (the counter) simulates synthesis
without witness data, discovering the [segment](#segments) structure — per-segment
multiplication and linear constraint counts. The floor plan is computed from
these per-segment metrics via prefix sum, assigning each segment an absolute
gate offset $m_s$ and constraint offset $\ell_s$.

**Trace path.** A second driver (the evaluator) runs synthesis with actual
witness data, recording concrete $(a, b, c)$ values per gate. Segments are
annotated with DFS paths during synthesis, then sorted into canonical order.

**Assembly.** The per-segment trace records are scattered into $r(X)$ at the
floor plan's absolute offsets — each segment's gate values are written to the
corresponding positions in the structured polynomial's coefficient vectors. Gate
0 of segment 0 is overwritten with the registry key.

**Evaluation.** Each query ($s(x, Y)$, $s(X, y)$, or $s(x, y)$) re-synthesizes
the circuit through a specialized driver — there is no cached $s(X, Y)$. Routine
boundaries give each driver the segment structure needed for
[shifting](#shifting) and memoization. On $W$-restricted registry paths,
per-circuit results are weighted by Lagrange basis coefficients and accumulated.

## Identity

Two invocations share work only when recognized as structurally equivalent.
**Shallow identity** — same segment-level constraint pattern ($f$ and the
$g_i$ match) — suffices for alignment on the polynomial-valued paths. **Deep
identity** — shallow identity at every level, same children in the same
order — is needed for subtree memoization, where an entire subtree's
contribution is cached as a single scalar. Type equality (checked via `Any` on
gadget types) is necessary but not sufficient: routines carrying different
runtime parameters produce different constraint patterns from the same Rust
type. Equivalence must therefore be discovered via fingerprinting.

The two levels serve independent optimization axes. Shallow fingerprints are
what the floor planner and the polynomial-valued drivers (`wx`, `wy`) consume:
two segments with the same shallow fingerprint can be aligned at the same
floor-plan offsets across circuits, so their structural contributions are
computed once and shared. Shallow identity is the only level that matters here,
because alignment operates on individual segments — whether the segments'
subtrees match is irrelevant to the polynomial sharing. Deep fingerprints are
what the scalar-valued `sxy` driver consumes: at a concrete evaluation point
the entire subtree collapses to one scalar, and two deeply identical subtrees
placed at the same gate offset produce the same scalar. Shallow identity alone
is insufficient for this — two routines can have identical segment-level
patterns but call different children, yielding different subtree scalars.

**Fingerprints** are opaque equality tags — compared for equality, never
combined algebraically (preserving the linear independence that Schwartz–Zippel
relies on). The shallow fingerprint uses an independent challenge scheme: three
PRF-derived challenges $\alpha_a, \alpha_b, \alpha_c$ assign each wire type a
geometric sequence over gate indices, with a fourth challenge $\hat{y}$
separating constraints via Horner accumulation. This avoids coupling to the
circuit rank $n$ — the $s(X, Y)$ monomial layout (descending exponents for
$\v{a}$/$\v{c}$, ascending for $\v{b}$) would require field inversions for the
descending-exponent wires, while the independent scheme uses only successive
multiplications.

**Exponent indexing.** Gate $i$ (0-indexed within the segment) contributes
$\alpha_t^{i+1}$ for wire type $t$. Exponents start at 1: a 0-based scheme
would evaluate all three wire types to 1 at gate 0, collapsing them — exactly
the collision the fingerprint must prevent. The Horner accumulator is seeded
with a nonzero PRF-derived value $h$; a zero seed would be invisible to leading
empty constraints, causing routines differing only by prepended `enforce_zero`
calls to collide. The fingerprint pairs the Horner scalar with exact
multiplication and linear constraint counts — without these, routines with
identical (or no) `enforce_zero` calls but different gate counts would both
produce $h$.[^fp-components]

**The ONE wire and interface wires.** The ONE wire sits at gate 0's
$\v{c}$-wire ($X^{4n-1}$), outside any segment's local gate indexing. It
receives an independent challenge $\alpha_1$ rather than $\alpha_c^1$ — the
latter equals $\alpha_c$, which already serves as gate 0's $\v{c}$-wire
evaluation, so a constraint of the form $\v{c}_0 - \text{ONE} = 0$ would
vanish. Interface wires occupy distinct positions in the gate-indexed geometric
sequences; their challenge values inherit linear independence from the
Vandermonde structure of the evaluation points, ensuring the Schwartz–Zippel
bound applies across the full set $\{\alpha_1\} \cup \{\alpha_t^{i+1}\}$.

**Deep fingerprint.** The deep fingerprint composes shallow fingerprints
bottom-up via hashing (not algebraic combination — that would risk interference
between tree levels):

$$
\text{deep}(R) = H\!\left(|\text{children}(R)|,\; \text{shallow}(R),\;
\text{binding}(R),\; \text{deep}(R_0),\; \ldots\right)
$$

The child count $|\text{children}(R)|$ prevents length-extension collisions.
The output wire binding $\text{binding}(R) = \sum_i \beta^i \cdot
\alpha_{t_i}^{g_i+1}$ encodes each output wire's relative gate index $g_i$ and
wire type $t_i$, using the same 1-based exponent convention — without it, two
subtrees whose children place output wires at different positions would be
falsely identified as equivalent. The binding enters only the deep fingerprint;
shallow fingerprints are correct without it, because at the segment level output
wire positions are already reflected in the constraint polynomial. Construction
is $O(n)$ in the number of segments: shallow fingerprints are computed during
the metrics pass, deep fingerprints assembled bottom-up.[^deep-propagation]

[^fp-components]: The triple (Horner scalar, multiplication count, linear
    constraint count) is the complete shallow fingerprint.

[^deep-propagation]: Two children with the same gate count but different
    constraints produce different shallow fingerprints, propagating upward
    through every ancestor's deep fingerprint regardless of whether external
    shape matches.

[routines]: ../guide/routines.md
[params]: ../guide/routines.md#parameterization
[fungibility]: ../guide/gadgets/index.md#fungibility
[`Routine`]: ragu_core::routines::Routine
[`Driver`]: ragu_core::drivers::Driver
