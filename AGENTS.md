# AGENTS.md

## Purpose

Buona is a Rust CLI for managing multi-repository workspaces and running
standardized build commands across different build systems.

## Repository map

- `src/main.rs`: CLI definitions and command dispatch.
- `src/config.rs`: global `~/.config/buona/config.json` behavior.
- `src/run/`: target detection, command planning, hooks, execution, and inspect.
- `src/workspace/`: workspace and package lifecycle operations.
- `src/output.rs`: process-wide text, JSON, color, and interaction policy.
- `schemas/`: JSON schemas for persisted configuration.
- `tests/fixtures/`: representative workspaces, systems, and invalid inputs.
- `docs/architecture.md`: component boundaries and execution flows.
- `docs/agent-interface.md`: machine-readable CLI contract.

## Development workflow

- During iteration, run the narrowest relevant `cargo test <name-or-module>`.
- Run all tests with `just test`.
- Run `just docs-check` after changing CLI help, schemas, or fixtures.
- Run `just ci` before considering a change complete.
- Do not run install, release, coverage-opening, or destructive workspace
  commands unless the user explicitly requests them.

## Change contracts

- Add tests beside pure logic and integration coverage for CLI contracts.
- When configuration changes, update the Rust type, schema, README, fixtures,
  and contract tests together.
- Preserve documented exit codes and JSON field meanings.
- Keep stdout valid JSON whenever `--output json` is selected; diagnostics go
  to stderr.
- JSON mode and `--non-interactive` must never prompt for terminal input.
- Keep destructive actions behind explicit `--yes` confirmation in automation.
- Preserve unrelated user changes and avoid broad rewrites.

## Architecture invariants

- Command precedence is per-command config, CLI override, target config, then
  marker-file detection.
- Workspace packages are directories under `src/`; they are not persisted in
  `buona.workspace.json`.
- `buona run` planning must stay separate from process execution so dry runs and
  inspection remain safe.
- Mutating workspace metadata must re-sync the generated `.code-workspace` file.
- Unknown persisted config keys may be read for compatibility but must not be
  silently introduced by CLI mutation commands.
