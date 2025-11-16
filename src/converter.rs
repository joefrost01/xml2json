//! Core conversion logic for transforming XML messages to NDJSON.
//!
//! This module handles streaming XML parsing, conversion to JSON,
//! and parallel processing of multiple files.

use crate::error::{ConversionError, Result};
use crate::storage::Storage;
use indicatif::{ProgressBar, ProgressStyle};
use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

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
}

impl Converter {
    /// Create a new converter with the given configuration
    pub fn new(config: ConverterConfig) -> Self {
        Self { config }
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
                            let dest_path = self.calculate_move_path(source_path, source, success_dir);
                            if let Err(e) = storage.move_file(source_path, &dest_path) {
                                eprintln!("Warning: Failed to move {} to success dir: {}", source_path, e);
                            }
                        }
                    }
                    Err(_) => {
                        if let Some(ref error_dir) = self.config.error_dir {
                            let dest_path = self.calculate_move_path(source_path, source, error_dir);
                            if let Err(e) = storage.move_file(source_path, &dest_path) {
                                eprintln!("Warning: Failed to move {} to error dir: {}", source_path, e);
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
            Path::new(dest_base)
                .join(relative)
                .display()
                .to_string()
        }
    }

    /// Convert XML stream to NDJSON, writing each message as a line
    fn convert_xml_to_ndjson<R: BufRead, W: Write>(
        &self,
        reader: R,
        writer: &mut W,
    ) -> Result<FileStats> {
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut xml_writer = quick_xml::Writer::new(Vec::new());
        let mut inside_message = false;
        let mut depth = 0;

        let mut stats = FileStats::default();

        loop {
            let event = xml_reader.read_event_into(&mut buf)?;
            
            match event {
                Event::Start(ref e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    if name == self.config.message_element && depth == 0 {
                        inside_message = true;
                        xml_writer = quick_xml::Writer::new(Vec::new());
                    }

                    if inside_message {
                        xml_writer.write_event(Event::Start(e.clone()))?;
                        depth += 1;
                    }
                }
                Event::End(ref e) => {
                    if inside_message {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        xml_writer.write_event(Event::End(e.clone()))?;
                        depth -= 1;

                        if name == self.config.message_element && depth == 0 {
                            // Get the XML string from the writer
                            let xml_bytes = xml_writer.into_inner();
                            let xml_str = String::from_utf8_lossy(&xml_bytes).to_string();
                            
                            // Convert to JSON and write
                            let json_line = self.xml_to_json(&xml_str)?;
                            writeln!(writer, "{}", json_line).map_err(|e| {
                                ConversionError::WriteError(format!("Failed to write JSON: {}", e))
                            })?;

                            stats.messages_converted += 1;
                            stats.bytes_processed += xml_bytes.len() as u64;

                            inside_message = false;
                            xml_writer = quick_xml::Writer::new(Vec::new());
                        }
                    }
                }
                Event::Text(ref e) => {
                    if inside_message {
                        xml_writer.write_event(Event::Text(e.clone()))?;
                    }
                }
                Event::CData(ref e) => {
                    if inside_message {
                        xml_writer.write_event(Event::CData(e.clone()))?;
                    }
                }
                Event::Empty(ref e) => {
                    if inside_message {
                        xml_writer.write_event(Event::Empty(e.clone()))?;
                    }
                }
                Event::Eof => break,
                _ => {
                    if inside_message {
                        xml_writer.write_event(event.clone())?;
                    }
                }
            }

            buf.clear();
        }

        Ok(stats)
    }

    /// Convert a single XML message string to a JSON line
    fn xml_to_json(&self, xml: &str) -> Result<String> {
        // Extract root element name from the XML
        let root_name = self.extract_root_element_name(xml)?;
        
        // Parse the XML string into a serde_json::Value
        // quick-xml deserializes without preserving the root element name
        let inner_value: Value = quick_xml::de::from_str(xml).map_err(|e| {
            ConversionError::XmlParseError(format!("Failed to deserialize XML: {}", e))
        })?;

        // Unwrap $text fields that quick-xml adds
        let cleaned_value = self.unwrap_text_fields(inner_value);

        // Wrap the value with the root element name to preserve structure
        let wrapped = serde_json::json!({
            root_name: cleaned_value
        });

        // Serialize to a compact JSON string (single line)
        serde_json::to_string(&wrapped).map_err(|e| {
            ConversionError::JsonSerializeError(format!("Failed to serialize JSON: {}", e))
        })
    }

    /// Recursively unwrap {"$text": "value"} into just "value"
    fn unwrap_text_fields(&self, value: Value) -> Value {
        match value {
            Value::Object(mut map) => {
                // If this object only has a "$text" field, return just that value
                if map.len() == 1 && map.contains_key("$text") {
                    return map.remove("$text").unwrap();
                }
                
                // Otherwise, recursively process all fields
                let cleaned: serde_json::Map<String, Value> = map
                    .into_iter()
                    .map(|(k, v)| (k, self.unwrap_text_fields(v)))
                    .collect();
                Value::Object(cleaned)
            }
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|v| self.unwrap_text_fields(v)).collect())
            }
            other => other,
        }
    }

    /// Extract the root element name from XML
    fn extract_root_element_name(&self, xml: &str) -> Result<String> {
        let trimmed = xml.trim();
        if !trimmed.starts_with('<') {
            return Err(ConversionError::XmlParseError(
                "Invalid XML: does not start with <".to_string(),
            ));
        }

        // Find the end of the opening tag name
        let start = 1; // Skip the '<'
        let end = trimmed[start..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|i| i + start)
            .unwrap_or(trimmed.len());

        Ok(trimmed[start..end].to_string())
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

        let dest = converter.calculate_move_path(
            "/source/dir/file.xml",
            "/source/dir",
            "/moved/dir",
        );

        assert_eq!(dest, "/moved/dir/file.xml");
    }

    #[test]
    fn test_xml_to_json_simple() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<trade><id>123</id><amount>100.50</amount></trade>"#;
        let json = converter.xml_to_json(xml).unwrap();

        // Should be valid JSON
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
        
        // Check structure - root element should be preserved
        assert!(parsed.get("trade").is_some());
    }

    #[test]
    fn test_xml_to_json_nested() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<trade><id>123</id><trader><name>John</name><desk>Equities</desk></trader></trade>"#;
        let json = converter.xml_to_json(xml).unwrap();

        let parsed: Value = serde_json::from_str(&json).unwrap();
        // Should be clean JSON without $text wrappers
        assert_eq!(parsed["trade"]["trader"]["name"], "John");
        assert_eq!(parsed["trade"]["id"], "123");
    }

    #[test]
    fn test_xml_to_json_with_attributes() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<trade id="123"><symbol>AAPL</symbol></trade>"#;
        let json = converter.xml_to_json(xml).unwrap();

        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_xml_to_json_invalid_xml() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<trade><unclosed>"#;
        let result = converter.xml_to_json(xml);

        assert!(result.is_err());
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

        let stats = converter.convert_xml_to_ndjson(reader, &mut output).unwrap();

        assert_eq!(stats.messages_converted, 1);
        assert!(stats.bytes_processed > 0);

        let output_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = output_str.trim().lines().collect();
        assert_eq!(lines.len(), 1);

        // Verify it's valid JSON
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

        let stats = converter.convert_xml_to_ndjson(reader, &mut output).unwrap();

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

        let stats = converter.convert_xml_to_ndjson(reader, &mut output).unwrap();

        assert_eq!(stats.messages_converted, 2);
    }

    #[test]
    fn test_convert_xml_to_ndjson_nested_elements() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<?xml version="1.0"?>
<root>
  <message>
    <trade>
      <id>1</id>
      <trader>
        <name>John</name>
        <desk>Equities</desk>
      </trader>
    </trade>
  </message>
</root>"#;

        let reader = std::io::Cursor::new(xml.as_bytes());
        let mut output = Vec::new();

        let stats = converter.convert_xml_to_ndjson(reader, &mut output).unwrap();

        assert_eq!(stats.messages_converted, 1);

        let output_str = String::from_utf8(output).unwrap();
        let parsed: Value = serde_json::from_str(output_str.trim()).unwrap();
        
        // XML has <name> tag, so field name is "n"
        assert_eq!(parsed["message"]["trade"]["trader"]["name"], "John");
    }

    #[test]
    fn test_convert_xml_to_ndjson_empty_input() {
        let converter = Converter::new(ConverterConfig::default());

        let xml = r#"<?xml version="1.0"?><root></root>"#;

        let reader = std::io::Cursor::new(xml.as_bytes());
        let mut output = Vec::new();

        let stats = converter.convert_xml_to_ndjson(reader, &mut output).unwrap();

        assert_eq!(stats.messages_converted, 0);
    }

    #[test]
    fn test_end_to_end_conversion() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        // Create source directory with test file
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

        // Convert
        let converter = Converter::new(ConverterConfig::default());
        let stats = converter
            .convert_directory(
                &storage,
                &source_dir.display().to_string(),
                &dest_dir.display().to_string(),
            )
            .unwrap();

        // Verify
        assert_eq!(stats.files_processed, 1);
        assert_eq!(stats.messages_converted, 2);
        assert_eq!(stats.files_failed, 0);

        // Check output file exists
        let output_file = dest_dir.join("test.ndjson");
        assert!(output_file.exists());

        // Verify content
        let content = std::fs::read_to_string(output_file).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        for line in lines {
            let parsed: Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    #[test]
    fn test_conversion_with_file_moves() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        // Create directories
        let source_dir = temp_dir.path().join("source");
        let dest_dir = temp_dir.path().join("dest");
        let success_dir = temp_dir.path().join("success");
        let error_dir = temp_dir.path().join("error");
        
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&dest_dir).unwrap();

        // Create valid file
        let valid_file = source_dir.join("valid.xml");
        let xml_content = r#"<?xml version="1.0"?>
<root>
  <message><trade><id>1</id></trade></message>
</root>"#;
        std::fs::write(&valid_file, xml_content).unwrap();

        // Create invalid file - malformed XML that will actually fail parsing
        let invalid_file = source_dir.join("invalid.xml");
        std::fs::write(&invalid_file, "<<not>valid>xml<").unwrap();

        // Convert with file moves
        let config = ConverterConfig {
            message_element: "message".to_string(),
            show_progress: false,
            success_dir: Some(success_dir.display().to_string()),
            error_dir: Some(error_dir.display().to_string()),
        };
        let converter = Converter::new(config);
        
        let _stats = converter.convert_directory(
            &storage,
            &source_dir.display().to_string(),
            &dest_dir.display().to_string(),
        );

        // Verify files were moved
        assert!(!valid_file.exists(), "Valid file should be moved");
        assert!(!invalid_file.exists(), "Invalid file should be moved");
        
        assert!(success_dir.join("valid.xml").exists(), "Valid file should be in success dir");
        assert!(error_dir.join("invalid.xml").exists(), "Invalid file should be in error dir");
    }

    #[test]
    fn test_conversion_stats() {
        let stats = ConversionStats {
            files_processed: 10,
            files_failed: 2,
            messages_converted: 1000,
            bytes_processed: 50000,
        };

        // Just verify print_summary doesn't panic
        stats.print_summary();
    }

    #[test]
    fn test_extract_root_element_name() {
        let converter = Converter::new(ConverterConfig::default());

        // Simple tag
        assert_eq!(
            converter.extract_root_element_name("<trade></trade>").unwrap(),
            "trade"
        );

        // Tag with attributes
        assert_eq!(
            converter.extract_root_element_name(r#"<trade id="123"></trade>"#).unwrap(),
            "trade"
        );

        // Self-closing tag
        assert_eq!(
            converter.extract_root_element_name("<trade/>").unwrap(),
            "trade"
        );

        // With whitespace
        assert_eq!(
            converter.extract_root_element_name("  <message>  ").unwrap(),
            "message"
        );
    }
}
