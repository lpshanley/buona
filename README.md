# buona

**The Good CLI** — making life easier when managing complex workspace and build tasks.

*Buona* (Italian for "good") is a lightweight command-line tool for organizing and managing workspaces. It gives you a single, consistent interface for creating, listing, and deleting project workspaces — and for adding and removing packages within them — so you can focus on building rather than bookkeeping.

## Features

- **Workspace management** — Create, list, and delete workspaces.
- **Package management** — Add and remove packages (git repositories) within workspaces.
- **Automatic workspace file sync** — The `.code-workspace` file is automatically regenerated whenever packages are added, removed, or a workspace is created — no manual sync step needed.
- **Sync & pull** — Pull the latest changes for every package in a workspace with a single command.
- **Editor integration** — Open a workspace directly in VS Code or Cursor.
- **Global configuration** — A simple config file (`~/.config/buona/config.json`) keeps your preferences consistent across projects.
- **Interactive setup** — A guided wizard walks you through first-time configuration.
- **Minimal & fast** — Built in Rust with a small dependency footprint.

## Installation

### From GitHub (requires [Rust](https://rustup.rs/))

```sh
cargo install --git https://github.com/lpshanley/buona
```

### From source (requires [Rust](https://rustup.rs/))

```sh
cargo install --path .
```

Or, if you have [just](https://github.com/casey/just) installed:

```sh
just install
```

## Quick start

### 1. Configure your workspace directory

```sh
buona config setup
```

This launches an interactive prompt to set the root directory where your workspaces will live (defaults to `~/workspace`).

### 2. Create a workspace

```sh
buona workspace create my-project
```

Creates a new workspace directory with a `buona.workspace.json` metadata file. You can optionally provide a display name:

```sh
buona workspace create my-project --name "My Project"
```

### 3. List workspaces

```sh
buona workspace list
```

### 4. Add packages to a workspace

From inside a workspace directory, add one or more packages:

```sh
cd ~/workspace/my-project

# By name (uses configured host, protocol, and organization)
buona ws add -p my-library

# By org/name (uses configured host and protocol)
buona ws add -p acme/toolkit

# By full URL
buona ws add -p git@github.com:acme/toolkit.git

# Multiple packages at once
buona ws add -p my-library -p acme/toolkit -p git@github.com:other/repo.git
```

Packages are cloned into the workspace's `src/` directory and tracked in `buona.workspace.json`. The `.code-workspace` file is automatically updated.

You can also target a workspace by name instead of being inside it:

```sh
buona ws add -p my-library --workspace my-project
```

### 5. Remove packages from a workspace

```sh
# Remove a single package
buona ws remove -p toolkit

# Remove multiple packages at once
buona ws remove -p toolkit -p utils

# Target a specific workspace
buona ws remove -p toolkit --workspace my-project

# Skip the confirmation prompt
buona ws remove -p toolkit --force
```

This removes the package directory from `src/` and updates `buona.workspace.json`. The `.code-workspace` file is automatically updated.

### 6. Sync packages

Pull the latest changes for every package in the workspace and regenerate the `.code-workspace` file:

```sh
buona ws sync

# Or target a specific workspace
buona ws sync --workspace my-project

# Sync only specific packages
buona ws sync -p toolkit -p utils

# Fetch only (no merge) — useful for reviewing changes before merging
buona ws sync --fetch

# Combine: fetch specific packages only
buona ws sync -p toolkit --fetch
```

This runs `git pull` (or `git fetch` with `--fetch`) in each package directory under `src/` and reports results. When `-p` is omitted, all tracked packages are synced.

### 7. Open a workspace in your editor

```sh
buona ws open

# Or target a specific workspace
buona ws open --workspace my-project
```

This regenerates the `.code-workspace` file and opens it in your configured editor (VS Code or Cursor).

### 8. Delete a workspace

```sh
buona workspace delete my-project
```

Add `--force` to skip the confirmation prompt.

## Usage

```
buona <COMMAND>

Commands:
  config     View or set up the global configuration
  workspace  Manage workspaces (alias: ws)
```

### `buona config`

```
buona config <COMMAND>

Commands:
  show   Display the current configuration (use --json for machine-readable output)
  setup  Launch the interactive setup wizard
```

### `buona workspace`

```
buona workspace <COMMAND>

Commands:
  list    List all workspaces in the configured directory
  create  Create a new workspace
  delete  Delete a workspace
  add     Add packages to a workspace
  remove  Remove packages from a workspace
  sync    Pull latest changes for all packages and sync the workspace file
  open    Open workspace in the configured editor
```

#### `buona workspace create`

```
buona workspace create <PATH> [--name <NAME>]
```

Creates a new workspace directory with a `buona.workspace.json` metadata file.

#### `buona workspace delete`

```
buona workspace delete <WORKSPACE> [--force]
```

Deletes a workspace and all of its contents. Prompts for confirmation unless `--force` is passed.

#### `buona workspace add`

```
buona workspace add -p <PACKAGE>... [--workspace <NAME>]
```

The `-p` flag accepts three package specifier formats:

| Format | Example | Resolution |
|--------|---------|------------|
| Package name | `my-library` | Uses configured `git.host`, `git.protocol`, and `git.organization` |
| Org/Package | `acme/toolkit` | Uses configured `git.host` and `git.protocol` |
| Full URL | `git@github.com:acme/toolkit.git` | Used directly |

Pass `-p` multiple times to add several packages in one command. Packages are cloned into the workspace's `src/` directory.

#### `buona workspace remove`

```
buona workspace remove -p <PACKAGE>... [--workspace <NAME>] [--force]
```

Removes one or more packages from a workspace. This deletes the package directory from `src/` and removes the entry from `buona.workspace.json`. The `.code-workspace` file is automatically updated. Prompts for confirmation unless `--force` is passed.

#### `buona workspace sync`

```
buona workspace sync [-p <PACKAGE>...] [--workspace <NAME>] [--fetch]
```

Runs `git pull` in tracked package directories and regenerates the `.code-workspace` file. Reports per-package success or failure. Pass `-p` one or more times to target specific packages (defaults to all). Pass `--fetch` to run `git fetch` instead of `git pull` (useful for reviewing incoming changes before merging).

#### `buona workspace open`

```
buona workspace open [--workspace <NAME>]
```

Regenerates the `.code-workspace` file and opens it in your configured editor (VS Code or Cursor).

## Configuration

Buona stores its configuration at `~/.config/buona/config.json`. The current settings:

| Key                | Description                                  | Default        |
|--------------------|----------------------------------------------|----------------|
| `workspace_dir`    | Root directory where workspaces are created   | `~/workspace`  |
| `ide`              | Preferred IDE (`vscode` or `cursor`)          | `vscode`       |
| `git.host`         | Default git host                              | `github.com`   |
| `git.organization` | Default organization on the git host          | *(empty)*      |
| `git.protocol`     | Clone/push protocol (`ssh` or `https`)        | `ssh`          |

## Workspace metadata

Each workspace contains a `buona.workspace.json` file:

```json
{
  "name": "my-project"
}
```

When packages are added via `buona ws add`, they are tracked in this file:

```json
{
  "name": "my-project",
  "packages": [
    {
      "name": "toolkit",
      "url": "git@github.com:acme/toolkit.git"
    }
  ]
}
```

The `packages` field is omitted when empty, so existing workspaces remain compatible.

## Development

```sh
# Run buona with arguments
just run <args>

# Run tests
just test

# Example commands
just config show
just workspace list
```

## License

MIT
