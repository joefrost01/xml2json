//! XML to NDJSON Converter - Command Line Interface
//!
//! This tool converts XML files containing multiple messages into NDJSON format
//! for efficient BigQuery ingestion. It supports both local filesystems and
//! Google Cloud Storage.

use xml2json::{
    cli::Args, create_storage, Converter, ConverterConfig,
};

fn main() {
    // Parse command-line arguments
    let args = Args::parse_args();

    // Validate arguments
    if let Err(e) = args.validate() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Print configuration
    println!("XML to NDJSON Converter");
    println!("======================");
    println!("Source:      {}", args.source);
    println!("Destination: {}", args.destination);
    println!("Message element: <{}>", args.message_element);
    println!();

    // Create storage backend based on source path
    let storage = match create_storage(&args.source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize storage: {}", e);
            std::process::exit(1);
        }
    };

    // Create converter with configuration
    let config = ConverterConfig {
        message_element: args.message_element,
        show_progress: !args.no_progress,
        success_dir: args.success_dir,
        error_dir: args.error_dir,
    };
    let converter = Converter::new(config);

    // Run the conversion
    match converter.convert_directory(storage.as_ref(), &args.source, &args.destination) {
        Ok(stats) => {
            stats.print_summary();

            if stats.files_failed > 0 {
                eprintln!("\nWarning: Some files failed to convert");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Conversion failed: {}", e);
            std::process::exit(1);
        }
    }
}
