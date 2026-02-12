# Run buona with optional arguments
run *args:
    cargo run -- {{args}}

# Run buona config with optional arguments
config *args:
    cargo run -- config {{args}}

# Install buona to the global Cargo bin (~/.cargo/bin)
install:
    cargo install --path .
