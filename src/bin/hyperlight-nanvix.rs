use anyhow::Result;
use clap::{Parser, Subcommand};
use hyperlight_nanvix::{cache, packages, RuntimeConfig, Sandbox};
use nanvix::log;
use nanvix::registry::Registry;
use std::path::PathBuf;

/// A Hyperlight VMM wrapper with out-of-the-box support for running Nanvix microkernel guests
#[derive(Parser)]
#[command(name = "hyperlight-nanvix")]
#[command(about = "Run scripts in a Nanvix microkernel guest")]
#[command(
    after_help = "Supported file types: .js, .mjs (JavaScript), .py (Python), .elf, .o (Binary)"
)]
struct Cli {
    /// Show detailed nanvix logging
    #[arg(long)]
    verbose: bool,

    /// Path to local nanvix build directory
    #[arg(long, value_name = "PATH")]
    nanvix_registry: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the script to run
    #[arg(value_name = "SCRIPT")]
    script_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Download nanvix registry and show compilation instructions
    SetupRegistry,
    /// Clear the nanvix registry cache
    ClearRegistry,
    /// Download and build FAT images for Python packages from PyPI
    BuildPackages {
        /// Package names to install (e.g. markdown requests)
        #[arg(required = true, num_args = 1..)]
        packages: Vec<String>,

        /// Force rebuild even if package FAT already exists
        #[arg(long)]
        force: bool,
    },
    /// List installed Python packages
    ListPackages,
}

/// Default log-level (overridden by RUST_LOG environment variable if set).
const DEFAULT_LOG_LEVEL: &str = "info";

async fn setup_registry_command() -> Result<()> {
    println!("Setting up Nanvix registry...");

    // Check cache status first using shared cache utilities
    let kernel_cached = cache::is_binary_cached("kernel.elf");
    let qjs_cached = cache::is_binary_cached("qjs");
    let python_cached = cache::is_binary_cached("python3");

    if kernel_cached && qjs_cached && python_cached {
        println!("Registry already set up at ~/.cache/nanvix-registry/");
    } else {
        // Trigger registry download by requesting key binaries
        let registry = Registry::new(None);

        if !kernel_cached {
            print!("Downloading kernel.elf... ");
            let _kernel = registry
                .get_cached_binary("hyperlight", "single-process", "kernel.elf")
                .await?;
            println!("done");
        } else {
            println!("kernel.elf already cached");
        }

        if !qjs_cached {
            print!("Downloading qjs binary... ");
            let _qjs = registry
                .get_cached_binary("hyperlight", "single-process", "qjs")
                .await?;
            println!("done");
        } else {
            println!("qjs already cached");
        }

        if !python_cached {
            print!("Downloading python3 binary... ");
            let _python = registry
                .get_cached_binary("hyperlight", "single-process", "python3")
                .await?;
            println!("done");
        } else {
            println!("python3 already cached");
        }

        println!("\nRegistry setup complete at ~/.cache/nanvix-registry/");
    }

    println!("\nTo compile and run C/C++ programs, see the README:");
    println!(
        "https://github.com/hyperlight-dev/hyperlight-nanvix?tab=readme-ov-file#c--c-programs"
    );

    Ok(())
}

async fn clear_registry_command() -> Result<()> {
    println!("Clearing Nanvix registry cache...");

    // Create a minimal config to instantiate the Sandbox for cache clearing
    let config = RuntimeConfig::new();
    let sandbox = Sandbox::new(config)?;

    match sandbox.clear_cache().await {
        Ok(()) => println!("Cache cleared successfully"),
        Err(e) => {
            eprintln!("Error clearing cache: {}", e);
            std::process::exit(1);
        }
    }

    println!("Run 'cargo run -- setup-registry' to re-download if needed.");
    Ok(())
}

fn resolve_registry(cli_override: &Option<String>) -> PathBuf {
    if let Some(ref path) = cli_override {
        PathBuf::from(path)
    } else {
        // Default to the same path as the nanvix registry cache
        cache::get_cache_directory()
    }
}

fn build_packages_command(
    registry_override: &Option<String>,
    package_names: &[String],
    force: bool,
) -> Result<()> {
    let registry = resolve_registry(registry_override);

    println!(
        "Building {} package(s) into registry: {}",
        package_names.len(),
        registry.display()
    );
    println!();

    let mut failures = Vec::new();

    for name in package_names {
        println!("[{}]", name);
        match packages::build_package(&registry, name, force) {
            Ok(fat_path) => {
                println!(
                    "  OK: {}",
                    fat_path.file_name().unwrap().to_string_lossy()
                );
            }
            Err(e) => {
                eprintln!("  FAILED: {}", e);
                failures.push(name.clone());
            }
        }
        println!();
    }

    if failures.is_empty() {
        println!("All packages installed successfully.");
        println!();
        println!("Use in your code:");
        println!(
            "  let config = RuntimeConfig::new()\n      .with_nanvix_registry(\"{}\")",
            registry.display()
        );
        for name in package_names {
            println!("      .with_python_package(\"{}\")", name);
        }
        println!("      ;");
    } else {
        eprintln!("Failed packages: {}", failures.join(", "));
        std::process::exit(1);
    }

    Ok(())
}

fn list_packages_command(registry_override: &Option<String>) -> Result<()> {
    let registry = resolve_registry(registry_override);
    let installed = packages::list_installed_packages(&registry);

    if installed.is_empty() {
        println!("No packages installed in {}", registry.display());
        println!("Install with: cargo run -- build-packages <package-name>");
    } else {
        println!("Installed packages in {}:", registry.display());
        for pkg in &installed {
            let fat = packages::package_fat_path(&registry, pkg);
            let size = std::fs::metadata(&fat)
                .map(|m| format!("{:.1}MB", m.len() as f64 / (1024.0 * 1024.0)))
                .unwrap_or_else(|_| "?".to_string());
            println!("  {} ({})", pkg, size);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle subcommands
    if let Some(command) = cli.command {
        return match command {
            Commands::SetupRegistry => setup_registry_command().await,
            Commands::ClearRegistry => clear_registry_command().await,
            Commands::BuildPackages { packages: pkgs, force } => {
                build_packages_command(&cli.nanvix_registry, &pkgs, force)
            }
            Commands::ListPackages => list_packages_command(&cli.nanvix_registry),
        };
    }

    // Require script path for default operation
    let script_path = cli.script_path.unwrap_or_else(|| {
        eprintln!("error: the following required arguments were not provided:\n  <SCRIPT>\n");
        eprintln!("Usage: hyperlight-nanvix [OPTIONS] <SCRIPT>");
        eprintln!("       hyperlight-nanvix setup-registry");
        eprintln!("       hyperlight-nanvix clear-registry");
        eprintln!("\nFor more information, try '--help'.");
        std::process::exit(1);
    });

    // Check if file exists
    if !script_path.exists() {
        eprintln!("Error: File {:?} does not exist", script_path);
        std::process::exit(1);
    }

    // Initialize nanvix logging only when --verbose is specified
    if cli.verbose {
        log::init(
            false,
            DEFAULT_LOG_LEVEL,
            "/tmp/hyperlight-nanvix".to_string(),
            None,
        );
    }

    // Create runtime configuration
    let mut config = RuntimeConfig::new()
        .with_log_directory("/tmp/hyperlight-nanvix")
        .with_tmp_directory("/tmp/hyperlight-nanvix");

    // Apply nanvix-registry if provided
    if let Some(registry_path) = cli.nanvix_registry {
        config = config.with_nanvix_registry(registry_path);
    }

    // Create Sandbox instance
    let mut sandbox = Sandbox::new(config)?;

    // Run the workload
    match sandbox.run(&script_path).await {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error running workload: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
