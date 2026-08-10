# Agent and automation interface

Buona exposes a stable discovery surface for coding agents, CI, and scripts.
Prefer discovery and dry-run commands before mutations.

## Global controls

```sh
buona --output json <command>
buona --no-color <command>
buona --non-interactive <command>
```

Global options can also appear after a subcommand. JSON mode implies
non-interactive mode and writes exactly one JSON document to stdout. Structured
errors are written to stderr.

## Recommended discovery sequence

```sh
buona inspect --output json
buona detect --output json
buona run test --dry-run --output json
```

`inspect` reports the selected target, workspace and packages, all applicable
config paths, marker-file detections, and resolved plans and hooks for every
standard command. Use `--target root` or `--target <package>` inside a workspace
to inspect another target.

## JSON result contracts

Read commands return domain documents. Successful mutations use this envelope:

```json
{
  "ok": true,
  "operation": "workspace.rename",
  "data": {
    "workspace": "old-name",
    "new_name": "new-name"
  }
}
```

Failures use this stderr envelope:

```json
{
  "ok": false,
  "error": {
    "code": "configuration",
    "message": "config error: ...",
    "exit_code": 68,
    "hint": "Check buona.json and the command arguments.",
    "target": null
  }
}
```

Field meanings are stable; new fields may be added in backward-compatible
releases. Consumers should ignore unknown fields.

## Interaction rules

- Use `--yes` for workspace deletion and package removal. `--force` remains a
  compatibility alias for those commands.
- `config setup` is intentionally interactive; automation should use
  `config set`, `config add`, and `config remove`.
- Actual `buona run` execution cannot use JSON output because child programs
  write directly to stdout. Use text output for execution and JSON with
  `--dry-run` for planning.
- `--no-color` also requests colorless output from spawned child commands.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `1` | General I/O, workspace, or update error |
| `2` | CLI usage error |
| `65` | Target/package resolution failed |
| `68` | Invalid configuration or incompatible options |
| `69` | Ambiguous convention-based hook |
| child code | A command or hook ran and failed |

## Compatibility checks

Run `just docs-check` after changing CLI options, schemas, output documents, or
fixtures. Golden JSON examples live in `tests/golden/`, while runnable examples
live in `tests/fixtures/`.
