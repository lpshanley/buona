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
