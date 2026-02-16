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

# Tag and publish a release (triggers GitHub Actions release workflow)
release version:
    @if ! echo "{{version}}" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then \
      echo "Error: version must match X.Y.Z (example: 0.1.1)"; \
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
