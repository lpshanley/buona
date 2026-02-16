# Run buona with optional arguments
run *args:
    cargo run -- {{args}}

# Run buona config with optional arguments
config *args:
    cargo run -- config {{args}}

# Run buona workspace with optional arguments
workspace *args:
    cargo run -- workspace {{args}}

# Run tests
test *args:
    cargo test {{args}}

# Install buona to the global Cargo bin (~/.cargo/bin)
install:
    cargo install --path .

# Set up development environment (git hooks)
setup:
    git config core.hooksPath .githooks
    @echo "Git hooks configured to use .githooks/"

# Run the full CI check suite locally (mirrors GitHub Actions)
ci:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-targets --all-features

# Run pre-release checks (formatting + linting)
pre-release:
    @echo "Running pre-release checks..."
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    @echo "Pre-release checks passed!"

# Tag and publish a release (triggers GitHub Actions release workflow)
release version:
    @if ! echo "{{version}}" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then \
      echo "Error: version must match X.Y.Z (example: 0.1.1)"; \
      exit 1; \
    fi
    @if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then \
      echo "Error: releases must be tagged from the main branch. Current branch: $(git rev-parse --abbrev-ref HEAD)"; \
      exit 1; \
    fi
    @git fetch origin main --quiet
    @if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then \
      echo "Error: local main is not up to date with origin/main. Run 'git pull' first."; \
      exit 1; \
    fi
    @if [ -n "$(git status --porcelain)" ]; then \
      echo "Error: git worktree is dirty. Commit or stash changes before releasing."; \
      exit 1; \
    fi
    @if git rev-parse "v{{version}}" >/dev/null 2>&1; then \
      echo "Error: tag v{{version}} already exists."; \
      exit 1; \
    fi
    just pre-release
    git tag -a "v{{version}}" -m "Release v{{version}}"
    git push origin "v{{version}}"

# Generate HTML coverage report (line + branch)
coverage:
    cargo tarpaulin --out Html --target-dir target/coverage

# Generate and open HTML coverage report
coverage-open:
    cargo tarpaulin --out Html --target-dir target/coverage && open target/coverage/tarpaulin-report.html

# Generate LCOV coverage report for IDE integration
coverage-lcov:
    cargo tarpaulin --out Lcov --target-dir target/coverage

# Generate text coverage report to console
coverage-text:
    cargo tarpaulin --out Stdout

# Clean coverage artifacts
clean-coverage:
    rm -rf target/coverage
