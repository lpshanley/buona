# buona

**The Good CLI** — making life easier when managing complex workspace and build tasks.

*Buona* (Italian for "good") is a lightweight command-line tool for organizing and managing workspaces. It gives you a single, consistent interface for creating, listing, and removing project workspaces so you can focus on building rather than bookkeeping.

## Features

- **Workspace management** — Create, list, and remove workspaces from a central directory.
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

### 4. Remove a workspace

```sh
buona workspace remove my-project
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
  remove  Remove a workspace
```

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

This file is used by buona to track workspace names and can be extended in the future with additional metadata.

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
