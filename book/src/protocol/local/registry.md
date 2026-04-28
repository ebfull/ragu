# Registry

The [overview](overview.md) presents a given circuit's $j$-th linear constraint
as a structured polynomial $\v{s}_j$ whose revdot with the trace recovers the
constraint: $\revdot{\v{r}}{\v{s}_j} = \v{k}_j$. Collecting all $4n$ constraints
with a formal indeterminate $Y$ gives the **wiring polynomial**

$$
s(X, Y) = \sum_{j=0}^{4n-1} Y^j \left(\sum_{i=0}^{n-1} \left(
  w_{i,j}\, X^{i} +
  v_{i,j}\, X^{2n-1-i} +
  u_{i,j}\, X^{2n+i} +
  d_{i,j}\, X^{4n-1-i}
\right)\right)
$$

where $w_{i,j}$, $v_{i,j}$, $u_{i,j}$, $d_{i,j}$ are the scalar entries of the
$j$-th constraint's structured regions.[^w-zero] $X$ encodes gate structure, $Y$
separates constraints. Evaluating at $Y = y$ yields the polynomial $s(X, y)$,
whose structured coefficient vector is the $Y$-batched vector $\v{s}_y$ from the
overview:

$$
\v{s}_y = \sum_{j=0}^{4n-1} y^j \cdot \v{s}_j
$$

The combined check

$$
\revdot{\v{r}}{\v{r} \circ \v{z^{4n}} + \v{t}_z + \v{s}_y} = \dot{\v{k}}{\v{y^{4n}}}
$$

is therefore parameterized by a single bivariate object $s(X, Y)$ per circuit.
The protocol never materializes $s(X, Y)$ as a full coefficient matrix — it
accesses partial restrictions through specialized evaluation
drivers.[^eval-drivers] Multiple circuits $s_0(X, Y), s_1(X, Y), \ldots$ coexist
within a single polynomial framework through the **registry polynomial** $m(W,
X, Y)$, a trivariate interpolation satisfying

$$
m(\omega^i, X, Y) = s_i(X, Y)
$$

where $\omega \in \F$ is a root of unity of order $2^k$,
$k = \lceil \log_2 C \rceil$, and $C$ is the number of registered circuits. The
combined check becomes circuit-dependent — $\v{s}_y$ is drawn from whichever
$s_i$ the prover claims to execute — and the registry is the mechanism that
binds the prover to a specific circuit description.

## Evaluation Restrictions

The registry is never materialized as a full trivariate polynomial. At most one
variable is left free at a time, yielding four **evaluation paths**:

| Path | Restriction | Free in | Output |
|------|-------------|---------|--------|
| `wx` | $m(w, x, Y)$ | $Y$ | unstructured polynomial |
| `wy` | $m(w, X, y)$ | $X$ | structured polynomial |
| `wxy` | $m(w, x, y)$ | — | scalar |
| `xy` | $m(W, x, y)$ | $W$ | unstructured polynomial |

The first three paths restrict $W = w$ and weight each circuit's contribution by
its Lagrange basis coefficient $\ell_i(w)$:

$$
m(w, \ldots) = \sum_{i=0}^{2^k - 1} \ell_i(w) \cdot s_i(\ldots)
$$

When $w = \omega^j$ is a domain element, $\ell_i(w)$ collapses to $\delta_{ij}$
and evaluation reduces to a direct lookup of circuit $j$. When $w$ is
out-of-domain, all circuits contribute with nonzero weights. The `xy` path
leaves $W$ free: it evaluates every circuit at the same $(x, y)$ and recovers
$m(W, x, y)$ as a polynomial in $W$.

These four paths mirror the bivariate restrictions of a single circuit's
$s(X, Y)$: `wx` computes $s_i(x, Y)$ per circuit and combines (the `sx`
driver), `wy` computes $s_i(X, y)$ per circuit and combines (`sy`), `wxy`
computes $s_i(x, y)$ per circuit and combines (`sxy`).

[^w-zero]: The $w$-region of every constraint is zero — constraint
    coefficients occupy only the $v$, $u$, and $d$ regions (the backward view).
    This complements the trace's zero $d$-region, ensuring the revdot
    cross-terms $\v{w}_r \cdot \v{d}_s + \v{d}_r \cdot \v{w}_s$ contribute
    nothing.

[^eval-drivers]: The three bivariate restrictions each have a specialized
    driver: `sx` fixes $X = x$ and returns a polynomial in $Y$ (coefficients
    are per-constraint evaluations), `sy` fixes $Y = y$ and returns a
    structured polynomial in $X$ (wire weights accumulated across all
    constraints), and `sxy` fixes both and returns a scalar via Horner
    accumulation. The registry's evaluation paths compose these per-circuit
    drivers with Lagrange interpolation in $W$.
