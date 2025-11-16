//! Error types for the XML to NDJSON converter.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Failed to read file: {0}")]
    ReadError(String),

    #[error("Failed to write file: {0}")]
    WriteError(String),

    #[error("Failed to parse XML: {0}")]
    XmlParseError(String),

    #[error("Failed to serialize JSON: {0}")]
    JsonSerializeError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("XML error: {0}")]
    QuickXmlError(#[from] quick_xml::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ConversionError>;
