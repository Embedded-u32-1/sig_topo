# Changelog

## v1.0.0 — Project Graduation

File-driven Rust state-machine/workflow engine: describe systems as DDL topology, run scenarios, persist state, trace events, visualize, and export to DOT/SVG/WASM.

### Capabilities

- **DDL** (Domain Description Language): `.ddl` source → JSON topology. Expressive syntax: signals, transitions with lifecycle hooks (`on_exit`/`on_transition`/`on_enter`), guards, reactions, fork/join, sub-topology components, `guard` templates, reaction补偿 (`on_fail`).
- **Workflow Engine**: fork (parallel reaction groups), join (sync bars), sub-topology composition (component ports + instance wiring), guard evaluation with补偿 hooks.
- **Multi-language**: C-ABI shared library (`.so`/`.a`), WASM/browser demo, Python/Node interop.
- **Visualization**: `snapshot_dot_extended` colors reaction edges by guard result (true=green/false=gray/error=red); `dot-ext` auto-renders SVG.
- **Toolchain**: `stv` (DOT/SVG), `stt` (scenario replay), `stp` (persist/reload), `sts` (REPL + why + dot-ext), `stc` (DDL compile + lint + watch).

### Test suite: 257 tests, clippy zero, doc zero warnings.

---

## v0.9.0

`stc --watch` — poll-mode file watcher with auto-recompile + scenario regression.

## v0.8.0

`dot-ext` command in `sts` — runtime DOT with guard-eval coloring. `render_dot_to_svg` shared helper.

## v0.7.0

Reaction compensation: `on_fail` hook runs a best-effort action carrying failure context before the cascade error propagates. `ActionContext.failure` field. `ReactionCompensated` trace event.

## v0.6.0

Fork/join workflow engine: `dispatch_reactions` Kahn-style topological scheduler. `join_group` + `requires` on `ReactionDef`. DDL `fork { }` / `join <group> { }` blocks. Sub-topology composition: `ComponentDef.ports` (PortDef/PortDirection), `InstanceDef.connections` (ConnectionDef), signal remapping at expansion.

## v0.5.0

Guard composition: top-level `guard <id> { <expr> }` templates + `when <id>` reaction references. `ReactionGuardEvaluated` trace event. `format_why` REPL helper. `snapshot_dot_extended`.

## v0.4.0

Guard trace + debugger: `ReactionGuardEvaluated` event, `sts why <reaction>` command. `snapshot_dot` runtime highlighting.

## v0.3.0

DDD expressiveness: `from *` wildcard, multi-action hooks, reaction static payload (`with { }`). stc `--check` semantic linting. WASM/browser demo via wasm-bindgen. `traces_json` helper.

## v0.2.0

Transaction guard: `ReactionDef.guard` evaluated against source-event payload. DDL `when <expr>`. C-ABI FFI (6 extern "C" functions). Scenario library (auto-discovering test regression).

## v0.1.0

MVP: JSON topology + engine + static validation + lifecycle actions. First DDL compiler (`stc`).
