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
