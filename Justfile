# Run unit tests
test:
    cargo test

# Code coverage report
cov:
    # Needs package cargo-tarpaulin
    cargo tarpaulin -v

# Constantly running coverage monitor
watch-cov:
    cargo watch -x tarpaulin
