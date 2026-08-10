# Buona architecture

Buona has two related responsibilities: it manages workspace state and it
resolves portable build commands. The CLI keeps those domains separate after a
small dispatch layer.

```text
CLI parsing and global policy (`src/main.rs`, `src/output.rs`)
├── workspace commands (`src/workspace/`)
│   └── locate → operate → persist metadata → sync `.code-workspace`
└── run, detect, and inspect (`src/run/`)
    └── resolve target → load config → detect → plan → hooks → execute/render
```

## Command dispatch

`src/main.rs` owns clap types and translates CLI arguments into domain option
types. It also configures global output policy before dispatch and maps errors
to process exit codes. Domain modules should not parse CLI arguments.

`src/output.rs` is the process-wide presentation boundary. Human progress is
emitted through `textln!`/`text_errln!`; JSON documents go through
`output::print_json`. This separation keeps stdout machine-readable in JSON
mode without forcing presentation concerns into domain types.

## Workspace flow

1. `workspace::locator` resolves an explicit workspace or walks upward for
   `buona.workspace.json`.
2. `workspace::ops` coordinates the requested operation.
3. Focused modules perform package addition, removal, adoption, sync, or editor
   opening.
4. `workspace::types` persists workspace metadata atomically.
5. `workspace::workspace_file` regenerates only the managed folder list while
   preserving user-owned workspace settings.

Packages are discovered dynamically as sorted directories under `src/`.
Package URLs and branches are read from Git; they are not stored in workspace
metadata.

## Run flow

1. `run::targets` selects the closest target, explicit targets, or the recursive
   root-plus-packages set.
2. `run::config` loads the target's `buona.json`.
3. `run::detect` scans marker files in priority order.
4. `run::resolve` builds a pure `ExecutionPlan`.
5. `run::hooks` resolves explicit and convention-based pre/post hooks.
6. `run::planner` combines the target, execution plan, and hooks.
7. `run::executor` executes serially or in parallel; dry runs stop before this
   boundary and render the plan instead.

Build-system precedence, from most to least specific, is:

1. `commands.<name>.system` in the target's `buona.json`
2. CLI `--system`
3. top-level `system` in the target's `buona.json`
4. marker-file detection

An explicit `commands.<name>.exec` bypasses standard command mapping entirely.

## Configuration domains

Three JSON documents have intentionally different lifetimes:

| Domain | Location | Rust owner | Purpose |
| --- | --- | --- | --- |
| Global | `~/.config/buona/config.json` | `src/config.rs` | Workspace root, editor, and Git defaults |
| Workspace | `<workspace>/buona.workspace.json` | `src/workspace/types.rs` | Workspace identity and tracking overrides |
| Target | `<target>/buona.json` | `src/run/config.rs` | Build system, command overrides, and hooks |

Schemas in `schemas/` are the editor- and automation-facing contract. Rust
types remain tolerant of unknown persisted keys for compatibility, while CLI
mutation paths deserialize strictly before writing.

## Side effects and safety

- Atomic writes are centralized in `src/fsutil.rs`.
- Workspace deletion and package removal require confirmation unless `--yes`
  is supplied.
- JSON output implies non-interactive operation.
- `buona run --dry-run` and `buona inspect` resolve plans without spawning them.
- Actual `buona run` execution is text-only because child processes own stdout.
- Self-update requires release checksums unless explicitly overridden.

## Testing boundaries

- Pure resolution logic is unit-tested beside its module.
- Filesystem behavior uses temporary directories.
- `tests/agent_cli.rs` treats the compiled binary as a black box.
- `tests/docs_contract.rs` verifies schemas, fixtures, generated help, and
  supported build-system examples.
- `just ci` is the canonical local equivalent of the GitHub Actions gate.
