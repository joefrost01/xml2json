# XML to NDJSON Converter

A high-performance Rust CLI tool for converting XML files containing multiple messages into NDJSON (newline-delimited JSON) format, optimized for BigQuery ingestion.

## Features

- 🚀 **Blazing Fast**: Parallel processing using all available CPU cores via Rayon
- 💾 **Memory Efficient**: Streaming XML parsing means constant memory usage regardless of file size
- ☁️ **Cloud Native**: Works seamlessly with both local filesystems and Google Cloud Storage
- 📊 **BigQuery Ready**: NDJSON output format is perfect for BigQuery's native ingestion
- 🎯 **Simple**: Zero-config setup, just point it at your data
- 🔄 **Structure Preserving**: Maintains your XML structure in the JSON output

## Installation

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs))
- For GCS usage: Google Cloud credentials configured

### Build from Source

```bash
git clone <your-repo>
cd xml-to-ndjson
cargo build --release
```

The binary will be at `target/release/xml-to-ndjson`.

## Usage

### Basic Usage (Local Files)

```bash
xml-to-ndjson \
  --source /path/to/xml/files \
  --destination /path/to/output
```

### Google Cloud Storage

```bash
xml-to-ndjson \
  --source gs://my-bucket/xml-files \
  --destination gs://my-bucket/ndjson-files
```

### Custom Message Element

If your XML uses a different element name for messages (e.g., `<trade>` instead of `<message>`):

```bash
xml-to-ndjson \
  --source /path/to/xml \
  --destination /path/to/json \
  --message-element trade
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
{"trade":{"id":"12345","symbol":"AAPL","quantity":"100","price":"150.25"}}
{"trade":{"id":"12346","symbol":"GOOGL","quantity":"50","price":"2800.50"}}
```

## Performance

Designed to handle:
- ✅ 300M+ messages
- ✅ 10K+ files
- ✅ Parallel processing across all CPU cores
- ✅ Streaming processing for minimal memory footprint

**Example**: On a modern 8-core machine, processing 10K files with 300M total messages typically completes in minutes, not hours.

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
        /path/to/xml-to-ndjson \
          --source gs://my-bucket/kafka-sink/{{ ds }} \
          --destination gs://my-bucket/processed/{{ ds }} \
          --message-element trade
    """,
    dag=dag
)
```

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
xml-to-ndjson/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Public API
│   ├── cli.rs            # Argument parsing
│   ├── converter.rs      # Core conversion logic
│   ├── storage.rs        # Storage abstraction
│   └── error.rs          # Error types
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
  --source ./test-data/xml \
  --destination ./test-data/output
```

### Design Principles

- **Simplicity First**: The number one feature is simple
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

The tool uses streaming processing, so memory usage should remain constant regardless of file size. If you encounter memory issues:

1. Check that files aren't being loaded entirely into memory elsewhere
2. Verify the XML is well-formed (malformed XML may cause parser issues)

### Performance Tuning

By default, the tool uses all available CPU cores. If running in a constrained environment:

```bash
# Limit Rayon thread pool (set before running)
export RAYON_NUM_THREADS=4
```

## License

MIT

## Contributing

Contributions welcome! Please ensure:
- Code follows Rust best practices
- Tests pass: `cargo test`
- Formatting is correct: `cargo fmt`
- No clippy warnings: `cargo clippy`
