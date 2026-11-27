# Number systems

The prospectus refuses a universal machine word. It proposes that the radix
in which a number is written, the unit it is measured in, and the algebra
its values live in are independent choices, and it works three examples: a
π-based domain, the golden ratio, and the twelfth roots of unity. This note
checks the mathematics, separates the claims that the literature supports
from the one it does not, and says how numeric domains reach Vibra without
changing its scalar types. It then takes up a question about Vibra itself:
whether an exact rational type should sit beside the IEEE floats.

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
is outside the static fragment. A rational type cannot take the bounded
route, as [the next section](#exact-rationals-beside-the-floats) shows.

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

## Exact rationals beside the floats

The proposal is that a quantity which is not whole is exact unless the
program says otherwise. Naturals, integers, and rationals are the exact
numbers. The algebraic domains above extend them where a workload needs
more. Approximation is something a program asks for by name. `F32` and
`F64` stay as they are, for compatibility.

### Theory

- **ℚ is an ordered field with a normal form.** It is closed under the four
  operations except division by zero, and every element has exactly one
  representation in lowest terms with a positive denominator. Equality and
  order are therefore decidable, and equal values have equal
  representations.
- **A finite float is a dyadic rational**, an integer times a power of two.
  The IEEE types add two infinities, the NaNs, and a signed zero to that
  subset, and they round after every operation. Approximation is not what
  is wrong with them. The rounding that nobody wrote and the values that
  are not numbers are.
- **The best approximations to a number by fractions of bounded denominator
  are the convergents of its continued fraction** [Khinchin64]. So "a
  rational close to this value" has a canonical answer for each bound.
- **Exactness can be a property of a number's kind.** Scheme separates
  exact numbers from inexact ones and makes a program convert between them
  on purpose [R7RS].

### The types

The names are illustrative.

| Type | Values | Its one job |
| --- | --- | --- |
| Unbounded natural and integer | ℕ and ℤ | Exact whole numbers with no overflow case |
| `Rational` | ℚ, in lowest terms | The default for any quantity that is not whole |
| `F32`, `F64`, unchanged | IEEE 754 | Interchange with formats and systems that use IEEE, and approximation at a fixed cost |
| The domains above | ℚ extended by φ, π, or a root of unity | Exactness beyond ℚ |

Each type has one job, so the float and the rational are not two spellings
of one idea.

### What the specification already provides

- **Nothing mixes silently.** "The floating-point types convert to and from
  other number types through no interface." No rule of contagion between
  exact and inexact numbers has to be designed.
- **A rational can be what a float cannot.** "The float types implement
  neither `Equatable` nor `Ordered`, because IEEE equality is not
  reflexive", and they are not admissible dictionary keys. A rational in
  lowest terms is all three.
- **`Rational` is a library type.** "Vibra is library-first. The compiler
  owns only what the language cannot build for itself". A rational is a
  wrapper over two integers whose `where` invariant says the denominator is
  positive and the pair is in lowest terms. Only the unbounded integer is a
  candidate for the compiler to own.
- **Conversion has an obvious shape.** From a float to a rational is exact
  for every finite value, fails for the infinities and NaN, and maps
  negative zero to zero. From a rational to a float is a rounding, and
  gets a name that says so.

### Approximation as an operation

- **Inside ℚ.** Two rounding operations cover the uses: to the nearest
  fraction under a denominator bound, which is the continued-fraction
  answer, and to the nearest multiple of a power of two, which is a shift.
- **Leaving ℚ.** A square root, an exponential, or a sine of a rational is
  usually irrational. Such a function takes a tolerance and returns a
  rational within it.
- **Executors agree for free.** These functions are ordinary Vibra over
  exact integers, so the interpreter, native code, and a fabric compute the
  same rational with no rule written for the purpose. For floats this is
  the expensive part. The runtime chapter gives the float types `add`,
  `sub`, `mul`, `div`, and `neg`, and no square root or transcendental
  function.

### Literals

"An unsuffixed literal takes its type from its expected type", among the
builtin number types. A library `Rational` receives the literal `2.5`
exactly only if it becomes a builtin, or if a general rule lets a library
type accept a decimal literal. Without either, a rational is written as a
constructor call or parsed from text.

### What it costs

- **Denominators grow.** Adding 1/1, 1/2, 1/3, and so on overflows a pair
  of 64-bit integers at the 47th term. Newton's iteration for √2 from 1
  doubles the size of its numerator every step and passes 64 bits at the
  sixth. With fixed-width parts, every operation returns `Result` and
  ordinary programs soon take the failing case. The proposal therefore
  needs unbounded integers.
- **Unbounded integers are a language decision.** Their operations are not
  constant-time, so tool limits that count work have to charge by size.
  They allocate, so they are outside the
  [static fragment](vibra-target.md#two-fragments). They add no new kind of
  failure, because memory exhaustion is already a host event.
- **Iterative numerics need rounding placed by hand.** A simulation or a
  linear solver over exact rationals grows without bound unless every step
  rounds. Rounding each step to a fixed number of significant bits is
  floating point written out by hand, and that workload is what `F64` is
  kept for.
- **The specification does not shrink.** NaN canonicalization, signed zero,
  total comparison, and the canonical float text all remain.
- **Habit.** Agents write `F64` because the code they learned from does.
  The counterweight is a standard library that takes and returns rationals
  for ratios, durations, percentages, and parsed decimals. Whether that is
  enough is a question for a pilot.

### Precedents

| System | Choice |
| --- | --- |
| Scheme | Exact integers and rationals beside inexact reals, with explicit conversion [R7RS] |
| Android's calculator | Rationals and a few symbolic constants where equality is decidable, with computable reals behind them [Boehm20] |
| Pyret | Exact rationals by default; an approximate number is marked as one in source [Pyret] |
| Raku | Rationals for decimal literals, falling back to floats without notice when a denominator outgrows 64 bits [Raku] |

Raku's fallback is the outcome that unbounded integers avoid.

### The domains depend on it

The π domain is Q[π, π⁻¹], whose coefficients are rationals, and division
in the golden field Q(√5) needs them too. `Rational` is a prerequisite of
the prospectus's own examples. A domain for e has the same shape as the
one for π, because e is transcendental as well [Hermite73]. The two
constants must not share a type, for the reason given
[above](#the-three-example-domains): their algebraic independence is
unproven.

**Recommendation.** Add `Rational` as a library type and make it the
standard library's default for quantities that are not whole. Keep the
floats unchanged. Both steps wait on the decision about unbounded integers,
which is [open](open-questions.md#37-unbounded-integers).

## Floating point

`F32` and `F64` are IEEE with a canonical NaN, and executors must agree on
results. An Aurum fabric would need bit-exact IEEE units to accept them.
That is a large node for a domain the prospectus is trying to move away
from, so the first Aurum profiles should report floating point unavailable.
In co-processor mode the affected functions stay on the processor.

With rationals in the language this division is convenient. The exact
types are composites of integer nodes and run on the fabric. The type kept
for compatibility with conventional machines is the one a fabric profile
rejects.

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
[Khinchin64]: bibliography.md#number-systems-and-exact-arithmetic
[R7RS]: bibliography.md#number-systems-and-exact-arithmetic
[Pyret]: bibliography.md#number-systems-and-exact-arithmetic
[Raku]: bibliography.md#number-systems-and-exact-arithmetic
[Hermite73]: bibliography.md#number-systems-and-exact-arithmetic
[Boehm20]: bibliography.md#interaction-live-environments-and-agents
