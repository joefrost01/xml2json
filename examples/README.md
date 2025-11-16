# Test Data Generator

Generate realistic XML trade message files for benchmarking.

## Usage

```bash
# Generate 100 files with 10,000 messages each (1M total messages)
cargo run --release --example generate_test_data 100 10000 ./test-data-large

# Generate 1,000 files with 30,000 messages each (30M total messages - 10% of your load)
cargo run --release --example generate_test_data 1000 30000 ./benchmark-data

# Default: 100 files with 10,000 messages
cargo run --release --example generate_test_data
```

## Parameters

1. **num_files** - Number of XML files to generate (default: 100)
2. **messages_per_file** - Messages per file (default: 10,000)
3. **output_dir** - Output directory (default: ./test-data-large)

## Generated Data

Each file contains trade messages with:
- Unique trade IDs
- Random symbols (AAPL, GOOGL, MSFT, etc.)
- Random quantities and prices
- Trader information with nested elements
- Timestamps
- Various market venues

## Benchmark Recommendations

**Quick test (1M messages):**
```bash
cargo run --release --example generate_test_data 100 10000 ./bench-1m
time cargo run --release -- --source ./bench-1m --destination ./output-1m
```

**Realistic test (10% of load - 30M messages):**
```bash
cargo run --release --example generate_test_data 1000 30000 ./bench-30m
time cargo run --release -- --source ./bench-30m --destination ./output-30m
```

**Full scale test (100% of load - 300M messages):**
```bash
cargo run --release --example generate_test_data 10000 30000 ./bench-300m
time cargo run --release -- --source ./bench-300m --destination ./output-300m
```

## File Size Estimates

Each message is ~350 bytes of XML.

- 1M messages = ~350 MB
- 10M messages = ~3.5 GB
- 30M messages = ~10.5 GB
- 300M messages = ~105 GB

Make sure you have enough disk space!

## Testing with File Moves

```bash
cargo run --release -- \
  --source ./bench-data \
  --destination ./output \
  --success-dir ./processed \
  --error-dir ./failed
```
