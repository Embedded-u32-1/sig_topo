# Architecture

```
┌─────────────────────────────────────────────────────┐
│                    DDL Source (.ddl)                 │
└───────────────────────┬─────────────────────────────┘
                        │ stc (compiler)
                        ▼
┌─────────────────────────────────────────────────────┐
│              TopologySchema (JSON model)              │
│  signals | transitions | reactions | components       │
└───────────────────────┬─────────────────────────────┘
                        │ from_json / load_topology
                        ▼
┌─────────────────────────────────────────────────────┐
│                  TopologyEngine                      │
│  validate → send_event → [guard] → actions →         │
│  StateChanged → reactions → fork/join →补偿          │
│                                                     │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌──────────┐  │
│  │  guard   │ │persist   │ │ trace  │ │ snapshot │  │
│  │  eval    │ │save/load │ │ events │ │ _dot     │  │
│  └─────────┘ └──────────┘ └────────┘ └──────────┘  │
└──────────┬────────────┬──────────────┬──────────────┘
           │            │              │
     sts (REPL)   stt (replay)    stv (DOT/SVG)
     stc (lint)   stp (persist)   stc watch
                  C-ABI (ffi.rs)   WASM (wasm-topology/)
```

## Core modules

| Module | Responsibility |
|--------|----------------|
| `schema` | Data model: `SignalDef`, `TransitionDef`, `ReactionDef`, `TopologySchema` |
| `engine` | Runtime: validation, event dispatch, guard eval, fork/join scheduling,补偿 |
| `compose` | Compile-time: includes resolution, component expansion, signal remapping |
| `guard` | Expression language: lexer → parser → evaluator |
| `trace` | Observability: `TraceEvent` log with signal/time filtering |
| `persist` | State snapshots: `save_state` / `load_state` / `reload_topology` |
| `export` | Visualization: `to_dot`, `to_dot_with_state`, `to_dot_extended`, `render_dot_to_svg` |
| `ddl` | DSL compiler: lexer → parser → codegen |
| `run` | Binary scaffolding: `Scenario` replay, shared helpers |

## Core concepts

- **Signal**: independent state machine with `states` + `initial_state`
- **Transition**: triggered by event; binds lifecycle actions
- **Reaction**: cross-signal cascade (A enters state → send_event to B)
- **Guard**: boolean expression gating a transition or reaction
- **Fork/Join**: parallel reaction groups + sync bars
- **Sub-topology**: reusable component with exposed ports + instance wiring

## Data flow

```
DDL → compile_full → (schema, DdlDoc)
                     ↓
         TopologyEngine::from_schema  ← StateSnapshot (optional)
                     ↓
              engine.send_event
                     ↓
    guard eval → on_exit → commit → on_transition → on_enter
                     ↓
              StateChanged → matching reactions → dispatch_reactions
                     ↓
              trace events → snapshot_dot_extended → stv/sts
```
