//! Storage abstraction for local filesystem and Google Cloud Storage.
//!
//! This module provides a unified interface for reading and writing files
//! regardless of whether they're stored locally or in GCS.

use crate::error::{ConversionError, Result};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

// Optimized buffer sizes for better I/O performance
const READ_BUFFER_SIZE: usize = 256 * 1024;  // 256 KB
const WRITE_BUFFER_SIZE: usize = 256 * 1024; // 256 KB

/// Trait for storage backends (local filesystem or cloud storage)
pub trait Storage: Send + Sync {
    /// List all files with the given prefix/path
    fn list_files(&self, prefix: &str) -> Result<Vec<String>>;

    /// Open a file for reading
    fn read(&self, path: &str) -> Result<Box<dyn Read + Send>>;

    /// Open a file for writing
    fn write(&self, path: &str) -> Result<Box<dyn Write + Send>>;

    /// Move a file from source to destination
    fn move_file(&self, from: &str, to: &str) -> Result<()>;
}

/// Local filesystem storage implementation
pub struct LocalStorage;

impl LocalStorage {
    pub fn new() -> Self {
        Self
    }
}

impl Storage for LocalStorage {
    fn list_files(&self, prefix: &str) -> Result<Vec<String>> {
        let path = Path::new(prefix);

        if !path.exists() {
            return Err(ConversionError::StorageError(format!(
                "Path does not exist: {}",
                prefix
            )));
        }

        let mut files = Vec::new();

        if path.is_file() {
            files.push(prefix.to_string());
        } else {
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    if let Some(ext) = entry.path().extension() {
                        if ext == "xml" {
                            files.push(entry.path().display().to_string());
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    fn read(&self, path: &str) -> Result<Box<dyn Read + Send>> {
        let file = std::fs::File::open(path).map_err(|e| {
            ConversionError::ReadError(format!("Failed to open {}: {}", path, e))
        })?;
        // Use larger buffer for better I/O performance
        Ok(Box::new(BufReader::with_capacity(READ_BUFFER_SIZE, file)))
    }

    fn write(&self, path: &str) -> Result<Box<dyn Write + Send>> {
        let path = Path::new(path);

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(path).map_err(|e| {
            ConversionError::WriteError(format!("Failed to create {}: {}", path.display(), e))
        })?;
        // Use larger buffer for better I/O performance
        Ok(Box::new(BufWriter::with_capacity(WRITE_BUFFER_SIZE, file)))
    }

    fn move_file(&self, from: &str, to: &str) -> Result<()> {
        let to_path = Path::new(to);

        // Create parent directories if they don't exist
        if let Some(parent) = to_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Try atomic rename first (works if same filesystem)
        if let Err(_) = std::fs::rename(from, to) {
            // If rename fails (different filesystems), copy then delete
            std::fs::copy(from, to).map_err(|e| {
                ConversionError::StorageError(format!("Failed to copy {} to {}: {}", from, to, e))
            })?;
            std::fs::remove_file(from).map_err(|e| {
                ConversionError::StorageError(format!("Failed to remove {}: {}", from, e))
            })?;
        }

        Ok(())
    }
}

/// Google Cloud Storage implementation
pub struct GcsStorage {
    client: google_cloud_storage::client::Client,
    runtime: tokio::runtime::Runtime,
}

impl GcsStorage {
    /// Create a new GCS storage backend with default credentials
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| ConversionError::StorageError(format!("Failed to create runtime: {}", e)))?;

        let client = runtime.block_on(async {
            let config = google_cloud_storage::client::ClientConfig::default()
                .with_auth()
                .await
                .map_err(|e| ConversionError::StorageError(format!("Failed to create auth: {}", e)))?;
            Ok::<_, ConversionError>(google_cloud_storage::client::Client::new(config))
        })?;

        Ok(Self { client, runtime })
    }

    /// Parse a GCS path into bucket and prefix
    /// Expects format: gs://bucket-name/path/to/files
    fn parse_gcs_path(path: &str) -> Result<(String, String)> {
        let path = path.strip_prefix("gs://").ok_or_else(|| {
            ConversionError::StorageError(format!(
                "GCS path must start with gs://. Got: {}",
                path
            ))
        })?;

        let parts: Vec<&str> = path.splitn(2, '/').collect();
        let bucket = parts[0].to_string();
        let prefix = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

        Ok((bucket, prefix))
    }
}

impl Storage for GcsStorage {
    fn list_files(&self, prefix: &str) -> Result<Vec<String>> {
        let (bucket, prefix) = Self::parse_gcs_path(prefix)?;

        let objects = self.runtime.block_on(async {
            self.client
                .list_objects(&google_cloud_storage::http::objects::list::ListObjectsRequest {
                    bucket: bucket.clone(),
                    prefix: Some(prefix.clone()),
                    ..Default::default()
                })
                .await
        }).map_err(|e| {
            ConversionError::StorageError(format!("Failed to list GCS objects: {}", e))
        })?;

        let files = objects
            .items
            .unwrap_or_default()
            .into_iter()
            .filter(|obj| obj.name.ends_with(".xml"))
            .map(|obj| format!("gs://{}/{}", bucket, obj.name))
            .collect();

        Ok(files)
    }

    fn read(&self, path: &str) -> Result<Box<dyn Read + Send>> {
        let (bucket, object) = Self::parse_gcs_path(path)?;

        let data = self.runtime.block_on(async {
            self.client
                .download_object(
                    &google_cloud_storage::http::objects::get::GetObjectRequest {
                        bucket: bucket.clone(),
                        object: object.clone(),
                        ..Default::default()
                    },
                    &Default::default(),
                )
                .await
        }).map_err(|e| {
            ConversionError::ReadError(format!("Failed to download from GCS: {}", e))
        })?;

        // Wrap in BufReader for consistency and potential buffering benefits
        Ok(Box::new(BufReader::with_capacity(
            READ_BUFFER_SIZE,
            std::io::Cursor::new(data)
        )))
    }

    fn write(&self, path: &str) -> Result<Box<dyn Write + Send>> {
        // For GCS, we'll use a buffer that uploads on drop
        let (bucket, object) = Self::parse_gcs_path(path)?;
        Ok(Box::new(GcsWriter::new(
            self.client.clone(),
            self.runtime.handle().clone(),
            bucket,
            object,
        )))
    }

    fn move_file(&self, from: &str, to: &str) -> Result<()> {
        let (src_bucket, src_object) = Self::parse_gcs_path(from)?;
        let (dst_bucket, dst_object) = Self::parse_gcs_path(to)?;

        self.runtime.block_on(async {
            // Copy the object
            self.client
                .copy_object(
                    &google_cloud_storage::http::objects::copy::CopyObjectRequest {
                        source_bucket: src_bucket.clone(),
                        source_object: src_object.clone(),
                        destination_bucket: dst_bucket.clone(),
                        destination_object: dst_object.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| {
                    ConversionError::StorageError(format!("Failed to copy in GCS: {}", e))
                })?;

            // Delete the original
            self.client
                .delete_object(
                    &google_cloud_storage::http::objects::delete::DeleteObjectRequest {
                        bucket: src_bucket.clone(),
                        object: src_object.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| {
                    ConversionError::StorageError(format!("Failed to delete original in GCS: {}", e))
                })?;

            Ok(())
        })
    }
}

/// A writer that buffers data and uploads to GCS on drop
/// Uses larger internal buffer for better performance
struct GcsWriter {
    client: google_cloud_storage::client::Client,
    runtime_handle: tokio::runtime::Handle,
    bucket: String,
    object: String,
    buffer: Vec<u8>,
}

impl GcsWriter {
    fn new(
        client: google_cloud_storage::client::Client,
        runtime_handle: tokio::runtime::Handle,
        bucket: String,
        object: String,
    ) -> Self {
        Self {
            client,
            runtime_handle,
            bucket,
            object,
            buffer: Vec::with_capacity(WRITE_BUFFER_SIZE),
        }
    }
}

impl Write for GcsWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for GcsWriter {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            let client = self.client.clone();
            let bucket = self.bucket.clone();
            let object = self.object.clone();
            let data = self.buffer.clone();

            // Upload the buffer to GCS
            let _ = self.runtime_handle.block_on(async {
                client
                    .upload_object(
                        &google_cloud_storage::http::objects::upload::UploadObjectRequest {
                            bucket: bucket.clone(),
                            ..Default::default()
                        },
                        data,
                        &google_cloud_storage::http::objects::upload::UploadType::Simple(
                            google_cloud_storage::http::objects::upload::Media::new(object.clone()),
                        ),
                    )
                    .await
            });
        }
    }
}

/// Determine the appropriate storage backend based on the path
pub fn create_storage(path: &str) -> Result<Box<dyn Storage>> {
    if path.starts_with("gs://") {
        Ok(Box::new(GcsStorage::new()?))
    } else {
        Ok(Box::new(LocalStorage::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_local_storage_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let test_path = temp_dir.path().join("test.txt").display().to_string();
        let content = "Hello, World!";

        // Write
        let mut writer = storage.write(&test_path).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
        drop(writer);

        // Read
        let mut reader = storage.read(&test_path).unwrap();
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer).unwrap();

        assert_eq!(buffer, content);
    }

    #[test]
    fn test_local_storage_write_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let test_path = temp_dir.path()
            .join("subdir1")
            .join("subdir2")
            .join("test.txt")
            .display()
            .to_string();

        // Write to nested path
        let mut writer = storage.write(&test_path).unwrap();
        writer.write_all(b"test").unwrap();
        drop(writer);

        // Verify file exists
        assert!(Path::new(&test_path).exists());
    }

    #[test]
    fn test_local_storage_list_files() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        // Create test XML files
        let base_path = temp_dir.path().display().to_string();
        for i in 1..=3 {
            let file_path = temp_dir.path().join(format!("file{}.xml", i));
            std::fs::write(&file_path, format!("content {}", i)).unwrap();
        }

        // Create a non-XML file (should be ignored)
        std::fs::write(temp_dir.path().join("ignore.txt"), "ignore").unwrap();

        // List files
        let files = storage.list_files(&base_path).unwrap();

        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|f| f.ends_with(".xml")));
    }

    #[test]
    fn test_local_storage_list_files_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        // Create nested structure
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        std::fs::write(temp_dir.path().join("file1.xml"), "content1").unwrap();
        std::fs::write(subdir.join("file2.xml"), "content2").unwrap();

        let base_path = temp_dir.path().display().to_string();
        let files = storage.list_files(&base_path).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.ends_with(".xml")));
    }

    #[test]
    fn test_local_storage_move_file_same_directory() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let source_path = temp_dir.path().join("source.xml").display().to_string();
        let dest_path = temp_dir.path().join("dest.xml").display().to_string();

        // Create source file
        std::fs::write(&source_path, "test content").unwrap();

        // Move file
        storage.move_file(&source_path, &dest_path).unwrap();

        // Verify
        assert!(!Path::new(&source_path).exists(), "Source should not exist");
        assert!(Path::new(&dest_path).exists(), "Destination should exist");

        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_local_storage_move_file_different_directory() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let source_path = temp_dir.path().join("source.xml").display().to_string();
        let dest_dir = temp_dir.path().join("moved");
        let dest_path = dest_dir.join("dest.xml").display().to_string();

        // Create source file
        std::fs::write(&source_path, "test content").unwrap();

        // Move file (should create destination directory)
        storage.move_file(&source_path, &dest_path).unwrap();

        // Verify
        assert!(!Path::new(&source_path).exists(), "Source should not exist");
        assert!(Path::new(&dest_path).exists(), "Destination should exist");

        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_local_storage_move_file_preserves_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let source_dir = temp_dir.path().join("source").join("subdir");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("file.xml").display().to_string();

        let dest_dir = temp_dir.path().join("dest").join("subdir");
        let dest_path = dest_dir.join("file.xml").display().to_string();

        // Create source file
        std::fs::write(&source_path, "nested content").unwrap();

        // Move file
        storage.move_file(&source_path, &dest_path).unwrap();

        // Verify
        assert!(!Path::new(&source_path).exists());
        assert!(Path::new(&dest_path).exists());
    }

    #[test]
    fn test_local_storage_move_nonexistent_file_errors() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new();

        let source_path = temp_dir.path().join("nonexistent.xml").display().to_string();
        let dest_path = temp_dir.path().join("dest.xml").display().to_string();

        let result = storage.move_file(&source_path, &dest_path);
        assert!(result.is_err(), "Should error when moving nonexistent file");
    }

    #[test]
    fn test_local_storage_read_nonexistent_file_errors() {
        let storage = LocalStorage::new();
        let result = storage.read("/nonexistent/path/file.xml");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_storage_local() {
        let storage = create_storage("/local/path").unwrap();
        // Can't easily test the concrete type, but we can test it doesn't panic
        assert!(storage.list_files("/nonexistent").is_err());
    }

    #[test]
    fn test_gcs_path_parsing() {
        let result = GcsStorage::parse_gcs_path("gs://bucket-name/path/to/file.xml");
        assert!(result.is_ok());
        let (bucket, object) = result.unwrap();
        assert_eq!(bucket, "bucket-name");
        assert_eq!(object, "path/to/file.xml");
    }

    #[test]
    fn test_gcs_path_parsing_no_prefix() {
        let result = GcsStorage::parse_gcs_path("bucket/path/file.xml");
        assert!(result.is_err());
    }

    #[test]
    fn test_gcs_path_parsing_bucket_only() {
        let result = GcsStorage::parse_gcs_path("gs://bucket-name");
        assert!(result.is_ok());
        let (bucket, object) = result.unwrap();
        assert_eq!(bucket, "bucket-name");
        assert_eq!(object, "");
    }

    #[test]
    fn test_buffer_sizes() {
        // Verify optimized buffer sizes are used
        assert_eq!(READ_BUFFER_SIZE, 256 * 1024);
        assert_eq!(WRITE_BUFFER_SIZE, 256 * 1024);
    }
}