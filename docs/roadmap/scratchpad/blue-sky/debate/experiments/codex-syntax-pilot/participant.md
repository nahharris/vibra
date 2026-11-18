# Syntax pilot participant

## Metadata

- Same model family as the parent; exact model version unknown.
- One participant; fixed surface order: K, then P, then B.
- No blinding between this participant's own three completions.
- No compiler, execution, or evaluation results.
- Each surface below is its uncorrected first complete solution, saved once.
- This is an illustrative one-participant matched pilot, not proof that one syntax is superior.
- Interpretation: exactly three requested top-level functions per surface, with the explicitly required recursive `walk` helper nested inside `classify`. The card does not specify local function binding; these solutions assume a named function can be the value of `let` and can refer to itself recursively.

## Card

Common semantics: integers are mathematical; lists are immutable; comparisons are `lt`, `eq`; operations are `add`, `sub`, `div`; list operations are `list.length`, `list.get`, `list.append`; Result constructors are `ok(x)`, `error(name)`; lists are zero-based.

1. `safe-ratio(n,d)`: `error(zero)` when `d=0`, otherwise `ok(div(n,d))`.
2. `clamp(x,lo,hi)`: `error(bounds)` when `hi<lo`; otherwise `ok(lo)` if `x<lo`, `ok(hi)` if `hi<x`, and `ok(x)` otherwise.
3. `classify(xs)`: output a list of the same length; negative input maps to `-1`, zero to `0`, positive to `1`. Use recursive helper `walk(xs,i,out)` with explicit termination on `list.length` and `list.get` only in the in-bounds branch.

Surface P: `(fn name (arg...) (body))`; conditionals `(if condition then-expression else-expression)`; bindings `(let name value body)`; calls `(callee arg...)`; empty list `(list)`.

Surface B: `fn name(arg...) { expression }`; conditionals `if condition { expression } else { expression }`; bindings `let name = value; expression`; calls `callee(arg...)`; empty list `list()`.

Surface K: `fn name takes: arg... do: expression endfn`; conditionals `if condition then: expression else: expression endif`; bindings `let name be: value in: expression endlet`; calls `callee(arg...)`; empty list `list()`.

## K — first complete solution

```text
fn safe-ratio takes: n d do:
  if eq(d, 0) then:
    error(zero)
  else:
    ok(div(n, d))
  endif
endfn

fn clamp takes: x lo hi do:
  if lt(hi, lo) then:
    error(bounds)
  else:
    if lt(x, lo) then:
      ok(lo)
    else:
      if lt(hi, x) then:
        ok(hi)
      else:
        ok(x)
      endif
    endif
  endif
endfn

fn classify takes: xs do:
  let walk be:
    fn walk takes: xs i out do:
      if eq(i, list.length(xs)) then:
        out
      else:
        let x be: list.get(xs, i) in:
          let sign be:
            if lt(x, 0) then:
              -1
            else:
              if eq(x, 0) then:
                0
              else:
                1
              endif
            endif
          in:
            walk(xs, add(i, 1), list.append(out, sign))
          endlet
        endlet
      endif
    endfn
  in:
    walk(xs, 0, list())
  endlet
endfn
```

## P — first complete solution

```text
(fn safe-ratio (n d)
  (if (eq d 0)
    (error zero)
    (ok (div n d))))

(fn clamp (x lo hi)
  (if (lt hi lo)
    (error bounds)
    (if (lt x lo)
      (ok lo)
      (if (lt hi x)
        (ok hi)
        (ok x)))))

(fn classify (xs)
  (let walk
    (fn walk (xs i out)
      (if (eq i (list.length xs))
        out
        (let x (list.get xs i)
          (let sign
            (if (lt x 0)
              -1
              (if (eq x 0)
                0
                1))
            (walk xs (add i 1) (list.append out sign))))))
    (walk xs 0 (list))))
```

## B — first complete solution

```text
fn safe-ratio(n, d) {
  if eq(d, 0) {
    error(zero)
  } else {
    ok(div(n, d))
  }
}

fn clamp(x, lo, hi) {
  if lt(hi, lo) {
    error(bounds)
  } else {
    if lt(x, lo) {
      ok(lo)
    } else {
      if lt(hi, x) {
        ok(hi)
      } else {
        ok(x)
      }
    }
  }
}

fn classify(xs) {
  let walk = fn walk(xs, i, out) {
    if eq(i, list.length(xs)) {
      out
    } else {
      let x = list.get(xs, i);
      let sign = if lt(x, 0) {
        -1
      } else {
        if eq(x, 0) {
          0
        } else {
          1
        }
      };
      walk(xs, add(i, 1), list.append(out, sign))
    }
  };
  walk(xs, 0, list())
}
```

## Brief self-report and confounds

I used the same branching and recursive algorithm in all three surfaces. K required keeping the closing keywords aligned; P required keeping the parentheses aligned; B used braces and semicolons around the local bindings. These are subjective composition observations, not measured differences in effort or accuracy.

The fixed order and exposure to my own earlier solutions allow transfer between completions. The participant count is one, and there was no independent checking, timing, compilation, or execution. Local recursive function binding is an assumption beyond the card's explicit grammar, and the helper makes the literal total number of function forms four per surface despite the three requested top-level functions. I assumed `list.append(out, value)` appends one element to the end of a list. The examples therefore cannot establish a syntax winner or verified language conformance.
