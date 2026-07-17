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

transition  = "on" IDENT "from" ( IDENT | "*" ) "->" IDENT
              [ "when" guard_expr ]
              [ "{" { lifecycle } "}" ]

lifecycle   = ("on_exit" | "on_transition" | "on_enter")
              ":" IDENT { "," IDENT }

reaction    = "reaction"
              "{" "when" IDENT "enters" IDENT "->" IDENT IDENT
              [ "when" guard_expr ] [ "with" "{" ... "}" ] [ "{" "}" ] "}"

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
- A **lifecycle hook** binds one or more action ids, comma-separated
  (`on_transition: x, y, z`). They run in the order written. Declaring the same
  phase twice in one block is an error.
- A **reaction** may carry a static payload block (`with { ... }`); see below.

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
| `on E from * -> B { ... }` (wildcard)       | expands to one `transitions[]` per source state (incl. `B -> B` self-loop) |
| `on E ... when G` (transition guard)        | `transitions[].guard` (string, verbatim)                   |
| lifecycle hook `on_exit: x, y`              | `transitions[].actions.on_exit[]` (in declaration order)   |
| lifecycle hook `on_transition: x, y`        | `transitions[].actions.on_transition[]`                    |
| lifecycle hook `on_enter: x, y`             | `transitions[].actions.on_enter[]`                         |
| `reaction { when S enters ST -> T EV }`    | `reactions[]`: `from_signal`, `from_state`, `to_signal`, `event` |
| `reaction { ... when G }` (reaction guard)  | `reactions[].guard` (string, verbatim; evaluated at cascade) |
| `reaction { ... with { ... } }` (payload)   | `reactions[].payload` (JSON `Value`; the derived event's payload) |
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

### Wildcard `from *` (M34)

A transition's `from` may be the wildcard `*`:

```ddl
on reset from * -> closed {
    on_transition: clear_fault_safely
    on_enter: log_reset
}
```

The compiler lowers it to one transition per source state, so an `N`-state
signal yields `N` transitions `{closed,open,fault} -> closed` — including the
`closed -> closed` self-loop. All arms share the same `event`/`to`/`actions`/
`guard`. The engine matches on `t.from == signal.current || t.from == "*"`,
so the self-loop is harmless and is, in fact, the proof that the wildcard
matches the **current** state rather than acting as a no-op. This mirrors the
JSON path (`transition.from == "*"` matches any current state at runtime) and
lets a single DDL line replace the hand-expanded three-line form.

### Multi-action lifecycle hooks (M34)

Each lifecycle hook binds a comma-separated list of action ids, evaluated in
the order written. The engine runs the phases in the fixed order
`on_exit` → `on_transition` → `on_enter` and, within each phase, in the order
declared (`schema::ActionBinding::all_actions`):

```ddl
on open from closed -> open {
    on_transition: activate_motor, warm_up
    on_enter: log_open
}
```

Declaring the same phase twice in a single block is a compile-time error.

### Reaction static payload (M34)

A reaction may carry a static payload delivered as the **derived event's**
payload to the target signal:

```ddl
reaction {
    when order enters approved -> inventory allocate
        when payload.auto == true
        with { "auto": true, "skip_reserve": true }
}
```

The `with { ... }` block is optional. The compiler parses it to a JSON value
and stores it in `reactions[].payload`; a malformed block is a compile-time
error. When the cascade fires, the engine delivers this payload to the target
(e.g. `send_event("inventory", "allocate", Some({"auto":true,...}))`).

Note the two payloads are distinct: the reaction's **guard** is evaluated
against the *source* event's payload (the `send_event` that triggered the
transition the reaction reacts to — the same rule as a transition guard), while
the reaction's **static payload** (`with { ... }`) rides on the *derived* event
to the target.

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
| `... 'from' state 'X' is not in the states list for 'S'` (for `from *`) | `*` is the only wildcard; other names must be in `states`. |
| `line L col C: 'when' requires a guard expression`                      | Empty `when` with no expression.               |
| `reaction payload is not valid JSON: ...`                               | The `with { ... }` block isn't valid JSON.    |
| `Failed to compile '...': line L col C: unterminated string literal`    | A `'...'` string wasn't closed.                |

All error messages carry `line`/`col` pointing at the offending token. The
parser validates incrementally and stops at the first problem, so fix them one
at a time. Guard-syntax mistakes surface only when the **engine** evaluates the
guard at runtime (as `GuardEvaluationError` / `GuardBlocked`), since the DDL
compiler passes guards through verbatim.

## Linting with `stc --check`

`stc` accepts an optional `--check` flag that runs semantic checks over the
compiled topology **before** the JSON is emitted. It prints any warnings to
stderr and writes the JSON as usual — warnings are **non-blocking** (they never
abort the run and never change the exit code):

```
stc [--check] <input.ddl> [output.json]
#   no output path  -> pretty JSON on stdout (+ warnings on stderr)
#   output path     -> JSON to the file, `Compiled ...` on stdout, warnings on stderr
```

The checks are pure functions of the compiled `TopologySchema` (see
`src/check.rs`); they do not depend on the DDL AST, so they tolerate the
compiler's lowering faithfully.

### Checks

| Warning              | What it means                                                                              | Example                                                                                       |
|----------------------|--------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------|
| `self-loop`          | A transition with `from == to`; the engine stays in the same state.                        | `gate_flow`'s `on reset from * -> closed` lowers to a `closed -> closed` self-loop (harmless, but worth knowing). |
| `unreachable-state`  | A signal state that is neither the initial state nor any other state's `to` target — dead. | An `obsolete` state that no transition ever enters.                                           |

The `--check` output format:

```
Warning: self-loop: gate: closed -> closed
1 warning(s) found.
```

### Exit code

Warnings are informational only: `stc --check` always exits `0` when the
DDL compiles. Only a **compile error** (syntax, validation) exits non-zero.
Use `--check` with no output path for a lint-only run:

```
cargo run --bin stc -- --check path/to/file.ddl
```

### How to read a self-loop warning

A self-loop is not always a mistake. Two common sources:

- **Literal self-loop** — you wrote `on ev from a -> a` directly.
- **Wildcard lowering** — `on ev from * -> x` expands to one transition per
  source state, producing an `x -> x` arm when `x` is itself a state. The
  compiler emits `from`/`to` as concrete state names, so the schema alone
  cannot tell these apart; `--check` reports the `from == to` pair either way.

If the self-loop is intentional (the `gate_flow` reset is), ignore the warning.
If it is not, either remove the arm or rewrite the wildcard so its `to` is not
in the source-state set.
