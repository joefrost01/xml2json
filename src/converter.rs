//! Core conversion logic for transforming XML messages to NDJSON.
//!
//! This module handles streaming XML parsing, conversion to JSON,
//! and parallel processing of multiple files.

use crate::error::{ConversionError, Result};
use crate::storage::Storage;
use indicatif::{ProgressBar, ProgressStyle};
use quick_xml::events::Event;
use quick_xml::{Reader, Writer as XmlWriter};
use rayon::prelude::*;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use serde::ser::SerializeMap;
use serde::ser::Serializer;
use serde_json::ser::Serializer as JsonSerializer;

// Buffer size constants for optimization
const XML_BUFFER_CAPACITY: usize = 32768;
const OUTPUT_BUFFER_CAPACITY: usize = 32768;

/// Configuration for the conversion process
#[derive(Debug, Clone)]
pub struct ConverterConfig {
    /// Root XML element name that wraps individual messages
    pub message_element: String,
    /// Whether to show progress bars
    pub show_progress: bool,
    /// Directory to move successfully processed files to
    pub success_dir: Option<String>,
    /// Directory to move failed files to
    pub error_dir: Option<String>,
}

impl Default for ConverterConfig {
    fn default() -> Self {
        Self {
            message_element: "message".to_string(),
            show_progress: true,
            success_dir: None,
            error_dir: None,
        }
    }
}

/// Main converter that orchestrates the conversion process
pub struct Converter {
    config: ConverterConfig,
    /// Cached bytes for the message element name to avoid per-event String allocations
    message_element_bytes: Vec<u8>,
}

impl Converter {
    /// Create a new converter with the given configuration
    pub fn new(config: ConverterConfig) -> Self {
        let message_element_bytes = config.message_element.as_bytes().to_vec();
        Self {
            config,
            message_element_bytes,
        }
    }

    /// Convert all XML files from source to NDJSON in destination
    pub fn convert_directory(
        &self,
        storage: &dyn Storage,
        source: &str,
        destination: &str,
    ) -> Result<ConversionStats> {
        // List all XML files in the source
        let files = storage.list_files(source)?;

        if files.is_empty() {
            return Err(ConversionError::StorageError(
                "No XML files found in source directory".to_string(),
            ));
        }

        println!("Found {} XML files to process", files.len());

        // Set up progress bar
        let progress = if self.config.show_progress {
            let pb = ProgressBar::new(files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=>-"),
            );
            Some(pb)
        } else {
            None
        };

        // Process files in parallel using Rayon
        let results: Vec<Result<FileStats>> = files
            .par_iter()
            .map(|source_path| {
                let result = self.convert_file(storage, source_path, source, destination);

                // Move file based on result
                match &result {
                    Ok(_) => {
                        if let Some(ref success_dir) = self.config.success_dir {
                            let dest_path =
                                self.calculate_move_path(source_path, source, success_dir);
                            if let Err(e) = storage.move_file(source_path, &dest_path) {
                                eprintln!(
                                    "Warning: Failed to move {} to success dir: {}",
                                    source_path, e
                                );
                            }
                        }
                    }
                    Err(_) => {
                        if let Some(ref error_dir) = self.config.error_dir {
                            let dest_path =
                                self.calculate_move_path(source_path, source, error_dir);
                            if let Err(e) = storage.move_file(source_path, &dest_path) {
                                eprintln!(
                                    "Warning: Failed to move {} to error dir: {}",
                                    source_path, e
                                );
                            }
                        }
                    }
                }

                if let Some(ref pb) = progress {
                    pb.inc(1);
                }
                result
            })
            .collect();

        if let Some(pb) = progress {
            pb.finish_with_message("Conversion complete");
        }

        // Aggregate results
        let mut total_stats = ConversionStats::default();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(stats) => {
                    total_stats.files_processed += 1;
                    total_stats.messages_converted += stats.messages_converted;
                    total_stats.bytes_processed += stats.bytes_processed;
                }
                Err(e) => {
                    total_stats.files_failed += 1;
                    errors.push(e.to_string());
                }
            }
        }

        if !errors.is_empty() {
            eprintln!("\nErrors encountered:");
            for error in &errors {
                eprintln!("  - {}", error);
            }
        }

        Ok(total_stats)
    }

    /// Convert a single XML file to NDJSON
    fn convert_file(
        &self,
        storage: &dyn Storage,
        source_path: &str,
        source_base: &str,
        dest_base: &str,
    ) -> Result<FileStats> {
        // Calculate destination path by replacing source base with dest base
        let dest_path = self.calculate_dest_path(source_path, source_base, dest_base);

        // Open source file for reading
        let reader = storage.read(source_path)?;
        let buf_reader = BufReader::new(reader);

        // Open destination file for writing
        let mut writer = storage.write(&dest_path)?;

        // Convert XML to NDJSON
        let stats = self.convert_xml_to_ndjson(buf_reader, &mut writer)?;

        Ok(stats)
    }

    /// Calculate the destination path based on source path
    fn calculate_dest_path(&self, source: &str, source_base: &str, dest_base: &str) -> String {
        // Handle both local paths and GCS paths
        let relative = if source.starts_with("gs://") && source_base.starts_with("gs://") {
            // For GCS, strip the bucket and base path
            source.strip_prefix(source_base).unwrap_or(source)
        } else {
            // For local paths
            Path::new(source)
                .strip_prefix(source_base)
                .unwrap_or(Path::new(source))
                .to_str()
                .unwrap_or(source)
        };

        // Replace .xml extension with .ndjson
        let relative = relative.trim_start_matches('/');
        let relative_json = if relative.ends_with(".xml") {
            relative.replace(".xml", ".ndjson")
        } else {
            format!("{}.ndjson", relative)
        };

        // Combine with destination base
        if dest_base.starts_with("gs://") {
            format!("{}/{}", dest_base.trim_end_matches('/'), relative_json)
        } else {
            Path::new(dest_base)
                .join(relative_json)
                .display()
                .to_string()
        }
    }

    /// Calculate path for moving files (preserves directory structure and filename)
    fn calculate_move_path(&self, source: &str, source_base: &str, dest_base: &str) -> String {
        // Handle both local paths and GCS paths
        let relative = if source.starts_with("gs://") && source_base.starts_with("gs://") {
            // For GCS, strip the bucket and base path
            source.strip_prefix(source_base).unwrap_or(source)
        } else {
            // For local paths
            Path::new(source)
                .strip_prefix(source_base)
                .unwrap_or(Path::new(source))
                .to_str()
                .unwrap_or(source)
        };

        let relative = relative.trim_start_matches('/');

        // Combine with destination base
        if dest_base.starts_with("gs://") {
            format!("{}/{}", dest_base.trim_end_matches('/'), relative)
        } else {
            Path::new(dest_base).join(relative).display().to_string()
        }
    }

    /// Convert XML stream to NDJSON, writing each message as a line
    ///
    /// OPTIMIZED VERSION: Reuses buffers, minimizes allocations, and streams efficiently
    pub fn convert_xml_to_ndjson<R: BufRead, W: Write>(
        &self,
        reader: R,
        writer: &mut W,
    ) -> Result<FileStats> {
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.config_mut().trim_text(true);

        let mut buf = Vec::new();

        // Pre-allocate reusable buffers for efficiency
        let mut xml_buffer = Vec::with_capacity(XML_BUFFER_CAPACITY);
        let mut output_buffer = Vec::with_capacity(OUTPUT_BUFFER_CAPACITY);

        let mut inside_message = false;
        let mut depth: u32 = 0;
        let mut stats = FileStats::default();

        // Writer that borrows xml_buffer while we’re inside a message
        let mut xml_writer: Option<XmlWriter<&mut Vec<u8>>> = None;

        loop {
            buf.clear();
            let event = xml_reader
                .read_event_into(&mut buf)
                .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;

            match &event {
                Event::Eof => break,

                Event::Start(e) => {
                    let name_tmp = e.name();
                    let name = name_tmp.as_ref();

                    if name == &*self.message_element_bytes && depth == 0 {
                        inside_message = true;
                        xml_buffer.clear();
                        xml_writer = Some(XmlWriter::new(&mut xml_buffer));
                    }

                    if inside_message {
                        if let Some(writer) = xml_writer.as_mut() {
                            writer
                                .write_event(event.clone())
                                .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                        }
                        depth += 1;
                    }
                }

                Event::End(e) => {
                    if inside_message {
                        let name_tmp = e.name();
                        let name = name_tmp.as_ref();

                        if let Some(writer) = xml_writer.as_mut() {
                            writer
                                .write_event(event.clone())
                                .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                        }

                        if depth > 0 {
                            depth -= 1;
                        }

                        if name == &*self.message_element_bytes && depth == 0 {
                            // Drop writer so &mut xml_buffer is released
                            xml_writer = None;

                            let bytes_processed = xml_buffer.len() as u64;

                            output_buffer.clear();
                            self.xml_to_json_optimized(
                                &self.config.message_element,
                                &xml_buffer,
                                &mut output_buffer,
                            )?;

                            writer.write_all(&output_buffer)?;
                            writer.write_all(b"\n")?;

                            stats.messages_converted += 1;
                            stats.bytes_processed += bytes_processed;

                            inside_message = false;
                            xml_buffer.clear();
                        }
                    }
                }

                Event::Empty(e) => {
                    let name_tmp = e.name();
                    let name = name_tmp.as_ref();

                    if name == &*self.message_element_bytes && depth == 0 {
                        inside_message = true;
                        xml_buffer.clear();
                        xml_writer = Some(XmlWriter::new(&mut xml_buffer));
                    }

                    if inside_message {
                        if let Some(writer) = xml_writer.as_mut() {
                            writer
                                .write_event(event.clone())
                                .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                        }

                        // For an empty element, depth doesn’t change, but this might be a whole message
                        if name == &*self.message_element_bytes && depth == 0 {
                            xml_writer = None;

                            let bytes_processed = xml_buffer.len() as u64;

                            output_buffer.clear();
                            self.xml_to_json_optimized(
                                &self.config.message_element,
                                &xml_buffer,
                                &mut output_buffer,
                            )?;

                            writer.write_all(&output_buffer)?;
                            writer.write_all(b"\n")?;

                            stats.messages_converted += 1;
                            stats.bytes_processed += bytes_processed;

                            inside_message = false;
                            xml_buffer.clear();
                        }
                    }
                }

                Event::Text(_)
                | Event::CData(_)
                | Event::Comment(_)
                | Event::Decl(_)
                | Event::PI(_) => {
                    if inside_message {
                        if let Some(writer) = xml_writer.as_mut() {
                            writer
                                .write_event(event.clone())
                                .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                        }
                    }
                }

                _ => {}
            }
        }

        Ok(stats)
    }



    /// Optimized XML to JSON conversion that writes directly to output buffer
    /// Minimizes intermediate allocations and conversions
    fn xml_to_json_optimized(
        &self,
        root_name: &str,
        xml_bytes: &[u8],
        output: &mut Vec<u8>,
    ) -> Result<()> {
        let xml_str = std::str::from_utf8(xml_bytes)
            .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;

        let mut inner_value: Value = quick_xml::de::from_str(xml_str)
            .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;

        self.unwrap_text_fields_in_place(&mut inner_value);

        output.clear();

        let mut ser = JsonSerializer::new(&mut *output);
        let mut map = ser
            .serialize_map(Some(1))
            .map_err(|e| ConversionError::JsonSerializeError(e.to_string()))?;

        // key: root_name, value: inner_value
        map.serialize_entry(root_name, &inner_value)
            .map_err(|e| ConversionError::JsonSerializeError(e.to_string()))?;
        map.end()
            .map_err(|e| ConversionError::JsonSerializeError(e.to_string()))?;

        Ok(())
    }

    fn unwrap_text_fields_in_place(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                // If object is exactly { "$text": <something> } then collapse to <something>
                if map.len() == 1 {
                    if let Some(v) = map.remove("$text") {
                        *value = v;
                        return;
                    }
                }

                // Otherwise recurse into children
                for v in map.values_mut() {
                    self.unwrap_text_fields_in_place(v);
                }
            }
            Value::Array(arr) => {
                for v in arr.iter_mut() {
                    self.unwrap_text_fields_in_place(v);
                }
            }
            _ => {
                // primitives: nothing to do
            }
        }
    }
}


/// Statistics for a single file conversion
#[derive(Debug, Default)]
pub struct FileStats {
    pub messages_converted: u64,
    pub bytes_processed: u64,
}

/// Aggregated statistics for the entire conversion
#[derive(Debug, Default)]
pub struct ConversionStats {
    pub files_processed: u64,
    pub files_failed: u64,
    pub messages_converted: u64,
    pub bytes_processed: u64,
}

impl ConversionStats {
    /// Print a summary of the conversion
    pub fn print_summary(&self) {
        println!("\n=== Conversion Summary ===");
        println!("Files processed: {}", self.files_processed);
        println!("Files failed: {}", self.files_failed);
        println!("Messages converted: {}", self.messages_converted);
        println!(
            "Data processed: {:.2} MB",
            self.bytes_processed as f64 / 1_000_000.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalStorage;
    use tempfile::TempDir;

    #[test]
    fn test_calculate_dest_path_local() {
        let converter = Converter::new(ConverterConfig::default());

        let dest = converter.calculate_dest_path(
            "/source/dir/subdir/file.xml",
            "/source/dir",
            "/dest/dir",
        );

        assert_eq!(dest, "/dest/dir/subdir/file.ndjson");
    }

    #[test]
    fn test_calculate_dest_path_gcs() {
        let converter = Converter::new(ConverterConfig::default());

        let dest = converter.calculate_dest_path(
            "gs://bucket/source/subdir/file.xml",
            "gs://bucket/source",
            "gs://bucket/dest",
        );

        assert_eq!(dest, "gs://bucket/dest/subdir/file.ndjson");
    }

    #[test]
    fn test_calculate_move_path_preserves_extension() {
        let converter = Converter::new(ConverterConfig::default());

        let dest =
            converter.calculate_move_path("/source/dir/file.xml", "/source/dir", "/moved/dir");

        assert_eq!(dest, "/moved/dir/file.xml");
    }

    #[test]
    fn test_convert_xml_to_ndjson_single_message() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<?xml version="1.0"?>
<root>
  <message>
    <trade><id>1</id><symbol>AAPL</symbol></trade>
  </message>
</root>"#;

        let reader = std::io::Cursor::new(xml.as_bytes());
        let mut output = Vec::new();

        let stats = converter
            .convert_xml_to_ndjson(reader, &mut output)
            .unwrap();

        assert_eq!(stats.messages_converted, 1);
        assert!(stats.bytes_processed > 0);

        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed: Value = serde_json::from_str(lines[0]).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_convert_xml_to_ndjson_multiple_messages() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<?xml version="1.0"?>
<root>
  <message>
    <trade><id>1</id></trade>
  </message>
  <message>
    <trade><id>2</id></trade>
  </message>
  <message>
    <trade><id>3</id></trade>
  </message>
</root>"#;

        let reader = std::io::Cursor::new(xml.as_bytes());
        let mut output = Vec::new();

        let stats = converter
            .convert_xml_to_ndjson(reader, &mut output)
            .unwrap();

        assert_eq!(stats.messages_converted, 3);

        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().lines().collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_convert_xml_to_ndjson_custom_element() {
        let config = ConverterConfig {
            message_element: "trade".to_string(),
            show_progress: false,
            success_dir: None,
            error_dir: None,
        };
        let converter = Converter::new(config);

        let xml = r#"<?xml version="1.0"?>
<root>
  <trade><id>1</id></trade>
  <trade><id>2</id></trade>
</root>"#;

        let reader = std::io::Cursor::new(xml.as_bytes());
        let mut output = Vec::new();

        let stats = converter
            .convert_xml_to_ndjson(reader, &mut output)
            .unwrap();

        assert_eq!(stats.messages_converted, 2);
    }

    #[test]
    fn test_end_to_end_conversion() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let source_dir = temp_dir.path().join("source");
        let dest_dir = temp_dir.path().join("dest");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&dest_dir).unwrap();

        let source_file = source_dir.join("test.xml");
        let xml_content = r#"<?xml version="1.0"?>
<root>
  <message><trade><id>1</id><symbol>AAPL</symbol></trade></message>
  <message><trade><id>2</id><symbol>GOOGL</symbol></trade></message>
</root>"#;
        std::fs::write(&source_file, xml_content).unwrap();

        let converter = Converter::new(ConverterConfig::default());
        let stats = converter
            .convert_directory(
                &storage,
                &source_dir.display().to_string(),
                &dest_dir.display().to_string(),
            )
            .unwrap();

        assert_eq!(stats.files_processed, 1);
        assert_eq!(stats.messages_converted, 2);
        assert_eq!(stats.files_failed, 0);

        let output_file = dest_dir.join("test.ndjson");
        assert!(output_file.exists());

        let content = std::fs::read_to_string(output_file).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        for line in lines {
            let parsed: Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }
}
