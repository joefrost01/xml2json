# XML to NDJSON Converter

A high-performance Rust CLI tool for converting XML files containing multiple messages into NDJSON (newline-delimited JSON) format, optimized for BigQuery ingestion.

## Features

- 🚀 **Fast**: Parallel processing using all available CPU cores via Rayon
- 💾 **Memory Efficient**: Streaming XML parsing with constant memory usage
- ☁️ **Cloud Native**: Works with both local filesystems and Google Cloud Storage
- 📊 **BigQuery Ready**: NDJSON output is perfect for BigQuery ingestion
- 🎯 **Simple**: Point it at your data and run
- 🔄 **Structure Preserving**: Maintains XML structure in JSON output

## Performance

Real-world benchmarks on a MacBook Pro M2 (10 cores):

| Messages | Files | Data Size | Time | Throughput |
|----------|-------|-----------|------|------------|
| 10M | 100 | 3.0 GB | 9.8s | ~1M msg/s |
| 100M | 1,000 | 30 GB | ~1.6 min | ~1M msg/s |
| 300M | 10,000 | 90 GB | ~5 min | ~1M msg/s |

The tool scales linearly with CPU cores and maintains consistent throughput regardless of dataset size.

## Installation

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs))
- For GCS: Google Cloud credentials configured

### Build

```bash
cargo build --release
```

The binary will be at `target/release/xml2json`.

## Usage

### Basic (Local Files)

```bash
xml2json \
  --source /path/to/xml/files \
  --destination /path/to/output
```

### Google Cloud Storage

```bash
xml2json \
  --source gs://my-bucket/xml-files \
  --destination gs://my-bucket/ndjson-files
```

### Custom Message Element

If your XML uses a different element name for messages:

```bash
xml2json \
  --source /path/to/xml \
  --destination /path/to/json \
  --message-element trade
```

### File Management

Move successfully processed and failed files to separate directories:

```bash
xml2json \
  --source /path/to/xml \
  --destination /path/to/json \
  --success-dir /path/to/processed \
  --error-dir /path/to/failed
```

### All Options

```
Options:
  -s, --source <SOURCE>
          Source directory or GCS bucket path
          
  -d, --destination <DESTINATION>
          Destination directory or GCS bucket path
          
  -m, --message-element <MESSAGE_ELEMENT>
          XML element name that wraps each message [default: message]
          
      --success-dir <SUCCESS_DIR>
          Directory to move successfully processed files to
          
      --error-dir <ERROR_DIR>
          Directory to move failed files to
          
      --no-progress
          Disable progress bar
          
  -h, --help
          Print help
          
  -V, --version
          Print version
```

## Input Format

The tool expects XML files where each file contains one or more messages wrapped in a root element:

```xml
<?xml version="1.0"?>
<messages>
  <message>
    <trade>
      <id>12345</id>
      <symbol>AAPL</symbol>
      <quantity>100</quantity>
      <price>150.25</price>
    </trade>
  </message>
  <message>
    <trade>
      <id>12346</id>
      <symbol>GOOGL</symbol>
      <quantity>50</quantity>
      <price>2800.50</price>
    </trade>
  </message>
</messages>
```

## Output Format

Each message becomes a single line of JSON (NDJSON):

```json
{"message":{"trade":{"id":"12345","symbol":"AAPL","quantity":"100","price":"150.25"}}}
{"message":{"trade":{"id":"12346","symbol":"GOOGL","quantity":"50","price":"2800.50"}}}
```

## Architecture

The tool is built with composable components:

```
CLI Args → Storage Abstraction → Parallel Processing (Rayon)
                 ↓
    Per-file: Stream XML → Parse → Convert → Write NDJSON
```

### Key Components

- **Storage Abstraction**: Unified interface for local and GCS storage
- **Streaming Parser**: Memory-efficient XML processing via `quick-xml`
- **Parallel Processing**: File-level parallelism via `rayon`
- **Structure Preservation**: XML structure maintained in JSON output

## Using in Google Cloud Composer

Create a DAG that runs the converter:

```python
from airflow import DAG
from airflow.operators.bash import BashOperator
from datetime import datetime

dag = DAG(
    'xml_to_ndjson_converter',
    start_date=datetime(2024, 1, 1),
    schedule_interval='@daily'
)

convert_task = BashOperator(
    task_id='convert_xml',
    bash_command="""
        /home/airflow/gcs/data/bin/xml2json \
          --source gs://my-bucket/kafka-sink/{{ ds }} \
          --destination gs://my-bucket/processed/{{ ds }} \
          --message-element trade
    """,
    dag=dag
)
```

See `examples/composer_dag.py` for a complete example including BigQuery loading.

## BigQuery Ingestion

After conversion, load into BigQuery:

```bash
bq load \
  --source_format=NEWLINE_DELIMITED_JSON \
  --autodetect \
  my_dataset.my_table \
  gs://my-bucket/ndjson-files/*.ndjson
```

## Development

### Project Structure

```
xml2json/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Public API
│   ├── cli.rs            # Argument parsing
│   ├── converter.rs      # Core conversion logic
│   ├── storage.rs        # Storage abstraction
│   └── error.rs          # Error types
├── examples/
│   ├── generate_test_data.rs
│   └── composer_dag.py
├── Cargo.toml
└── README.md
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Test with sample data
cargo run -- \
  --source ./test-data \
  --destination ./output
```

### Generating Test Data

Use the included example to generate realistic test data:

```bash
# Generate 100 files with 100,000 messages each (10M messages, 3GB)
cargo run --release --example generate_test_data 100 100000 ./bench-10m

# Generate 1,000 files with 100,000 messages each (100M messages, 30GB)
cargo run --release --example generate_test_data 1000 100000 ./bench-100m
```

See `examples/README.md` for more details on benchmarking.

### Design Principles

- **Simplicity First**: Simple is the number one feature
- **Pragmatic**: Clean code optimized for the problem space
- **Modular**: Composable components that intersect with the domain
- **Idiomatic**: Rust best practices where they aid readability
- **Well-Documented**: Critical for adoption and maintenance

## Troubleshooting

### GCS Authentication

Ensure you have Google Cloud credentials configured:

```bash
# Set up Application Default Credentials
gcloud auth application-default login

# Or use a service account
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account-key.json
```

### Memory Usage

The tool uses streaming processing, so memory usage remains constant. If you encounter memory issues:

1. Check that files aren't being loaded entirely into memory elsewhere
2. Verify the XML is well-formed (malformed XML may cause parser issues)

### Performance Tuning

By default, the tool uses all available CPU cores. To limit CPU usage:

```bash
# Limit Rayon thread pool (set before running)
export RAYON_NUM_THREADS=4
```

## License

[MIT License](LICENSE)

## Contributing

Contributions welcome! Please ensure:
- Code follows Rust best practices
- Tests pass: `cargo test`
- Formatting is correct: `cargo fmt`
- No clippy warnings: `cargo clippy`