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
doc         = { signal | reaction | guard | fork | join | component | instantiate }

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
              [ "when" ( guard_expr | IDENT ) ]
              [ "with" "{" ... "}" ] [ "{" "}" ]
              [ "on_fail" ":" IDENT ] "}"

guard       = "guard" IDENT "{" guard_expr "}"

fork        = "fork" "{" { reaction_body } "}"

join        = "join" IDENT "{" { reaction_body } "}"

component   = "component" IDENT
              "{" [ params_decl ] { port | signal | reaction | fork | join } "}"

params_decl = "params" ":" "[" [ IDENT { "," IDENT } [ "," ] ] "]"

port        = "port" direction IDENT "." IDENT [ "as" IDENT ]

direction   = "in" | "out" | "inout"

instantiate = "instantiate" IDENT "as" IDENT
              "{" { IDENT "->" IDENT } "}"
              [ "connect" "{" { IDENT "->" IDENT } "}" ]

reaction_body = "when" IDENT "enters" IDENT "->" IDENT IDENT
                [ "when" ( guard_expr | IDENT ) ]
                [ "with" "{" ... "}" ]
                [ "on_fail" ":" IDENT ]

note: inside a `fork`/`join` block each `reaction_body` is a self-contained
`when ... enters ... -> ...` clause (no wrapping `reaction { }`). The word
`when` that begins a fresh clause is distinguished from a `when` guard by the
`<ident> entres` shape — see "Fork / join (M44)".

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
- A **reaction guard** (`when ...`) may be either a literal expression or a bare
  identifier, which references a top-level `guard <id> { <expr> }` declaration
  (see "Guard templates" below). Forward references are allowed.

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
| `reaction { ... when ID }` (guard ref)      | `reactions[].guard` (the referenced guard's expr, inlined verbatim) |
| `reaction { ... with { ... } }` (payload)   | `reactions[].payload` (JSON `Value`; the derived event's payload) |
| `reaction { ... on_fail: A }` (M47)          | `reactions[].on_fail` (`Some("A")`; compensation action on cascade failure) |
| `fork { ... }` (M44) block members          | `reactions[].join_group` = the block's auto name (`fork0`, `fork1`, …) |
| `join <group> { ... }` (M44) block members  | `reactions[].requires` = `["<group>"]` |
| `guard ID { G }` (M38)                      | no direct JSON field; inlined into each `reactions[].guard` that refs it |
| `component C { ... }` (M45)                 | `components["C"]`: `params`, `ports`, `signals`, `transitions`, `reactions` |
| `port out S.ST [as A]` (M45)                | `components[].ports[]`: `direction`, `signal`, `state`, `alias` |
| `instantiate C as I with {...} connect {...}` (M45) | `instances[]`: `component`, `bindings`, `connections` |
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

### Guard templates (M38)

A guard expression can be written once and shared by many reactions via a
top-level `guard <id> { <expr> }` declaration:

```ddl
guard allow_alloc {
    payload.auto == true
}

signal order { states: [pending, approved] initial: pending on approve from pending -> approved }
signal inventory { states: [idle, allocated] initial: idle on allocate from idle -> allocated }
signal audit { states: [idle, noted] initial: idle on note from idle -> noted }

reaction {
    when order enters approved -> inventory allocate when allow_alloc
}
reaction {
    when order enters approved -> audit note when allow_alloc
}
```

`when allow_alloc` references the guard declaration by id. The compiler
**inlines** the referenced expression verbatim into each referencing reaction's
`reactions[].guard`, so both reactions above end up with the identical guard
text `payload.auto == true` — exactly as if it had been written out twice. This
gives single-source-of-truth guard conditions: change the declaration and every
referencing reaction follows.

Rules:

- A bare identifier after `when` is a reference; anything else (compound
  expression, literal, `payload.x`) is a literal guard. For example
  `when payload.auto` is a literal expression, while `when allow_alloc` is a
  reference.
- Forward references are allowed — a reaction may reference a guard declared
  later in the source.
- A reference to an undeclared guard id is a parse error.
- Duplicate guard ids are a parse error.
- The schema layer (`ReactionDef.guard`) never sees a bare reference id; the
  guard is always the expanded expression text, so the JSON/engine are
  unchanged.

The guard language already supports `and` / `or` / `not`, so compound conditions
need no extra syntax — `payload.auto == true and payload.cfg.enabled == true`
expresses composition directly.

### Guard evaluation trace (M38)

M29/M30 record actions and state changes, but until M38 a reaction guard was
silent: a `false` or failed guard just skipped the reaction with no trace. Now
every reaction guard evaluation emits a `ReactionGuardEvaluated` trace event:

```
[1784320531134] ReactionGuardEvaluated order.approved -> inventory.allocate guard=`payload.auto == true` result=true
[1784320531135] ReactionGuardEvaluated order.approved -> audit.note guard=`payload.auto == true` result=false
```

Fields: the reaction's `from_signal.from_state -> to_signal.event`, the guard
expression, and a `result` that is `"true"` (reaction fired), `"false"`
(reaction skipped), or `"error: <msg>"` (guard failed to evaluate, reaction
skipped). Together they answer "why did this reaction fire / not fire", and a
shared guard shows identical `result` values across the reactions that share it.

### Guard coordination scenario (M39)

`examples/scenarios/guard_coordination/` is the canonical "payment success ->
inventory decrement" teaching scenario for shared-guard coordination. A single
`guard canreserve { payload.amount <= 100 }` template is referenced by *two*
reactions (inventory reservation and audit clearing). Because both reactions
share the one guard, they cannot diverge: a small payment (guard true) fires
both, a large payment (guard false) skips both. The scenario is replayed by
`all_scenario_dirs_pass` (`tests/scenarios_test.rs`), so the shared-guard
consistency is covered by the automatic scenario regression suite.

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

### Reaction compensation (M47)

A reaction may name a compensation action to run when its cascade fails:

```ddl
reaction {
    when order enters approved -> inventory allocate
        when payload.auto == true
        on_fail: cancel_order
}
```

`on_fail: <action_id>` is optional. When the cascade the reaction triggers
fails (a lifecycle action in the target signal's derived transition fails, so
the target rolls back and the cascade error propagates), the engine runs the
named action **before** propagating that error upward. The action is run with
the failure message carried in its `ActionContext.failure` field, so a
compensation hook can learn *why* the cascade failed. The hook is best-effort:
its own completion or failure never masks the original cascade error, which is
still returned. This makes `on_fail` a cross-signal rollback hook — e.g. undo
reservation bookkeeping when an allocation cascade cannot complete.

The hook runs once per failing reaction, in the natural cascade order
(bottom-up: an inner failing reaction compensates before the outer reaction
that cascaded into it observes the failure and compensates in turn). A reaction
with no `on_fail` behaves exactly as before — the cascade error propagates
untouched.

## Fork / join (M44)

A single transition can fan out to several cross-signal reactions, and a later
reaction can wait until a whole *group* of those has finished. `fork` declares
the parallel group; `join` declares the reactions that wait for it:

```ddl
signal A { states: [a0, a1] initial: a0 on go from a0 -> a1 }
signal B { states: [b0, b1] initial: b0 on react from b0 -> b1 }
signal C { states: [c0, c1] initial: c0 on react from c0 -> c1 }
signal D { states: [d0, d1] initial: d0 on react from d0 -> d1 }

fork {
    // Both fire when A enters a1 — in parallel, each with its own cascade.
    when A enters a1 -> B react
    when A enters a1 -> C react
}
// Held back until the fork group completes, then fires.
join fork0 {
    when A enters a1 -> D react
}
```

Semantics:

- `fork { ... }` assigns every reaction inside it the same `join_group` name,
  auto-generated from source order: the first `fork` block is `fork0`, the
  second `fork1`, and so on. All members fire (in source order, each running to
  completion including its own sub-cascade, mirroring the existing per-signal
  atomic rollback). The group is *complete* once every member has fired.
- `join <group> { ... }` assigns each reaction inside it a `requires` on the
  named group. Those reactions are held until the group completes, then fired.
  A `join` may reference a `fork` declared either earlier or later in the
  source (forward references are allowed, like guard references); a reference
  to a group with no matching `fork` is a compile-time error.
- A reaction with no `join_group` and empty `requires` behaves exactly like the
  pre-M44 cascade — so existing topologies are unchanged.

Each `fork`/`join` block may carry a `when <guard>` guard and/or a
`with { ... }` static payload exactly like a standalone `reaction` block.

The compiler lowers `fork`/`join` onto the `ReactionDef.join_group` and
`ReactionDef.requires` fields. Because both fields are `#[serde(default)]`, the
engine accepts topologies that omit them.

## Sub-topology components (M45)

A reusable **component** bundles signals, transitions, and reactions under a
name, like the JSON `ComponentDef`. On top of M16's parameterized placeholders
(`${param}`, bound at instantiation), a component can declare **ports** — named
exposed reaction interfaces — that an instance wires to parent-level signals.

```ddl
component lockable {
    params: [name]                       // optional; each `${param}` is bound on instantiation
    port out lock.locked as locked        // expose signal `lock` state `locked`, aliased `locked`
    port in  lock.unlocked               // no alias → addressed as `lock.unlocked`

    signal lock {
        states: [locked, unlocked]
        initial: unlocked
        on lock from unlocked -> locked
        on unlock from locked -> unlocked
    }
}
```

Ports:

- `port <direction> <signal>.<state> [as <alias>]` declares one.
- **direction** is `in` (parent can trigger this signal), `out` (this signal's
  state changes are visible to the parent) or `inout` (both). A port counts as
  metadata for wiring; direction does not change runtime semantics.
- Every string field — including a port's `signal`/`state` — may use
  `${param}` placeholders.

An **instantiation** creates a concrete copy of a component with its params
bound and its ports wired to parent signals:

```ddl
instantiate lockable as door with { name -> door } connect { locked -> door }
```

- `instantiate <component> as <id> with { <param> -> <value>, ... }` binds the
  component's declared params. Every declared param must be supplied (the same
  rule as JSON `InstanceDef`).
- `connect { <port> -> <parent_signal>, ... }` (optional) wires each named port
  — by its alias, or by `<signal>.<state>` when it has no alias — to a
  parent-level signal. During expansion the component-internal signal named by
  the port is **renamed** to the connected parent signal everywhere inside the
  instance (its signal id, its transitions, and any reaction referencing it).
  That is how a component's exposed reaction interface becomes a parent
  reaction: wire the port to a parent signal and write the parent reaction
  against that signal.

Rules:

- A component name must be unique; duplicate ids are a compile-time error.
- A component body holds `port`, `signal`, and `reaction` blocks. Fork/join
  blocks are top-level only and cannot appear inside a component.
- A connection to an undeclared port, a port whose signal/state does not exist
  in the component, or the same port wired to two different targets is a
  compile-time error.
- When a component has no ports wired in an instance, expansion behaves exactly
  like the pre-M45 M16 expansion (signal ids are param-substituted; no signal
  is renamed). Param-only reuse keeps working unchanged.

Example wiring a sub-topology into a parent:

```ddl
signal controller { states: [idle, alerted] initial: idle
    on notify from idle -> alerted
    on reset from alerted -> idle }

component lockable {
    port out lock.locked as locked
    signal lock { states: [locked, unlocked] initial: unlocked
        on lock from unlocked -> locked
        on unlock from locked -> unlocked } }

reaction { when door enters locked -> controller notify }

instantiate lockable as door with {} connect { locked -> door }
```

After compiling, the component's internal `lock` signal is renamed to `door`,
its `lock`/`unlock` transitions become `door`'s transitions, and the parent
reaction `when door enters locked -> controller notify` cascades when the locked
sub-topology is entered — the sub-topology's exposed port feeds the parent.

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
| `line L col C: expected Identifier, found ...` (after `on_fail:`)       | `on_fail:` must be followed by an action id.  |
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
| `unused-guard-template` (M39) | A top-level `guard <id>` declaration no reaction references — dead code.            | A typo'd guard id that nothing wires up.                                                      |
| `duplicate-guard-condition` (M39) | Two top-level guards with identical expression text — likely meant to be one shared template. | `guard g1 { payload.amount <= 100 }` and `guard g2 { payload.amount <= 100 }`.          |

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
