# Quick Start Guide

Get up and running with xml-to-ndjson in 5 minutes.

## 1. Install Rust

If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## 2. Build the Tool

```bash
cd xml-to-ndjson
cargo build --release
```

The binary will be at `target/release/xml-to-ndjson`.

## 3. Try with Sample Data

```bash
# Using just (recommended)
just run-sample

# Or using cargo directly
cargo run --release -- \
  --source test-data \
  --destination test-data/output \
  --message-element message
```

## 4. Check the Output

```bash
# View the converted NDJSON
cat test-data/output/*.ndjson
```

You should see JSON lines like:
```json
{"trade":{"id":"TRD-2024-001","timestamp":"2024-01-15T10:30:00Z","symbol":"AAPL",...}}
```

## 5. Use with Your Data

### Local Files

```bash
./target/release/xml-to-ndjson \
  --source /path/to/your/xml/files \
  --destination /path/to/output \
  --message-element message
```

### Google Cloud Storage

First, authenticate:
```bash
gcloud auth application-default login
```

Then run:
```bash
./target/release/xml-to-ndjson \
  --source gs://your-bucket/xml-files \
  --destination gs://your-bucket/ndjson-files \
  --message-element message
```

## 6. Load into BigQuery

```bash
bq load \
  --source_format=NEWLINE_DELIMITED_JSON \
  --autodetect \
  your_dataset.your_table \
  gs://your-bucket/ndjson-files/*.ndjson
```

## Common Workflows

### Development Loop

```bash
# Check code quality
just check

# Run tests
just test

# Run with sample data
just run-sample

# View output
just show-output
```

### Production Deployment

```bash
# Build optimized release
cargo build --release

# Copy to Cloud Composer bucket
gsutil cp target/release/xml-to-ndjson gs://your-composer-bucket/data/bin/

# Deploy your DAG
gsutil cp examples/composer_dag.py gs://your-composer-bucket/dags/
```

## Troubleshooting

### "No XML files found"

Make sure your files have a `.xml` extension.

### "Failed to parse XML"

Check that your XML is well-formed:
```bash
xmllint --noout your-file.xml
```

### GCS Permission Errors

Ensure your credentials have the necessary permissions:
```bash
gcloud auth application-default login
# or
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json
```

## Next Steps

- Read the [full README](README.md) for detailed documentation
- Explore the [Cloud Composer example](examples/composer_dag.py)
- Check the source code for customization options
