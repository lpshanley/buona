# Workspace Templates & Default Packages

**Date:** 2026-02-23
**Status:** Approved

## Problem

When creating a new workspace with `buona workspace create`, the output contains only structural files (`buona.workspace.json`, `.code-workspace`, `src/`). Users who want to bootstrap workspaces with custom configuration — hook scripts, tool configs, project docs — must manually create these files every time.

Similarly, users who always include the same package(s) in every workspace must pass `--packages` on every invocation.

## Solution

Three additions to buona:

1. **Default packages** — global config array of packages auto-added to every new workspace
2. **Workspace template** — global config path to a directory whose contents are copied into new workspaces
3. **Auto-run install** — after packages are added during create, automatically run `buona run install`

## Design

### 1. Default Packages

**Config** — add `default_packages: Vec<String>` to `BuonaConfig`:

```json
{
  "default_packages": ["shippo-ai-tools"]
}
```

**Behavior** in `workspace::create()`:
- Load global config
- Merge `default_packages` with explicit `--packages` args (deduped, explicit wins)
- Pass combined list to `add_packages_to_workspace()`

**CLI** — add `--no-defaults` flag to skip default packages.

**Setup** via existing config commands:
- `buona config set default_packages '["shippo-ai-tools"]'`

### 2. Workspace Template

**Config** — add `workspace_template: Option<String>` to `BuonaConfig`:

```json
{
  "workspace_template": "~/.config/buona/workspace-template"
}
```

**Template directory** (user-managed):
```
~/.config/buona/workspace-template/
├── buona.json
├── .buona/
│   └── hooks/
│       └── postinstall
└── CLAUDE.md
```

**Behavior** in `workspace::create()`:
- After creating workspace dir + `buona.workspace.json`, before adding packages
- If `workspace_template` is configured and exists: recursively copy contents into workspace root
- Preserve file permissions (executable hooks)
- Warn (don't fail) if template dir doesn't exist
- Don't overwrite `buona.workspace.json` or `.code-workspace`

**CLI**:
- `--template <path>` — override global config for this invocation
- `--no-template` — skip template entirely

### 3. Auto-Run Install

**Behavior** in `workspace::create()`:
- After packages are added (defaults + explicit)
- If packages were added AND `buona.json` exists in workspace root: run `buona run install`
- This triggers hooks (e.g., postinstall copies `.claude/` from a package into workspace root)

**CLI** — `--no-install` flag to skip.

## End-to-End Flow

```
$ buona workspace create ticket-CET-1234 -p some-service

Creating workspace at ~/workspace/ticket-CET-1234...
  ✓ Created workspace directory
  ✓ Wrote buona.workspace.json
  ✓ Applied workspace template
  ✓ Synced .code-workspace file
  ✓ Added package: shippo-ai-tools (default)
  ✓ Added package: some-service
  ✓ Running install...
    Copied .claude/skills from shippo-ai-tools
    Copied .claude/commands from shippo-ai-tools
    Copied .claude/rules from shippo-ai-tools
    Copied .claude/agents from shippo-ai-tools
    Copied .claude/templates from shippo-ai-tools
    Shippo AI tools installed into workspace
  ✓ Workspace ready
```

## Files Changed

| File | Change |
|------|--------|
| `src/config.rs` | Add `default_packages` and `workspace_template` to `BuonaConfig` |
| `src/workspace/ops.rs` | Template copy logic, merge default packages, auto-run install |
| `src/main.rs` | Add `--no-defaults`, `--template`, `--no-template`, `--no-install` CLI flags |

## Ordering

1. Template is applied first (provides `buona.json` + hooks)
2. Default packages are added second (alongside explicit packages)
3. Install runs last (hooks from template can now act on installed packages)
