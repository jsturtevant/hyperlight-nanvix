//! CSV Processor Example
//!
//! This example demonstrates how to:
//! 1. Mount a file from the host into the guest VM
//! 2. Run a Python script that processes the file
//!
//! NOTE: This example currently requires a Nanvix fix to work.
//! See doc/fix-prebuilt-fat-with-mounts.md for details.
//! When using pre-built FAT images (for Python stdlib), file mounts
//! and ramfs are not currently supported.
//!
//! Run with: cargo run --example csv_processor

use anyhow::Result;
use hyperlight_nanvix::{RuntimeConfig, Sandbox};
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup: Create sample CSV in a temp directory
    let temp_dir = std::env::temp_dir().join("hyperlight-csv-example");
    fs::create_dir_all(&temp_dir)?;
    let input_csv = temp_dir.join("input.csv");

    let csv_content = "name,age,city\n\
                       Alice,30,NYC\n\
                       Bob,25,LA\n\
                       Charlie,35,Chicago\n";
    fs::write(&input_csv, csv_content)?;

    println!("Created sample CSV at: {}", input_csv.display());

    // Get nanvix registry path from environment or use default (dist/ folder)
    let nanvix_registry = std::env::var("NANVIX_REGISTRY").unwrap_or_else(|_| {
        // Default to dist/ folder relative to the project root
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        format!("{}/dist", manifest_dir)
    });

    // Configure runtime with file mounts
    // Nanvix copies files to FAT automatically
    let config = RuntimeConfig::new()
        .with_log_directory("/tmp/hyperlight-nanvix")
        .with_tmp_directory("/tmp/hyperlight-nanvix")
        .with_nanvix_registry(&nanvix_registry)
        .with_file_mount(
            input_csv.to_string_lossy().to_string(),
            "/tmp/test.csv".to_string(), // Guest path
        );

    let mut sandbox = Sandbox::new(config)?;

    // Run Python CSV processor
    println!("Running CSV processor...");
    sandbox.run("guest-examples/csv_processor.py").await?;

    // Output is printed directly to stdout by the guest
    println!("Processing complete!");

    // Cleanup temp file
    let _ = fs::remove_dir_all(&temp_dir);

    Ok(())
}
