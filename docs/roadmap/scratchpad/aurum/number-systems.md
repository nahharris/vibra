# Number systems

The prospectus refuses a universal machine word. It proposes that the radix
in which a number is written, the unit it is measured in, and the algebra
its values live in are independent choices, and it works three examples: a
π-based domain, the golden ratio, and the twelfth roots of unity. This note
checks the mathematics, separates the claims that the literature supports
from the one it does not, and says how numeric domains reach Vibra without
changing its scalar types.

## Theory

Two different mathematical objects are in play, and most of the confusion in
this area comes from treating them as one.

- **A positional system in base β** writes a real number as a string of
  digits weighted by powers of β. For a base that is not an integer these
  are the β-expansions of Rényi and Parry [Renyi57] [Parry60], and base φ
  is the best-known example [Bergman57]. The
  questions are about digit strings: which are admissible, how redundant the
  system is, and whether addition can be computed by a local function of the
  digits.
- **A ring of algebraic numbers** such as Z[φ] or Z[ζ₁₂] represents a value
  exactly as a short tuple of ordinary integers, the coefficients over a
  basis. The questions are about algebra: closure, exact equality, and how
  fast the coefficients grow.

The pair (a, b) standing for a + bφ is the second kind of object. Its
components are binary integers, and nothing about it is "base φ". The
prospectus's independence principle is right, and it applies to its own
examples: the benefits it claims for golden-ratio and cyclotomic arithmetic
come from the ring, and none of them needs an exotic radix underneath.

## The prospectus's mathematics

Every identity the prospectus states was checked by hand and holds.

| Claim | Check |
| --- | --- |
| In base φ, `100 = 011`, and 2φᵏ = φᵏ⁺¹ + φᵏ⁻² | Both follow from φ² = φ + 1 |
| (a + bφ)(c + dφ) = (ac + bd) + (ad + bc + bd)φ | Expands with φ² = φ + 1; three integer multiplications suffice |
| Multiplying by φ maps (a, b) to (b, a + b); dividing maps (a, b) to (b − a, a) | Direct; these are a shift of the Fibonacci recurrence |
| The conjugate of a + bφ is (a + b) − bφ | The other root is 1 − φ |
| Φ₁₂(x) = x⁴ − x² + 1, and multiplying (a, b, c, d) by ζ gives (−d, a, b + d, c) | ζ⁴ = ζ² − 1 |
| Base π uses the digits 0 to 3 | ⌊π⌋ = 3 |
| Base −1 + i represents the Gaussian integers with digits 0 and 1 | Penney's system [Penney65] |

## Parallel addition, and hypothesis H2

H2 says that redundant golden-ratio arithmetic shortens carry propagation.
The relevant theory is complete enough to predict the outcome.

Addition is **parallel** when each output digit is a function of a bounded
window of input digits, so that the time to add does not depend on the
length of the numbers. Frougny, Pelantová, and Svobodová characterize when
this is possible: an algebraic base admits parallel addition on some digit
alphabet exactly when it has no conjugate of modulus one [FPS11]. The golden
ratio qualifies. What matters is the price.

| System | Digits | Window | Source |
| --- | --- | --- | --- |
| Base 3 and above, signed digits | Symmetric around zero | 2 | [Avizienis61] |
| Base 2, signed digits | {−1, 0, 1} | 3 | [ChowRobertson78] |
| Base 2, carry-save | {0, 1, 2} | Constant | Standard practice |
| Base φ | {−5, …, 5} | See source | Algorithm I of [FPS11] |
| Base φ | {−3, …, 3} | See source | Algorithm II of [FPS11] |
| Base φ, minimal alphabet | {−1, 0, 1} | 21: ten digits either side | Algorithm III of [FPS11]; minimality in [FPS16] |
| Base −1 + i, minimal alphabet | Five digits | See source | [FPS16] |

Three digits is the least any parallel adder for base φ can use [FPS16], and
at that size the adder reads twenty-one digits to produce one. Binary on
the same three digits reads three.

Information density adds to the gap. A base-φ digit carries log₂ φ ≈ 0.694
bits, so the range of a 64-bit integer takes 93 digits. With three-valued
digits at two wires each that is 186 wires, against 128 for binary signed
digits.

**Prior expectation: H2 fails.** The fair control for H2 is not
ripple-carry binary. It is redundant binary, and redundant representations
have given constant-time addition since the 1960s. Against that control,
base φ loses on window size and on wires, by theory, before any circuit is
built.

Two things could still make base φ interesting, and neither is speed.

- **Structural error detection.** The admissible strings of a β-expansion
  form a restricted language, so some corruptions are visible as
  inadmissible strings. Recent work finds the benefit narrow: a single
  corrupted digit cannot be semantically repaired without outside
  information, and what the redundancy gives is detectability
  [TaizhanovSeilkhan26].
- **Multiplication by φ is a shift.** That is true and useful only for
  workloads dominated by scaling by φ, and the ring representation gets the
  same operation from one addition.

**Recommendation.** Keep the experiment, because it is cheap in an emulator
and the prospectus values falsifiability. Register the control and the
expected negative result in advance, and do not let any fabric decision
depend on it. Represent golden-ratio values as the ring Z[φ] over binary
integers.

## The three example domains

**π.** No integer above 3 has a finite expansion in base π: a finite
expansion would be a polynomial equation with integer coefficients that π
satisfies, and π is transcendental [Lindemann82]. Digit arithmetic in base π
is therefore arithmetic on infinite strings. The prospectus's other
formulation is the workable one: Q[π, π⁻¹], the Laurent polynomials in π
with rational coefficients. Because π is transcendental this ring is
isomorphic to Q[x, x⁻¹], so it is ordinary symbolic polynomial arithmetic
with a named unit. Equality is decidable by comparing coefficients, and
ordering is decidable too: a nonzero element is never zero at π, so refining
an interval around π always terminates with a sign. The boundary is close
by. Add e and the algebraic independence of the two constants is an open
problem; add elementary functions and zero-testing becomes undecidable
[Richardson68].

**φ.** Z[φ] is the ring of integers of Q(√5). It is exact for golden-ratio
geometry: pentagonal symmetry, quasicrystal tilings, Fibonacci structure.
Its cost is coefficient growth, since repeated multiplication grows the
integers like powers of φ.

**ζ₁₂.** Z[ζ₁₂] contains i and √3 and represents every rotation by a
multiple of 30° exactly. This kind of ring is in production use: exact
synthesis of Clifford+T quantum circuits computes in Z[1/√2, i], and a
unitary is exactly representable in that gate set if and only if its entries
lie in that ring [GilesSelinger13]. Computational geometry reaches the same
goal by another route, adaptive-precision predicates that are exact without
exact numbers [Shewchuk97], and that is the baseline an exact-ring geometry
kernel has to beat for H3.

## The other engines in the prospectus

| Engine | What it buys | What it costs | Evidence |
| --- | --- | --- | --- |
| Residue number systems | Addition and multiplication with no carries between residues | Comparison, sign, overflow detection, and division are hard, which confines it to fixed pipelines | [Garner59] |
| Logarithmic number systems | Multiplication, division, and powers become addition and shifts | Addition needs a nonlinear function, by table or approximation | A fabricated processor [ELM08]; for 8-bit neural-network training, over 90% less energy than 32-bit floats and 55% less than 8-bit floats, from post-synthesis modelling rather than silicon [LNSMadam22] |
| Posits | Tapered precision and a single exception value | Variable-width fields make the hardware larger than a float of the same width, and the numerical advantage is disputed | [Posit17] [PositCritique19] |
| Zeckendorf codes | Unique representation as non-adjacent Fibonacci numbers | Density of 0.694 bits per digit | [Zeckendorf72] |

The logarithmic system is the engine with the best-supported benefit, for
the prospectus's AI workloads. Its result is specific:
low-precision multiply-accumulate, with the accumulation and the conversion
back to linear form as the costs to engineer away.

## Numeric domains in Vibra

Vibra's scalar types are the fixed-width integers, two IEEE floats, and a
few others. The prospectus's `Int` and `Golden64` are illustrative. Nothing
has to be added to the scalar set to host other domains, for three reasons.

- **There are no infix operators.** Arithmetic is spelled `U64.add(a b)`.
  A library type's `Golden.mul(a b)` reads the same way, so a new numeric
  domain costs no syntax and raises no question of operator overloading.
- **A domain is a wrapper with an invariant.** A golden integer is a
  wrapper over a pair of signed integers; a cyclotomic integer wraps four; a
  Zeckendorf code wraps an unsigned integer with a `where` invariant that no
  two adjacent bits are set. Each is ordinary library Vibra and runs on the
  interpreter and on native code unchanged.
- **Overflow already has a policy.** Exact rings grow their coefficients.
  Vibra arithmetic saturates or returns `Result`, and neither silent
  wrap-around nor a trap exists. An exact domain has to return `Result` from
  the operations that can overflow, because a saturated coefficient is no
  longer exact. The prospectus's example contract, which requires that a sum
  of coefficients fits in 64 bits, is a bare `requires`; it is accepted only
  where the checker's integer facts can prove it at each call.

The last point is a real constraint. Vibra has no arbitrary-precision
integer. An exact-arithmetic library needs either bounded domains with
checked operations or a big-integer type built from `List`, and the second
is outside the static fragment.

On the fabric, a library domain is a composite value made of integer nodes,
and it needs no special support to be correct. A fabric that implements a
domain's operations as single wide nodes is an optimization the program
cannot observe. Recognizing a library type in the compiler would need a new
closed language role, which is a specification change; nothing here
justifies one yet.

**Recommendation.** Write the numeric domains as a pure Vibra package first.
That tests H3 in software, on the existing executors, with no hardware and
no language change, and it produces the benchmark programs the fabric
experiments need.

## Floating point

`F32` and `F64` are IEEE with a canonical NaN, and executors must agree on
results. An Aurum fabric would need bit-exact IEEE units to accept them.
That is a large node for a domain the prospectus is trying to move away
from, so the first Aurum profiles should report floating point unavailable.
In co-processor mode the affected functions stay on the processor.

[Renyi57]: bibliography.md#number-systems-and-exact-arithmetic
[Parry60]: bibliography.md#number-systems-and-exact-arithmetic
[Bergman57]: bibliography.md#number-systems-and-exact-arithmetic
[Penney65]: bibliography.md#number-systems-and-exact-arithmetic
[FPS11]: bibliography.md#number-systems-and-exact-arithmetic
[FPS16]: bibliography.md#number-systems-and-exact-arithmetic
[Avizienis61]: bibliography.md#number-systems-and-exact-arithmetic
[ChowRobertson78]: bibliography.md#number-systems-and-exact-arithmetic
[TaizhanovSeilkhan26]: bibliography.md#number-systems-and-exact-arithmetic
[Lindemann82]: bibliography.md#number-systems-and-exact-arithmetic
[Richardson68]: bibliography.md#number-systems-and-exact-arithmetic
[GilesSelinger13]: bibliography.md#number-systems-and-exact-arithmetic
[Shewchuk97]: bibliography.md#number-systems-and-exact-arithmetic
[Garner59]: bibliography.md#number-systems-and-exact-arithmetic
[ELM08]: bibliography.md#number-systems-and-exact-arithmetic
[LNSMadam22]: bibliography.md#number-systems-and-exact-arithmetic
[Posit17]: bibliography.md#number-systems-and-exact-arithmetic
[PositCritique19]: bibliography.md#number-systems-and-exact-arithmetic
[Zeckendorf72]: bibliography.md#number-systems-and-exact-arithmetic
