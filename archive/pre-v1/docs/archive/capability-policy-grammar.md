---
title: Archived capability and policy grammar
category: archive
status: superseded
updated: 2026-08-07
---

# Archived capability and policy grammar

This document preserves the capability and policy passages that once appeared
in [`decisions/s-expression-language.md`](../decisions/s-expression-language.md).
The source-language authority system was decommissioned in #213. These forms
are historical context only; they are not accepted Vibra syntax or runtime
enforcement contracts.

## Former type and handle rationale

The former contract described `capability-type` and `policy-type` as part of the
authority system. The capability domain and handle access were positional
operands rather than part of a form head. The legacy surface fused them into
head-varying spellings such as `$capability.env-read` and `$handle.read`.

That rationale was retained here because it explains the rejected design, not
because those types still exist. The historical model described a capability
as carrying zero or more policy groups and a handle as carrying an access mode.
The current language has neither a source-level capability type nor a policy
value.

## Former `policy.narrow` grammar

The old expression production was:

```ebnf
policy-narrow = "(", "policy.narrow", expr, type-expr, ")" ;
```

`policy.narrow` was a distinct expression head rather than a spelling of
`cast`. It narrowed a `policy` value into a `capability` or a more specific
`policy` only when subsumption showed that the target authority was covered by
the source. The distinction kept newtype conversion separate from authority
checking, while target normalization and subsumption reused the relation
module shared by `cast` and `convert`.

This grammar and rationale are archived so future authority work can recover
the design context without making it look like a current language feature.
