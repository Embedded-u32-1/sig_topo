# Getting Started

5 minutes from zero to your first running DDL workflow.

## 1. Build

```bash
cargo build --release
```

## 2. Write a DDL topology

```bash
cat > my_flow.ddl << 'DDL'
signal order {
    states: [draft, submitted, approved]
    initial: draft

    on submit from draft -> submitted {
        on_transition: validate
        on_enter: notify
    }

    on approve from submitted -> approved {
        when payload.amount > 0
    }
}

reaction {
    when order enters submitted -> order approve
}
DDL
```

## 3. Compile to JSON

```bash
cargo run --bin stc -- my_flow.ddl my_flow.json
```

## 4. Drive it interactively

```bash
cargo run --bin sts -- my_flow.json
```

```
sts> event order submit
order -> draft
  action executed: validate
  action executed: notify
sts> event order approve {"amount": 100}
order -> approved
sts> trace
[1784320531134] EventReceived order.submit payload=None
[1784320531134] StateChanged order: draft -> submitted
...
sts> quit
```

## 5. Watch mode (auto-recompile on change)

```bash
Terminal 1:  cargo run --bin stc -- watch my_flow.ddl --interval 500
Terminal 2:  edit my_flow.ddl  ->  "Recompiled OK" appears automatically
```

## 6. Visualize

```bash
cargo run --bin stv -- my_flow.json          # -> my_flow.dot + .svg (needs Graphviz)
cargo run --bin sts -- my_flow.json           # then: dot-ext  -> my_flow_guarded.svg
```

## Next steps

- [Shell reference](shell.md) — all `sts` commands including `why`, `dot-ext`
- [DDL language](ddl.md) — guards, fork/join, sub-topology, `guard` templates
- [Scenarios](../examples/scenarios/) — 11 ready-to-run teaching scenarios
- [Architecture](architecture.md) — how it fits together
