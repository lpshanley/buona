# Buona 🤌😘

[![CI](https://github.com/lpshanley/buona/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/lpshanley/buona/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/lpshanley/buona/branch/main/graph/badge.svg)](https://codecov.io/gh/lpshanley/buona)

**The Good CLI** — Workspace Bliss – Build More, Fuss Less.

*Buona* (Italian for "good") is a lightweight command-line tool for organizing and managing workspaces. It gives you a single, consistent interface for creating, listing, and deleting project workspaces — and for adding and removing packages within them — so you can focus on building rather than bookkeeping.

## Features

- **Workspace management** — Create, list, inspect, rename, and delete workspaces.
- **Package management** — Add and remove packages (git repositories) within workspaces.
- **Automatic workspace file sync** — The `.code-workspace` file is automatically regenerated whenever packages are added, removed, or a workspace is created — no manual sync step needed. Only the `folders` list is rewritten; any `settings` or other customizations you add to the file are preserved.
- **Sync & pull** — Pull the latest changes for every package in a workspace with a single command.
- **Editor integration** — Open a workspace directly in VS Code or Cursor.
- **Global configuration** — A simple config file (`~/.config/buona/config.json`) keeps your preferences consistent across projects.
- **Interactive setup** — A guided wizard walks you through first-time configuration.
- **Build system detection** — Auto-detects Cargo, Go, npm, pnpm, yarn, Bun, uv, Poetry, Make, Gradle, and Maven projects.
- **Universal build commands** — Run standardized commands (`build`, `test`, `lint`, etc.) across any project via `buona run`.
- **Lifecycle hooks** — Configure `pre<command>` and `post<command>` hooks in `buona.json` or as executable scripts.
- **Target configuration** — Fine-tune build system and commands via `buona.json` per execution target (workspace root or package).
- **Package init** — Scaffold a `buona.json` in any project with `buona init`.

## Installation

### Quick install (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/lpshanley/buona/main/install.sh | sh
```

This downloads the latest prebuilt binary for your platform and installs it to `~/.local/bin`. You can customize the install directory:

```sh
BUONA_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/lpshanley/buona/main/install.sh | sh
```

To install a specific version:

```sh
curl -fsSL https://raw.githubusercontent.com/lpshanley/buona/main/install.sh | sh -s -- v0.1.6
```

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

### Updating

Once installed, buona can update itself:

```sh
buona self update
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

### 4. Rename a workspace

```sh
buona workspace rename AI-115 CET-2670
```

Renames the workspace directory and updates the name in `buona.workspace.json`, keeping the registry metadata intact. The `.code-workspace` file is regenerated under the new name. Use `--keep-directory` to update only the metadata name and leave the directory as-is:

```sh
buona workspace rename AI-115 CET-2670 --keep-directory
```

### 5. Add packages to a workspace

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

### 6. Remove packages from a workspace

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

### 7. Sync packages

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

### 8. View workspace details

```sh
buona ws info

# Or target a specific workspace
buona ws info --workspace my-project

# Output as JSON (useful for scripting)
buona ws info --json
```

Shows detailed information about a workspace: its name, directory, `.code-workspace` file status, and all tracked packages with their clone URLs and on-disk status.

### 9. Open a workspace in your editor

```sh
buona ws open

# Or target a specific workspace
buona ws open --workspace my-project
```

This regenerates the `.code-workspace` file and opens it in your configured editor (VS Code or Cursor).

#### `buona workspace adopt`

```
buona workspace adopt <PATH> [--workspace <NAME>] [--copy] [--name <PACKAGE_NAME>]
```

Adopts an existing local directory into a workspace. By default, the directory is moved into the workspace's `src/` directory. Use `--copy` to copy instead of move. Use `--name` to override the package name (defaults to the directory name).

```sh
# Adopt a local project
buona ws adopt ~/projects/my-library

# Copy instead of move
buona ws adopt ~/projects/my-library --copy

# Override the package name
buona ws adopt ~/projects/my-library --name custom-name
```

### 10. Initialize package config

```sh
buona init
```

Creates a `buona.json` in the current directory. Auto-detects the build system when possible and writes it as `"system"`. Use `--system <name>` to set it explicitly, or `--force` to overwrite an existing file. Outside a workspace, this file also marks the package root for nested `buona run` / `buona detect` (walk-up resolution).

```sh
buona init --system npm
buona init --force
```

### 11. Detect build system

```sh
buona detect
```

Prints the auto-detected build system for the closest context (package if inside `src/<pkg>`, otherwise workspace root). Outside a workspace, walks up for the nearest `buona.json` and detects there (falls back to the current directory if none is found).

You can also target specific locations (workspace only):

```sh
# detect only at workspace root
buona detect -t root

# detect in explicit order
buona detect -t root -t api -t web

# detect workspace root + all packages (alphabetical)
buona detect -r
```

### 12. Run build commands

```sh
buona run build
```

See the [Build System Commands](#build-system-commands) section for full details.

## Build System Commands

Buona provides universal build commands that work consistently across all your projects, regardless of their underlying build system.

### Detect build system

```sh
buona detect
```

Prints the auto-detected build system for the closest context (package if inside `src/<pkg>`, otherwise workspace root), including all detected marker files. Outside a workspace, walks up for the nearest `buona.json` and detects there (falls back to the current directory if none is found). This helps you verify that buona correctly identifies your project type.

Example output:
```
→ detected: cargo (via Cargo.toml)

  Other marker files found:
  ·  make (via Makefile)
```

### Run build commands

```sh
buona run <COMMAND> [ARGS...]
```

Executes a build command in the closest context by default (package if inside `src/<pkg>`, otherwise workspace root). Outside a workspace, walks up for the nearest `buona.json` and runs there (falls back to the current directory if none is found) using the same detection and hook rules — no workspace required. Use `buona init` to create that marker. `--target` / `-t` and `--recursive` / `-r` still require a workspace.

Buona automatically detects the build system and maps standard commands to the appropriate tool invocation.

**Standard commands** (mapped across all build systems):
| Command | Description |
|---------|-------------|
| `install` | Install dependencies |
| `build` | Compile/build the project |
| `run` | Run the application |
| `test` | Run tests |
| `lint` | Run linters |
| `fmt` / `format` | Format code |
| `clean` | Clean build artifacts |
| `publish` | Publish package |
| `bench` | Run benchmarks |
| `doc` / `docs` | Generate documentation |
| `dev` | Start development server |

**Examples:**
```sh
# Build the current package
buona run build

# Run tests with extra arguments
buona run test -- --nocapture

# Run a non-standard command (proxied through the build system)
buona run my-custom-script

# Run only in the workspace root
buona run build -t root

# Run in explicit ordered targets
buona run test -t api -t web

# Recursive orchestration: root + all packages (alphabetical)
buona run install -r

# Recursive parallel execution with 4 workers
buona run test -r --parallel --jobs 4

# Preview staged graph with noop leaves
buona run install --dry-run -r
```

**Options:**
- `--system <SYSTEM>` — Force a specific build system (overrides auto-detection and the global `system` in `buona.json`; per-command `commands.<name>.system` overrides still win)
- `--dry-run` — Show the resolved command without executing it
- `--verbose` — Print detailed resolution information
- `--target <root|PACKAGE>` / `-t` — Run only for the provided target(s), in the order provided
- `--recursive` / `-r` — Run staged orchestration for workspace root and all packages (alphabetical)
- `--parallel` — Enable parallel execution across recursive package runs or explicit target lists
- `--jobs <N>` — Maximum concurrent tasks in parallel mode
- `--fail-policy <fail-fast|continue>` — Parallel failure behavior
- `--` — Pass remaining arguments to the underlying tool

When `--dry-run` is used with `-t` or `-r`, buona prints a staged execution graph and renders missing leaves as dimmed `noop` (e.g., no pre/post hook for that stage). If a command stage cannot be resolved (for example no build system detected), it is shown as `skipped` and `--verbose` includes the reason.

**Build system precedence** (most specific wins): `commands.<name>.system` in `buona.json` → CLI `--system` → global `system` in `buona.json` → auto-detection from marker files.

When a single target runs serially (the common `buona run build` case), the child process inherits the terminal directly, so progress bars, colors, and interactive prompts work as usual. With multiple targets, `--parallel`, or `-r`, output is streamed line-by-line with a `[target:<name>/<stage>]` prefix.

**Supported build systems:**
| System | Marker Files | Notes |
|--------|--------------|-------|
| `cargo` | `Cargo.toml` | Rust projects |
| `go` | `go.mod` | Go modules |
| `npm` | `package.json`, `package-lock.json` | Node.js (npm) |
| `pnpm` | `pnpm-lock.yaml` | Node.js (pnpm) |
| `yarn` | `yarn.lock` | Node.js (Yarn) |
| `bun` | `bun.lock`, `bun.lockb` | Bun runtime |
| `uv` | `pyproject.toml` | Python (non-Poetry) |
| `poetry` | `pyproject.toml` | Python (Poetry projects) |
| `make` | `Makefile` | Make-based projects |
| `gradle` | `build.gradle`, `build.gradle.kts` | Prefers `gradlew` wrapper if present |
| `maven` | `pom.xml` | Prefers `mvnw` wrapper if present |

## Target Configuration (`buona.json`)

Create a `buona.json` file in any execution target directory (workspace root or package root) to customize build system behavior for that target.

### Complete `buona.json` template

```json
{
  "$schema": "./schemas/buona.schema.json",
  "system": "cargo",
  "commands": {
    "build": {
      "system": "make"
    },
    "test": {
      "exec": ["pnpm", "run", "custom-test"]
    }
  },
  "hooksDir": ".buona/hooks",
  "hooks": {
    "prebuild": "./scripts/generate.sh",
    "posttest": "docker compose down",
    "prelint": ["cargo", "fmt", "--check"]
  }
}
```

### Configuration fields

| Field | Type | Description |
|-------|------|-------------|
| `system` | string | Global build system for this target (`"auto"` or a system name) |
| `commands` | object | Per-command overrides (see below) |
| `hooksDir` | string | Directory to scan for convention-based hook scripts (default: `.buona/hooks`, resolved from the directory containing `buona.json`) |
| `hooks` | object | Explicit hook definitions (keys: `pre<command>` or `post<command>`) |

### Command overrides

Each command in the `commands` object can have:

- `system` — Override the build system for just this command
- `exec` — Full exec override that replaces the entire command (array of strings)

```json
{
  "commands": {
    "build": { "system": "make" },
    "test": { "exec": ["./run-tests.sh", "--ci"] }
  }
}
```

### Fallthrough (proxy) behavior

If a command is not explicitly overridden with `commands.<name>.exec`, buona proxies it through the resolved system.

- With `"system": "make"`, `buona run deploy` executes `make deploy`.
- With `"system": "cargo"`, `buona run clippy` executes `cargo clippy`.
- With `"system": "npm"`, non-builtins are proxied as `npm run <command>`.

This means you can set a single system and still run custom/non-standard commands through that tool.

## Lifecycle Hooks

Hooks are `pre<command>` and `post<command>` scripts that run before and after the main command. They are resolved from two sources in priority order:

1. **Explicit `hooks` map in `buona.json`** (highest priority)
2. **Convention-based files in `hooksDir`**

### Explicit hooks in `buona.json`

```json
{
  "hooks": {
    "prebuild": "./scripts/generate-code.sh",
    "posttest": "docker compose down",
    "prelint": "cargo fmt --check"
  }
}
```

Hook values can be:
- A **build system name** string (e.g., `"cargo"`, `"npm"`) — buona will use that system's template for the command
- A **shell command** string — executed via `sh -c`
- An **argv array** (e.g., `["pnpm", "run", "build"]`) — executed directly as program + args

### Convention-based hooks

Place executable files in your `hooksDir` (default: `.buona/hooks/`):

```
my-package/
├── .buona/
│   └── hooks/
│       ├── prebuild
│       └── posttest
├── Cargo.toml
└── src/
```

Hook files must be **executable** to run. The filename (without extension) determines which hook it is. Multiple files with the same stem will cause an ambiguity error.

### Hook execution order

For `buona run build`:
1. `prebuild` hook (if exists)
2. Main build command
3. `postbuild` hook (if exists)

Hooks run in the package directory with the same working directory as the main command. If a hook fails, execution stops.

### Viewing hooks

Use `--verbose` to see which hooks are resolved:

```sh
buona run build --verbose
```

Use `--dry-run` to preview what would execute without running anything:

```sh
buona run build --dry-run
```

## Usage

Commands:
  config     View or set up the global configuration
  detect     Print the auto-detected build system for context/target(s)
  run        Run a command in context/target(s)
  self       Manage the buona binary itself
  workspace  Manage workspaces (alias: ws)
```

### `buona config`

```
buona config <COMMAND>

Commands:
  show   Display the current configuration (use --json for machine-readable output)
  setup  Launch the interactive setup wizard
  set    Set a global configuration value
  get    Get a global configuration value
  unset  Unset a global configuration value
```

Examples:

```sh
# set a primitive
buona config set git.host github.example.com

# get a nested value
buona config get git.host

# set structured JSON
buona config set git --json '{"host":"github.com","organization":"acme","protocol":"ssh","tracking":"package"}'

# unset nested key (falls back to serde default on next load/save)
buona config unset git.organization
```

### `buona workspace`

```
buona workspace <COMMAND>

Commands:
  list    List all workspaces in the configured directory
  create  Create a new workspace
  delete  Delete a workspace
  rename  Rename a workspace (updates metadata and the directory name)
  add     Add packages to a workspace
  adopt   Adopt an existing local directory into the workspace
  remove  Remove packages from a workspace
  sync    Pull latest changes for all packages and sync the workspace file
  open    Open workspace in the configured editor
  config  Get or set workspace-specific configuration values
  info    Show detailed information about a workspace
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

#### `buona workspace rename`

```
buona workspace rename <WORKSPACE> <NEW_NAME> [--keep-directory]
```

Renames a workspace. Updates the name in `buona.workspace.json`, renames the workspace directory to match, and regenerates the `.code-workspace` file under the new name. Pass `--keep-directory` to update only the metadata name and leave the directory name unchanged. Fails if another workspace or directory already uses the new name.

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

#### `buona workspace config`

```
buona workspace config <COMMAND>

Commands:
  set    Set a workspace configuration value
  get    Get a workspace configuration value
  unset  Reset a workspace configuration value to default
```

Example:

```sh
# enable root folder mount in current workspace
buona ws config set mount_root

# disable (back to default false)
buona ws config unset mount_root
```

`set/get/unset` support dotted object paths and array indexes in both global and workspace config commands (for example: `git.host`, `arr[0]`, `obj.items[1].name`). Keys must match the exact JSON field names (snake_case).

#### `buona workspace info`

```
buona workspace info [--workspace <NAME>] [--json]
```

Displays detailed information about a workspace, including its name, directory path, `.code-workspace` file status, and all tracked packages with their URLs and on-disk clone status. Pass `--json` to output the raw workspace metadata as JSON.

### `buona self`

```
buona self <COMMAND>

Commands:
  update  Check for and install updates
```

#### `buona self update`

```
buona self update [OPTIONS] [VERSION]
```

Checks for the latest release on GitHub and installs it, replacing the current binary. Downloads the prebuilt archive for your platform, verifies the SHA-256 checksum, and performs an atomic binary replacement. Installs without a checksum are refused unless `--force-insecure` is passed.

**Options:**
- `--check` — Only check for updates without installing
- `--yes` / `-y` — Skip the confirmation prompt
- `--force-insecure` — Allow install when the release has no `.sha256` checksum asset (not recommended)

**Examples:**
```sh
# Check for updates and prompt to install
buona self update

# Only check, don't install
buona self update --check

# Install without confirmation
buona self update --yes

# Install a specific version
buona self update v0.1.5
buona self update 0.1.5

# Override missing checksum (not recommended)
buona self update --force-insecure
```

## Configuration

Buona stores its configuration at `~/.config/buona/config.json`. The current settings:

| Key                | Description                                  | Default        |
|--------------------|----------------------------------------------|----------------|
| `workspace_dir`    | Root directory where workspaces are created   | `~/workspace`  |
| `ide`              | Preferred IDE (`vscode`, `cursor`, or `windsurf`) | `vscode`    |
| `git.host`         | Default git host                              | `github.com`   |
| `git.organization` | Default organization on the git host          | *(empty)*      |
| `git.protocol`     | Clone/push protocol (`ssh` or `https`)        | `ssh`          |
| `git.tracking`     | Default workspace git tracking mode (`package` or `workspace`) | `package` |

Complete `~/.config/buona/config.json` template:

```json
{
  "$schema": "./schemas/config.schema.json",
  "workspace_dir": "~/workspace",
  "ide": "vscode",
  "git": {
    "host": "github.com",
    "organization": "",
    "protocol": "ssh",
    "tracking": "package"
  }
}
```

Unknown keys are ignored when reading config files and are not written back, so legacy/abandoned keys are automatically dropped the next time buona saves config. `config set`/`add`/`remove` reject unknown keys instead of silently discarding them, so a typo'd key name is an error rather than a no-op.

## Workspace metadata

Each workspace contains a `buona.workspace.json` file:

```json
{
  "$schema": "./schemas/buona.workspace.schema.json",
  "name": "my-project",
  "git_tracking": "package",
  "mount_root": true
}
```

Packages are discovered from the workspace `src/` directory and from git remotes at runtime; they are not stored in `buona.workspace.json`.

Like global config, unknown keys in `buona.workspace.json` are ignored on read and dropped on write.

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [just](https://github.com/casey/just) (task runner)
- [cargo-release](https://github.com/crate-ci/cargo-release) (release automation)

```sh
cargo install cargo-release
```

### Commands

```sh
# Run buona with arguments
just run <args>

# Run tests
just test

# Run the full CI check suite locally (formatting, linting, tests)
just ci

# Run pre-release checks only (formatting + linting)
just pre-release

# Example commands
just config show
just workspace list
```

## Releasing

Releases are managed by [cargo-release](https://github.com/crate-ci/cargo-release), which handles version bumping in `Cargo.toml`, committing, tagging, and pushing. The tag push triggers CI, and on success, GitHub Actions builds and publishes binaries for Linux, Intel macOS, and Apple Silicon macOS.

```sh
# Dry-run (preview what will happen, no changes made)
just release patch
just release minor
just release 0.2.0

# Execute the release
just release patch --execute
```

Releases are restricted to the `main` branch. Before bumping the version, `cargo-release` runs `just pre-release` to verify formatting and linting pass.

## License

MIT
