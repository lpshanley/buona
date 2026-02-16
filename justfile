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

# Publish a release: bumps version, commits, tags, and pushes (triggers CI → release)
# Usage: just release patch|minor|major|X.Y.Z
# Dry-run by default — pass --execute to actually release
release *args:
    cargo release {{args}}

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
