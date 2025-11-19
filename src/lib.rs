//! XML to NDJSON converter library
//!
//! This library provides functionality to convert XML files containing multiple
//! messages into NDJSON (newline-delimited JSON) format, optimized for BigQuery
//! ingestion.
//!
//! # Features
//!
//! - **Parallel processing**: Uses Rayon to process multiple files concurrently
//! - **Streaming**: Processes files in a streaming fashion to minimize memory usage
//! - **Cross-platform**: Works on local filesystems and Google Cloud Storage
//! - **Efficient**: Designed to handle hundreds of millions of messages
//!
//! # Example
//!
//! ```no_run
//! use xml2json::{create_storage, Converter, ConverterConfig};
//!
//! let config = ConverterConfig {
//!     message_element: "trade".to_string(),
//!     show_progress: true,
//!     success_dir: None,error_dir: None,};
//!
//! let converter = Converter::new(config);
//! let storage = create_storage("/path/to/source").unwrap();
//!
//! let stats = converter.convert_directory(
//!     storage.as_ref(),
//!     "/path/to/source",
//!     "/path/to/destination"
//! ).unwrap();
//!
//! stats.print_summary();
//! ```

pub mod cli;
pub mod converter;
pub mod error;
pub mod storage;
pub mod jnode;

// Re-export main types for convenience
pub use converter::{Converter, ConverterConfig, ConversionStats};
pub use error::{ConversionError, Result};
pub use storage::{create_storage, Storage};
