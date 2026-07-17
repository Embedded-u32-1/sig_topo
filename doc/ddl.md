# Domain Description Language (DDL) — M28

DDL is a small, domain-facing language for describing signal topologies. You
write a `.ddl` file with **business semantics** (`signal`, `states`, `on`,
`when`, `reaction`) and compile it to the engine's JSON `TopologySchema` with
the `stc` tool. The engine itself is untouched — DDL is purely a front-end.

```
stc <input.ddl> [output.json]      # compile; prints to stdout if no output
```

The readable form is the `.ddl` file. The compiled JSON feeds the unmodified
engine and the rest of the tool chain (`sts` / `stt` / `stp` / `stv`).

## Quick example

This `examples/order_approval.ddl` is semantically equivalent to
`examples/order_approval.json`:

```ddl
// Order-approval signal.
signal order {
    states: [draft, submitted, approved, rejected, shipped]
    initial: draft

    on submit from draft -> submitted {
        on_exit: log_draft_exit
        on_transition: validate_order_payload
        on_enter: notify_submitted
    }

    on approve from submitted -> approved
        when payload.amount > 0 and payload.amount <= 100000 {
        on_transition: reserve_inventory
        on_enter: notify_customer_approved
    }

    on reject from submitted -> rejected {
        on_transition: release_hold
        on_enter: notify_customer_rejected
    }

    on ship from approved -> shipped {
        on_transition: dispatch_order
        on_enter: notify_shipped
    }
}

// Cross-signal cascade (reaction).
reaction {
    when order enters approved -> order_fulfill begin {}
}
```

## Syntax reference (EBNF)

```
doc         = { signal | reaction }

signal      = "signal" IDENT
              "{" states_decl initial_decl { transition } "}"

states_decl = "states" ":" "[" [ IDENT { "," IDENT } [ "," ] ] "]"

initial_decl= "initial" ":" IDENT

transition  = "on" IDENT "from" IDENT "->" IDENT
              [ "when" guard_expr ]
              [ "{" { lifecycle } "}" ]

lifecycle   = ("on_exit" | "on_transition" | "on_enter") ":" IDENT

reaction    = "reaction"
              "{" "when" IDENT "enters" IDENT "->" IDENT IDENT
              [ "when" guard_expr ] [ "{" "}" ] "}"

guard_expr  = <verbatim expression; see Guard grammar below>
```

Notes:

- **Comments**: `//` to end of line is skipped.
- **Identifiers** start with a letter or `_`; continue with letters, digits, or
  `_`. Keywords cannot be reused as identifiers in keyword positions (e.g. you
  cannot name a state `signal`), but the parser otherwise accepts them as names.
- **Bare transitions** (`on ev from a -> b` with no action block) are allowed
  and produce a transition with no lifecycle actions. The `{ }` block is
  optional.
- **Initial state** must be a member of the `states` list; `from`/`to` states
  must also belong to the list (`from` may be `*` for wildcard).

## Guard grammar

`when` clauses reuse the engine's existing guard expression syntax verbatim —
the DDL compiler does **not** interpret the guard, it passes the source text
straight through to `TransitionDef.guard`, which the engine evaluates at
runtime. Supported forms (copied from `doc/guards.md`):

| Category   | Syntax                                                        |
|------------|---------------------------------------------------------------|
| Literals   | integers, floats, `'strings'`, `true`, `false`                |
| Field read | `payload.field` (nested: `payload.a.b`)                       |
| Comparison | `==` `!=` `<` `<=` `>` `>=`                                   |
| Arithmetic | `+` `-` `*` `/`                                               |
| Logic      | `and` `or` `not`                                              |
| Grouping   | `(...)`                                                       |

Example: `payload.amount > 0 and payload.amount <= 100000`.

A guard that fails returns `EngineError::GuardBlocked`; the signal's state is
unchanged.

## DDL → JSON mapping

| DDL construct                               | JSON field(s)                                              |
|---------------------------------------------|------------------------------------------------------------|
| `signal S { states: [...] initial: I }`     | `signals[]`: `id`, `states`, `initial_state`               |
| `on E from A -> B { ... }`                  | `transitions[]`: `signal_id`, `event`, `from`, `to`, `actions` |
| `on E ... when G` (transition guard)        | `transitions[].guard` (string, verbatim)                   |
| lifecycle hook `on_exit: x`                 | `transitions[].actions.on_exit[]` (in declaration order)   |
| lifecycle hook `on_transition: x`           | `transitions[].actions.on_transition[]`                    |
| lifecycle hook `on_enter: x`                | `transitions[].actions.on_enter[]`                         |
| `reaction { when S enters ST -> T EV }`    | `reactions[]`: `from_signal`, `from_state`, `to_signal`, `event` |
| `reaction { ... when G }` (reaction guard)  | `reactions[].guard` (string, verbatim; evaluated at cascade) |
| (implicit)                                  | `version` = `"0.1"`                                        |

Mapping rules:

- Each `signal` block becomes one `SignalDef` plus one `TransitionDef` per
  `on` clause, all carrying the signal's `id` as `signal_id`.
- Lifecycle hooks preserve declaration order inside each phase, and the
  engine runs them in the fixed phase order `on_exit` → `on_transition` →
  `on_enter` (see `schema::ActionBinding::all_actions`). Declaring the same
  phase twice in one block is an error.
- An empty `reactions` list is omitted from the output entirely (matches the
  canonical examples).
- The DDL compiler never emits `components` / `instances` / `includes`.

## Reactions (cross-signal cascade)

A `reaction` declares: *when `from_signal` enters `from_state`, deliver
`event` to `to_signal`*. It maps 1:1 onto `ReactionDef`. The engine fires
matching reactions after the main transition commits; see `doc/cascades.md` and
`doc/transaction.md` for the cascade / rollback semantics (per-signal atomic,
already-committed ancestors retained on a later failure).

### Reaction guard (M32)

A `reaction` may carry a `when <guard>` clause. At cascade time the engine
evaluates the guard against the **source event's payload** — the payload of
the `send_event` call that triggered the transition the reaction reacts to —
exactly as a transition guard reads its own event's payload. The reaction's
static `payload` (the derived event's payload delivered to the target) is a
separate value and is *not* what the guard reads.

Semantics:

- guard evaluates to `true` (or is absent) → the cascade fires.
- guard evaluates to `false` → that reaction is skipped. The main transition
  has already committed, and the remaining reactions are untouched.
- guard fails to evaluate (e.g. a syntax error) → that reaction is skipped,
  not an error. A single bad guard never breaks the whole cascade chain.

```ddl
reaction {
    when order enters approved -> inventory allocate when payload.auto == true
}
```

Mapping: the guard lands verbatim in `reactions[].guard` and is evaluated by
`engine::send_event_internal`.

## Tool chain

```
order_approval.ddl ──stc──▶ order_approval.json ──▶ engine (sts/stt/stp/stv)
```

- `stc <in.ddl> [out.json]` — compile. No output path → pretty JSON on stdout.
- Errors are printed to stderr with a line/column and a non-zero exit; nothing
  panics.

## Troubleshooting

| Symptom                                                                 | Likely cause                                   |
|-------------------------------------------------------------------------|------------------------------------------------|
| `line L col C: expected 'signal' or 'reaction', found ...`              | Top-level block didn't start with a keyword.   |
| `line L col C: expected Identifier, found ...`                          | A keyword appeared where a name was needed.    |
| `line L col C: expected Arrow, found ...` (or `expected "->"`)          | Missing `->` between `from` and `to`.          |
| `... 'from' state 'X' is not in the states list for 'S'`                | `from`/`to`/state name not in `states: [...]`. |
| `... initial state 'X' is not in the states list`                       | `initial:` not a member of `states`.           |
| `... duplicate signal 'S'`                                              | Two `signal S` blocks.                         |
| `... duplicate 'on_exit' hook`                                          | Same lifecycle phase declared twice in a block.|
| `line L col C: 'when' requires a guard expression`                      | Empty `when` with no expression.               |
| `Failed to compile '...': line L col C: unterminated string literal`    | A `'...'` string wasn't closed.                |

All error messages carry `line`/`col` pointing at the offending token. The
parser validates incrementally and stops at the first problem, so fix them one
at a time. Guard-syntax mistakes surface only when the **engine** evaluates the
guard at runtime (as `GuardEvaluationError` / `GuardBlocked`), since the DDL
compiler passes guards through verbatim.
