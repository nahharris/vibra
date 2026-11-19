# Experiment X1: hidden-test encoding

Status: draft for Codex to confirm against the harness

One JSON object per line, one file per task (`tN.jsonl`).

```json
{"schema": 1, "id": "t1-003", "task": "t1", "function": "clamp",
 "args": [{"kind": "int", "value": "5"}, {"kind": "int", "value": "0"},
          {"kind": "int", "value": "3"}],
 "expect": {"outcome": "value", "value": {"kind": "int", "value": "3"}}}
```

## Values

| Kind | Encoding |
| --- | --- |
| int | `{"kind": "int", "value": "-12"}`: a canonical signed decimal string |
| bool | `{"kind": "bool", "value": true}` |
| text | `{"kind": "text", "value": "abc"}` |
| list | `{"kind": "list", "items": [ … ]}` |
| option | `{"kind": "variant", "type": "option", "case": "some", "fields": {"value": … }}` or the same with `"case": "none", "fields": {}` |
| record | `{"kind": "record", "type": "line", "fields": {"quantity": … , "price": … }}` |
| variant | `{"kind": "variant", "type": "shape", "case": "rect", "fields": {"width": … , "height": … }}` |
| wrapped | `{"kind": "wrapped", "type": "cents", "value": {"kind": "int", "value": "5"}}` |

## Expectations

| `outcome` | Meaning |
| --- | --- |
| `value` | The call returns exactly `value` |
| `requires-violated` | The call breaks the task's stated precondition. The submission must not return a value; a contract fault counts as correct |

A case passes when the outcome matches. A fault, a wrong value, a
missing function, or fuel exhaustion fails it. A task passes when every
case passes.

Types named in a case (`shape`, `cents`, and so on) are the ones the
task statement tells the participant to declare, with the field and
case names given there.
