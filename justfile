# Build the project
build:
    cargo build --release

# Run tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Format code
fmt:
    cargo fmt

# Run clippy linter
lint:
    cargo clippy -- -D warnings

# Run with sample data
run-sample:
    mkdir -p test-data/output
    cargo run --release -- \
        --source test-data \
        --destination test-data/output \
        --message-element message

# Clean build artifacts
clean:
    cargo clean
    rm -rf test-data/output

# Install the binary
install:
    cargo install --path .

# Run all checks (format, lint, test)
check: fmt lint test

# Show the output of sample run
show-output:
    @echo "=== NDJSON Output ==="
    @cat test-data/output/*.ndjson || echo "Run 'just run-sample' first"

# Run with custom message element (for trade examples)
run-trade:
    mkdir -p test-data/output
    cargo run --release -- \
        --source test-data \
        --destination test-data/output \
        --message-element message
    @echo "\n=== Output ==="
    @cat test-data/output/*.ndjson
