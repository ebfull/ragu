# Split-accumulation for Batched Evaluation

## PCS Aggregation

In the [last step](../nark.md#nark) of our NARK, the verifier needs to verify
many polynomial evaluations on different polynomials. Naively running an instance
of [PCS evaluation](../../prelim/bulletproofs.md) protocol for each claim is
expensive. Instead, we use batching techniques to aggregate all evaluation claims
into a single claim that can be verified once. This is sometimes called
_multi-opening_ or _batched opening_ in the literature. Here is how Ragu
aggregates evaluation claims of multiple points on multiple polynomials:

**Input claims**: For each $i$, we have the claim that $p_i(x_i) = y_i$ where

- public instance: $\inst:=(\bar{C}_i\in\G, x_i, y_i\in\F)_i$, the "(commitment,
  evaluation point, evaluation)" tuple held by both the prover and the verifier
- secret witness: $\wit:=(p_i(X)\in\F[X], \gamma_i\in\F)$, the underlying
  polynomial and the blinding factor used for commitment, held by the prover

**Output claim**: A single aggregated claim $p(u)=v$ where

- public instance: $\inst:=(\bar{P}, u, v)\in\G\times\F^2$, held by both
- secret witness: $\wit:=(p(X), \gamma)$, aggregated polynomial and blinding
  factors held by the prover

**Summary**: The key idea is to batch using quotient polynomials. For each
claim $p_i(x_i) = y_i$, the quotient $q_i(X) = \frac{p_i(X) - y_i}{X - x_i}$
exists (with no remainder) if and only if the claim is valid. The protocol
proceeds in three phases:
- _alpha batching_: linearly combines these quotients as 
  $f(X) = \sum_i \alpha^i \cdot q_i(X)$
- _evaluation at u_: the prover evaluates each $p_i$ at a fresh challenge point $u$
- _beta batching_: combines the quotient with the original polynomials as
  $p(X) = f(X) + \sum_i \beta^i \cdot p_i(X)$. The verifier derives the expected
  evaluation from the quotient relation and the $p_i(u)$ values.

The full protocol proceeds as follows:

1. Verifier sends challenge $\alpha \sample \F$
2. Prover computes quotient polynomials $q_i(X) = \frac{p_i(X) - y_i}{X - x_i}$
for each claim. The prover linearly combines them as
$f(X)=\sum_i \alpha^i \cdot q_i(X)$, samples a blinding factor
$\gamma_f\sample\F$, computes the commitment $\bar{F}\leftarrow\com(f(X);\gamma_f)$,
and sends $\bar{F}$ to the verifier
3. Verifier sends challenge $u\sample\F$, which will be the evaluation point for
the aggregated polynomial
4. Prover computes $p_i(u)$ for each $i$ and sends these to the verifier. When
multiple claims share the same underlying polynomial, only one evaluation per
polynomial is needed since $p_i(u)$ depends only on the polynomial, not the
original evaluation point $x_i$.
5. Verifier sends challenge $\beta\sample\F$
6. Prover computes the aggregated polynomial
$p(X) = f(X) + \sum_i \beta^i \cdot p_i(X)$ and the aggregated blinding factor
$\gamma = \gamma_f + \sum_i \beta^i \cdot \gamma_i$
7. Verifier derives the aggregated commitment
$\bar{P} = \bar{F} + \sum_i \beta^i \cdot \bar{C}_i$ and the aggregated evaluation
$v=\sum_i \alpha^i\cdot\frac{p_i(u)-y_i}{u-x_i} + \sum_i\beta^i\cdot p_i(u)$,
then outputs $(\bar{P}, u, v)$

The soundness of our aggregation relies on the simple fact that: the quotients
polynomial $q_i(X)=\frac{p_i(X)-y_i}{X-x_i}$ exist (with no remainder) if and
only if the claims $p_i(x_i) = y_i$ are valid. The random linear combination
would preserve this with overwhelming probability, causing the final verification
to fail if any one of the claims is false. The quotient relation is enforced
at step 7 when the verifier derives the $q_i(u)$ from the prover-provided
$p_i(u)$ values through the quotient equation.

## Split-accumulation for PCS Batched Evaluation

The split-accumulation scheme for batched polynomial evaluation wraps the PCS
aggregation technique to conform with the [2-arity PCD
syntax](./index.md#2-arity-pcd).

### Idea

We first present a simplified single-curve version allowing non-native arithmetic
to convey the core idea, then explain Ragu's adaptation when implementing over
[a cycle of curves](./index.md#ivc-on-a-cycle-of-curves).

Consider folding PCS evaluation claims from a NARK instance $\pi.\inst$ into an
accumulator $\acc_i$:

$$
\begin{cases}
\pi.\inst = \Bigg(\begin{array}{l}
  (\bar{A}, 0, 1),(\bar{A}, x, a(x)), (\bar{A}, xz, a(xz)),\\
  (\bar{B}, x, b(x)),\\
  (S, x, s(x,y)),\\
  (K, 0, 1), (K, y, c) \in\G\times\F^2
\end{array}\Bigg)\\
\pi.\wit = (\v{a},\v{b},\v{s},\v{k}\in\F^{4n})\\
\end{cases}

\begin{cases}
\acc.\inst=(\bar{P}\in\G, u,v\in\F) \\
\acc.\wit=(\v{p}\in\F^{4n},\gamma\in\F)
\end{cases}
$$

The accumulation prover:
1. Parses all evaluation claims from both the accumulator and NARK proof as
  $\big[(\bar{C}_i, x_i, y_i)\big]_i$, along with the underlying polynomials
  and blinding factors $\big[(p_i(X), \gamma_i)\big]_i$
2. Runs the [PCS aggregation](#pcs-aggregation) protocol on all claims
3. Outputs $\acc_{i+1}.\inst:=(\bar{P}',u',v')$ as the batched claim,
  $\acc_{i+1}.\wit:=(\v{p}',\gamma')$ as the batched polynomial and blinding
  factor, and $\pf_{i+1}:=(\bar{F}\in\G, [p_i(u)]_i)$ containing all prover
  messages from the aggregation transcript

The accumulation verifier parses the same claims and executes the verifier side
of the PCS aggregation. The bottleneck is Step 7: deriving $\bar{P}'$ requires
non-native scalar multiplications while computing $v'$ uses native field
operations.

### Ragu Adaptation

As [previously noted](./index.md#admonition-split-up-of-the-folding-work-title),
Ragu splits the folding work across the cycle to eliminate non-native arithmetic
from the in-circuit verifier. Specifically, the primary merge circuit
$CS_{merge}^{(1)}$ over $\F_p$:

- Folds claims from $\pi_{i,L/R}^{(1)}.\inst, \acc_{i,L/R}^{(1)}.\inst$
  to enforce the correct value $\acc_{i+1}^{(1)}.v\in\F_p$
- Folds claims from $\pi_{i,L/R}^{(2)}.\inst, \acc_{i,L/R}^{(2)}.\inst$
  to enforce the correct commitment
  $\acc_{i+1}^{(2)}.\bar{P}\in\G^{(2)}\subseteq\F_p^2$

Effectively, the accumulation work [described above](#idea) (for both
$\acc_i^{(1)}$ and $\acc_i^{(2)}$) is split between the two merge circuits.
Fold the evaluations in its field-native circuit while folding the commitments
in the other merge circuit. This cross-circuit splitting begets two challenges:

1. two merge circuits must access the same instances
  $\inst_{i,L/R},\acc_{i,L/R}$ containing both group elements and field elements
  from two curves (there contains elements in all of $\F_p,\F_q,\G_p,\G_q$) 
2. verifier challenges $\alpha,\beta$ used in the random linear combination of
  commitments and evaluations must be consistent across circuits

For challenge #1, Ragu uses **nested commitments in the preamble stage**.
We will discuss the [staging concept](../../extensions/staging.md) later. Think of
the "preamble stage" as the first internal step of the merge routine for now.
Naively, we can hash all elements in $\inst_{i,L/R},\acc_{i,L/R}$ on both
circuits, mark the digest as a public input, and enforce their equivalence.
However, directly accessing the input instance values inevitably requires
non-native arithmetic again. Instead, Ragu uses
[nested commitments](../../prelim/nested_commitment.md) to encode all instance
data in a form that each circuit can manipulate natively.

**Encoding instance elements**: Instance data contains elements from all four
algebraic spaces: $\F_p, \F_q, \G_p, \G_q$. In the preamble stage of the
primary circuit (over $\F_p$), each type is handled as follows:

- $\F_p$ scalars (e.g., native evaluations $\acc_{i+1}^{(1)}.v$): used directly
  as native field elements
- $\G_p$ points (e.g., commitments from primary circuit): nested committed
  using $\G_q$ generators, producing a $\G_q$ commitment with $\F_p$ coordinates
- $\F_q$ scalars (e.g., evaluations from secondary circuit like
  $\acc_i^{(2)}.v$): packed into a witness vector and committed using $\G_q$
  generators, producing a $\G_q$ commitment with $\F_p$ coordinates
- $\G_q$ points (e.g., commitments from secondary circuit): used directly,
  as their coordinates are in the native base field $\F_p$

The secondary circuit symmetrically handles its instance data using $\G_p$
commitments. All resulting commitments (polynomial commitments, nested
commitments encoding foreign values) are aggregated into a single preamble
commitment (`nested_preamble_commitment`) which becomes part of each merge
circuit's public instance
(i.e. $\inst_{merge,i}^{(1)}$ and $\inst_{merge,i}^{(2)}$).

**Equivalence check mechanism**: Rather than directly comparing digests across
circuits, equivalence is enforced through the recursive verification structure.
At step $i$, the primary circuit computes
$\mathsf{nested\_preamble\_commitment}_i^{(1)} \in \G_q$ over instance data from
step $i-1$, while the secondary circuit independently computes
$\mathsf{nested\_preamble\_commitment}_i^{(2)} \in \G_p$ over the same
underlying data. At step $i+1$, each circuit recursively verifies the other's
proof from step $i$, checking that the preamble commitment was correctly
computed. This creates a "chain of custody" ensuring both circuits committed to
the same instance data, without either circuit performing non-native arithmetic
to access the other's field elements directly.

For challenge #2,
Ragu **uses challenges squeezed from $\F_p$ transcript on the primary half
for both merge circuits**. After applying Poseidon hash function to the verifier
transcript to get the next random oracle output $s\in\F_p$, we set the next
verifier challenge as its extracted [endoscalar](../../extensions/endoscalar.md)
$\endo{s}:=\mathsf{extract(s)}\in\{0,1\}^\lambda\subset \F_p$. This supports both
native scalar arithmetic $\endo{s}\cdot c\in\F_p$ in the primary circuit and also
native scalar multiplication (a.k.a. _endoscaling_) $\endo{s}\cdot P\in\G^{(1)}$
in the secondary circuit. This challenge sharing trick is safe because:
inputs to merge circuits are the same set of instances
$\{\inst_{i,L/R},\acc_{i,L/R}\}$, _and_ challenges generated in the current step are
used to accumulate proofs from the previous step whose instances are already
committed in a binding manner.
