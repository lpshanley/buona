# Run buona with optional arguments
run *args:
    cargo run -- {{args}}

# Install buona to the global Cargo bin (~/.cargo/bin)
install:
    cargo install --path .
