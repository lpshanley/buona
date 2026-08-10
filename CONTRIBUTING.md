# Contributing to Buona

## Prerequisites

- Rust 1.88 or newer
- `just`
- `cargo-release` only when performing an authorized release

Set up the repository's Git hooks with:

```sh
just setup
```

## Development workflow

```sh
# Run Buona with arguments
just run <args>

# Run tests
just test

# Verify schemas, fixtures, and generated CLI help
just docs-check

# Run the full CI-equivalent suite
just ci
```

Add unit tests beside pure logic and integration tests under `tests/` for CLI
behavior. Changes to persisted configuration must update the matching schema,
fixtures, README, and tests.

See [docs/architecture.md](docs/architecture.md) for module boundaries and
[docs/agent-interface.md](docs/agent-interface.md) for the machine-readable
contract.

## Releasing

Releases use `cargo-release`. It updates the version, commits, tags, and pushes;
the tag triggers GitHub Actions to build and publish platform archives and
checksums. Releases are restricted to `main`.

Preview a release without changing the repository:

```sh
just release patch
just release minor
just release 1.2.3
```

Only when explicitly authorized, execute it with:

```sh
just release patch --execute
```

The pre-release hook runs formatting, Clippy with warnings denied, and the full
test suite.
