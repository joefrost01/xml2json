//! Command-line interface definitions and argument parsing.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "xml-to-ndjson",
    about = "Convert XML files to NDJSON format for BigQuery ingestion",
    version
)]
pub struct Args {
    /// Source directory or GCS bucket path (e.g., /path/to/xml or gs://bucket/path)
    #[arg(short, long)]
    pub source: String,

    /// Destination directory or GCS bucket path (e.g., /path/to/json or gs://bucket/path)
    #[arg(short, long)]
    pub destination: String,

    /// XML element name that wraps each message (default: "message")
    #[arg(short, long, default_value = "message")]
    pub message_element: String,

    /// Directory to move successfully processed files to (optional)
    #[arg(long)]
    pub success_dir: Option<String>,

    /// Directory to move failed files to (optional)
    #[arg(long)]
    pub error_dir: Option<String>,

    /// Disable progress bar
    #[arg(long)]
    pub no_progress: bool,
}

impl Args {
    /// Parse command-line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Validate the arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.source.is_empty() {
            return Err("Source path cannot be empty".to_string());
        }

        if self.destination.is_empty() {
            return Err("Destination path cannot be empty".to_string());
        }

        if self.message_element.is_empty() {
            return Err("Message element name cannot be empty".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_empty_source() {
        let args = Args {
            source: String::new(),
            destination: "/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: None,
            error_dir: None,
            no_progress: false,
        };

        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validation_empty_destination() {
        let args = Args {
            source: "/source".to_string(),
            destination: String::new(),
            message_element: "message".to_string(),
            success_dir: None,
            error_dir: None,
            no_progress: false,
        };

        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validation_empty_message_element() {
        let args = Args {
            source: "/source".to_string(),
            destination: "/dest".to_string(),
            message_element: String::new(),
            success_dir: None,
            error_dir: None,
            no_progress: false,
        };

        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validation_valid() {
        let args = Args {
            source: "/source".to_string(),
            destination: "/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: None,
            error_dir: None,
            no_progress: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validation_with_success_dir() {
        let args = Args {
            source: "/source".to_string(),
            destination: "/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: Some("/success".to_string()),
            error_dir: None,
            no_progress: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validation_with_error_dir() {
        let args = Args {
            source: "/source".to_string(),
            destination: "/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: None,
            error_dir: Some("/error".to_string()),
            no_progress: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validation_with_both_dirs() {
        let args = Args {
            source: "/source".to_string(),
            destination: "/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: Some("/success".to_string()),
            error_dir: Some("/error".to_string()),
            no_progress: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validation_gcs_paths() {
        let args = Args {
            source: "gs://bucket/source".to_string(),
            destination: "gs://bucket/dest".to_string(),
            message_element: "message".to_string(),
            success_dir: Some("gs://bucket/success".to_string()),
            error_dir: Some("gs://bucket/error".to_string()),
            no_progress: false,
        };

        assert!(args.validate().is_ok());
    }
}
