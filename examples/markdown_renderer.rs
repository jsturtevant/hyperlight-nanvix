//! Markdown Renderer Example
//!
//! This example demonstrates how to:
//! 1. Mount the Python stdlib via a FAT image
//! 2. Mount a third-party Python package (markdown) via `with_python_package`
//! 3. Run a Python script that converts Markdown to HTML inside a Nanvix VM
//!
//! Prerequisites:
//!   - `dist/lib/python3.12.fat`  — Python stdlib FAT (created by `dist/copy-nanvix.sh`)
//!   - Install the markdown package:
//!     ```
//!     cargo run -- build-packages markdown
//!     ```
//!
//! Run with: cargo run --example markdown_renderer

use anyhow::Result;
use hyperlight_nanvix::{RuntimeConfig, Sandbox};

#[tokio::main]
async fn main() -> Result<()> {
    // Get nanvix registry path from environment or use default (dist/ folder)
    let nanvix_registry = std::env::var("NANVIX_REGISTRY").unwrap_or_else(|_| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        format!("{}/dist", manifest_dir)
    });

    // Configure runtime — just name the package, no paths needed
    let config = RuntimeConfig::new()
        .with_log_directory("/tmp/hyperlight-nanvix-markdown")
        .with_tmp_directory("/tmp/hyperlight-nanvix-markdown")
        .with_nanvix_registry(&nanvix_registry)
        .with_python_package("markdown");

    let mut sandbox = Sandbox::new(config)?;

    // Run the markdown renderer guest script
    println!("Running markdown renderer in Nanvix VM...");
    sandbox.run("guest-examples/hello-markdown.py").await?;

    println!("Done!");
    Ok(())
}
