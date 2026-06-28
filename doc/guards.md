# Guard Expressions

Guards are lightweight boolean expressions attached to transitions. A transition only executes when its guard evaluates to `true`.

## Evaluation Order

When `send_event` matches a transition:

1. Event is received and logged.
2. Transition is matched by `(signal_id, from, event)`.
3. If the transition has a `guard`, it is evaluated against the event payload.
4. If the guard is `false`, the engine returns `EngineError::GuardBlocked` and the signal state remains unchanged.
5. If the guard is `true` or absent, `on_exit`, state change, `on_transition`, and `on_enter` actions run as usual.

## Syntax

### Literals

- Integers: `100`, `-5`
- Floats: `3.14`
- Strings: `'USD'` (single quotes)
- Booleans: `true`, `false`

### Payload Access

Use `payload.<field>` to read the event payload. Nested objects are supported:

```text
payload.amount > 0
payload.user.is_admin == true
```

### Comparison Operators

- `==` equal
- `!=` not equal
- `<` less than
- `<=` less than or equal
- `>` greater than
- `>=` greater than or equal

### Logical Operators

- `and`
- `or`
- `not`

### Arithmetic Operators

- `+`, `-`, `*`, `/` on numbers

### Parentheses

Use parentheses to override precedence:

```text
(payload.amount > 0 and payload.currency == 'USD') or payload.vip == true
```

### Operator Precedence

Highest to lowest:

1. `not`, unary `-`
2. `*`, `/`
3. `+`, `-`
4. `==`, `!=`, `<`, `<=`, `>`, `>=`
5. `and`
6. `or`

## Missing Fields

Accessing a missing payload field evaluates to `null`. In boolean context `null` is `false`. Comparisons with `null` return `false` (except `null == null`, which is `true`).

## Error Semantics

- `GuardBlocked`: the guard expression is valid but evaluates to `false`. The state does not change and no actions run.
- `GuardEvaluationError`: the guard expression has a syntax error, references an unknown identifier, or compares incompatible types. The state does not change and no actions run.

## Example Topology

```json
{
  "version": "0.1",
  "signals": [
    {
      "id": "payment",
      "initial_state": "pending",
      "states": ["pending", "processed", "rejected"]
    }
  ],
  "transitions": [
    {
      "signal_id": "payment",
      "from": "pending",
      "event": "process",
      "to": "processed",
      "guard": "payload.amount > 0 and payload.currency == 'USD'",
      "actions": {
        "on_enter": ["mark_processed"]
      }
    },
    {
      "signal_id": "payment",
      "from": "pending",
      "event": "reject",
      "to": "rejected"
    }
  ]
}
```

## Debugging Guards

If a transition is unexpectedly blocked, inspect the guard expression and the payload. Enable trace logging to confirm the event was received before the guard was evaluated.
