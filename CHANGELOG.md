# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2024-01-15

### Added
- Initial release
- XML to NDJSON conversion with streaming processing
- Parallel processing using Rayon (all CPU cores)
- Support for local filesystem
- Support for Google Cloud Storage (GCS)
- Storage abstraction trait for extensibility
- Configurable message element name
- Progress bar with indicatif
- Comprehensive error handling
- CLI with clap
- Test suite
- Documentation and examples
- Cloud Composer DAG example
- Deployment script for GCS/Composer
- Sample test data

### Features
- Streaming XML parsing for constant memory usage
- Parallel file processing across all available cores
- Preserves XML structure in JSON output
- Cross-platform compatibility
- Zero-config setup for local use
- BigQuery-ready NDJSON output format

### Performance
- Optimized for 300M+ messages across 10K+ files
- Memory-efficient streaming processing
- File-level parallelism with Rayon
- Minimal memory footprint regardless of file size

## [Unreleased]

### Planned
- [ ] Parquet output format option
- [ ] Avro output format option
- [ ] Schema validation
- [ ] Custom XML namespace handling
- [ ] Compression support (gzip, snappy)
- [ ] Metrics and observability hooks
- [ ] Batch size configuration
- [ ] Resume capability for failed conversions
