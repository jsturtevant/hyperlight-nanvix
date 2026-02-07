//! Python package management for hyperlight-nanvix.
//!
//! Downloads pure-Python wheels from PyPI, extracts them, pre-compiles
//! bytecode, and builds FAT images that can be mounted in the Nanvix guest.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The guest mount point for Python site-packages FAT images.
pub const SITE_PACKAGES_GUEST_PATH: &str = "/.local/lib/python3.12/site-packages";

/// The guest mount point where package FATs are mounted.
pub const PACKAGE_MOUNT_POINT: &str = "/root";

/// Default package directory name inside the registry.
const PACKAGES_DIR: &str = "lib";

/// Prefix for package FAT files to distinguish them from other FATs.
const PACKAGE_FAT_PREFIX: &str = "pkg-";

/// Get the packages directory inside a registry path.
fn packages_dir(registry: &Path) -> PathBuf {
    registry.join(PACKAGES_DIR)
}

/// Get the FAT image path for a given package name inside a registry.
pub fn package_fat_path(registry: &Path, package_name: &str) -> PathBuf {
    packages_dir(registry)
        .join(format!("{}{}.fat", PACKAGE_FAT_PREFIX, normalize_name(package_name)))
}

/// The Python version used in the Nanvix guest.
const GUEST_PYTHON_VERSION: &str = "3.12";

/// Find a Python 3.12 interpreter on the host.
///
/// Tries `python3.12` first, then falls back to `python3` with a version check.
/// This ensures `.pyc` bytecode is compiled with the correct magic number for the guest.
fn find_python312() -> Result<String> {
    // Try python3.12 first
    if let Ok(output) = Command::new("python3.12")
        .args(["--version"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        if output.status.success() {
            return Ok("python3.12".to_string());
        }
    }

    // Fall back to python3, but verify the version
    let output = Command::new("python3")
        .args(["--version"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("Neither python3.12 nor python3 found on PATH")?;

    if !output.status.success() {
        bail!("python3 --version failed");
    }

    let version_str = String::from_utf8_lossy(&output.stdout);
    // Parse "Python 3.12.x" → check major.minor
    let version = version_str.trim().strip_prefix("Python ").unwrap_or("");
    if !version.starts_with(&format!("{}.", GUEST_PYTHON_VERSION)) {
        bail!(
            "Host Python is {} but Nanvix guest uses Python {}. \
             Install python{} to pre-compile bytecode correctly.\n\
             On Ubuntu: sudo apt-get install python{}",
            version.trim(),
            GUEST_PYTHON_VERSION,
            GUEST_PYTHON_VERSION,
            GUEST_PYTHON_VERSION,
        );
    }

    Ok("python3".to_string())
}

/// Normalize a PyPI package name (PEP 503): lowercase, replace [-_.] with -.
fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .replace('_', "-")
        .replace('.', "-")
}

/// Check if a package FAT already exists in the registry.
pub fn is_package_installed(registry: &Path, package_name: &str) -> bool {
    package_fat_path(registry, package_name).exists()
}

/// List all installed package FATs in the registry.
pub fn list_installed_packages(registry: &Path) -> Vec<String> {
    let pkg_dir = packages_dir(registry);
    let mut packages = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(pkg) = name
                .strip_prefix(PACKAGE_FAT_PREFIX)
                .and_then(|s| s.strip_suffix(".fat"))
            {
                packages.push(pkg.to_string());
            }
        }
    }
    packages.sort();
    packages
}

/// Build a package FAT image from a PyPI package name.
///
/// Steps:
/// 1. `pip download` the pure-Python wheel to a temp dir
/// 2. Extract the wheel (it's a zip)
/// 3. Pre-compile all `.py` files to `.pyc`
/// 4. Build a FAT image with guest path `/.local/lib/python3.12/site-packages`
/// 5. Store as `lib/pkg-<name>.fat` in the registry
pub fn build_package(registry: &Path, package_name: &str, force: bool) -> Result<PathBuf> {
    let fat_path = package_fat_path(registry, package_name);

    if fat_path.exists() && !force {
        println!("  {} already installed at {}", package_name, fat_path.display());
        return Ok(fat_path);
    }

    let staging_base = registry.join("tmp").join("pkg-staging");
    std::fs::create_dir_all(&staging_base)
        .context("Failed to create staging directory")?;

    let staging_dir = staging_base.join(normalize_name(package_name));
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir)?;
    }
    std::fs::create_dir_all(&staging_dir)?;

    let wheel_dir = staging_dir.join("wheels");
    std::fs::create_dir_all(&wheel_dir)?;

    // Step 1: Download wheel from PyPI
    // Only download pure-Python wheels (tagged py3-none-any or py2.py3-none-any).
    // Using --only-binary=:all: ensures we get wheels (not sdists), and
    // --platform=any restricts to platform-independent (pure Python) wheels.
    println!("  Downloading {} from PyPI...", package_name);
    let pip_output = Command::new("pip")
        .args([
            "download",
            "--no-deps",
            "--only-binary=:all:",
            "--platform=any",
            "--python-version=3.12",
            "--abi=none",
            "-d",
        ])
        .arg(&wheel_dir)
        .arg(package_name)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .context("Failed to run pip. Is pip installed?")?;

    if !pip_output.status.success() {
        let stderr = String::from_utf8_lossy(&pip_output.stderr);
        bail!(
            "Failed to download a pure-Python wheel for '{}'. \
             Only pure-Python packages (no C extensions) are supported.\n  pip stderr: {}",
            package_name,
            stderr.lines().last().unwrap_or(&stderr)
        );
    }

    // Find the downloaded .whl file
    let whl_file = find_wheel(&wheel_dir, package_name)?;
    let whl_name = whl_file.file_name().unwrap().to_string_lossy().to_string();
    println!("  Downloaded: {}", whl_name);

    // Validate wheel is pure Python by checking the filename tag.
    // Pure-Python wheels are tagged "py3-none-any" or "py2.py3-none-any".
    // Platform-specific wheels have tags like "cp312-cp312-linux_x86_64".
    if !whl_name.contains("-none-any") {
        bail!(
            "Package '{}' is not a pure-Python wheel (filename: {}). \
             Only pure-Python packages (no C extensions) are supported in Nanvix.",
            package_name,
            whl_name
        );
    }

    // Step 2: Extract wheel (it's a zip)
    let extract_dir = staging_dir.join("extracted");
    std::fs::create_dir_all(&extract_dir)?;

    println!("  Extracting wheel...");
    let unzip_status = Command::new("unzip")
        .args(["-q", "-o"])
        .arg(&whl_file)
        .arg("-d")
        .arg(&extract_dir)
        .status()
        .context("Failed to run unzip. Is unzip installed?")?;

    if !unzip_status.success() {
        bail!("Failed to extract wheel for '{}'", package_name);
    }

    // Validate: reject packages containing native extensions (.so, .pyd, .dll)
    let native_files = find_native_extensions(&extract_dir)?;
    if !native_files.is_empty() {
        let examples: Vec<_> = native_files.iter().take(3).map(|p| {
            p.strip_prefix(&extract_dir)
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        }).collect();
        bail!(
            "Package '{}' contains native extensions and cannot run in Nanvix:\n  {}{}",
            package_name,
            examples.join("\n  "),
            if native_files.len() > 3 {
                format!("\n  ... and {} more", native_files.len() - 3)
            } else {
                String::new()
            }
        );
    }

    // Step 3: Pre-compile .py files to .pyc
    // Use python3.12 explicitly to ensure bytecode matches the guest Python version.
    // Falling back to python3 would produce .pyc with the wrong magic number if
    // the host Python version differs from 3.12.
    let python_bin = find_python312()?;
    println!("  Pre-compiling bytecode (using {})...", python_bin);
    let _compile_status = Command::new(&python_bin)
        .args(["-m", "compileall", "-q"])
        .arg(&extract_dir)
        .status();
    // Don't fail if compileall has issues — some files may not compile

    // Step 4: Create FAT image
    println!("  Building FAT image...");
    std::fs::create_dir_all(fat_path.parent().unwrap())?;
    create_fat_image(&extract_dir, SITE_PACKAGES_GUEST_PATH, &fat_path)?;

    // Clean up staging
    let _ = std::fs::remove_dir_all(&staging_dir);

    println!("  Installed: {}", fat_path.display());
    Ok(fat_path)
}

/// Build FAT image from a source directory. Equivalent to the nanvix create-fat.sh script.
fn create_fat_image(source: &Path, guest_path: &str, output: &Path) -> Result<()> {
    // Check required tools
    for tool in &["mkfs.fat", "mcopy", "mmd"] {
        if Command::new("which")
            .arg(tool)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| !s.success())
            .unwrap_or(true)
        {
            bail!(
                "Required tool '{}' not found. Install with: sudo apt-get install dosfstools mtools",
                tool
            );
        }
    }

    // Calculate size with 30% overhead
    let content_size = dir_size(source)?;
    let mut fat_size = content_size * 130 / 100;
    let min_size: u64 = 1024 * 1024; // 1MB minimum
    if fat_size < min_size {
        fat_size = min_size;
    }
    // Round up to nearest 1MB
    fat_size = ((fat_size + 1024 * 1024 - 1) / (1024 * 1024)) * (1024 * 1024);

    // Remove existing image
    if output.exists() {
        std::fs::remove_file(output)?;
    }

    // Create sparse file
    let file = std::fs::File::create(output)?;
    file.set_len(fat_size)?;
    drop(file);

    // Format FAT image
    let fat32_min: u64 = 33 * 1024 * 1024;
    let mut mkfs_cmd = Command::new("mkfs.fat");
    if fat_size >= fat32_min {
        mkfs_cmd.args(["-F", "32"]);
    }
    let label = normalize_name(
        output
            .file_stem()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("PACKAGE"),
    )
    .to_uppercase()
    .replace('-', "")
    .chars()
    .filter(|c| c.is_alphanumeric())
    .take(11)
    .collect::<String>();

    mkfs_cmd
        .args(["-n", &label])
        .arg(output)
        .stdout(std::process::Stdio::null())
        .status()
        .context("Failed to run mkfs.fat")?;

    // Create parent directories in FAT image
    let parent = Path::new(guest_path).parent();
    if let Some(p) = parent {
        if p != Path::new("/") {
            let mut current = String::new();
            for component in p.components() {
                if let std::path::Component::Normal(c) = component {
                    current.push('/');
                    current.push_str(&c.to_string_lossy());
                    let _ = Command::new("mmd")
                        .args(["-i"])
                        .arg(output)
                        .arg(format!("::{}", current))
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }
        }
    }

    // Create the target directory
    let _ = Command::new("mmd")
        .args(["-i"])
        .arg(output)
        .arg(format!("::{}", guest_path))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // Copy directory contents recursively
    let entries: Vec<_> = std::fs::read_dir(source)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    if entries.is_empty() {
        bail!("No files found in extracted wheel at {}", source.display());
    }

    let mut mcopy_cmd = Command::new("mcopy");
    mcopy_cmd
        .args(["-i"])
        .arg(output)
        .arg("-s");

    for entry in &entries {
        mcopy_cmd.arg(entry);
    }
    mcopy_cmd.arg(format!("::{}/", guest_path));

    let mcopy_status = mcopy_cmd.status().context("Failed to run mcopy")?;
    if !mcopy_status.success() {
        bail!("mcopy failed when populating FAT image");
    }

    Ok(())
}

/// Recursively find native extension files (.so, .pyd, .dll) in a directory.
fn find_native_extensions(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut natives = Vec::new();
    find_native_extensions_recursive(dir, &mut natives)?;
    Ok(natives)
}

fn find_native_extensions_recursive(dir: &Path, results: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            find_native_extensions_recursive(&path, results)?;
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = name.to_lowercase();
            if lower.ends_with(".so")
                || lower.contains(".so.")
                || lower.ends_with(".pyd")
                || lower.ends_with(".dll")
            {
                results.push(path);
            }
        }
    }
    Ok(())
}

/// Recursively compute the total size of a directory.
fn dir_size(path: &Path) -> Result<u64> {
    let mut total: u64 = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total += dir_size(&entry.path())?;
            } else {
                total += metadata.len();
            }
        }
    } else {
        total = std::fs::metadata(path)?.len();
    }
    Ok(total)
}

/// Find a .whl file in the given directory matching the package name.
fn find_wheel(dir: &Path, package_name: &str) -> Result<PathBuf> {
    let normalized = normalize_name(package_name);
    // Wheel filenames use _ not - per PEP 427
    let normalized_underscore = normalized.replace('-', "_");

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.ends_with(".whl") {
            // Check if it matches our package (wheel names start with package_name-version)
            let name_lower = name.replace('-', "_");
            if name_lower.starts_with(&format!("{}_", normalized_underscore))
                || name_lower.starts_with(&format!("{}_", normalized.replace('-', "_")))
            {
                return Ok(entry.path());
            }
        }
    }

    // Fallback: just return the first .whl file if only one was downloaded
    let whls: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".whl")
        })
        .collect();

    if whls.len() == 1 {
        return Ok(whls[0].path());
    }

    bail!(
        "Could not find wheel for '{}' in {}. Found {} .whl files.",
        package_name,
        dir.display(),
        whls.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("Markdown"), "markdown");
        assert_eq!(normalize_name("my_package"), "my-package");
        assert_eq!(normalize_name("Some.Package"), "some-package");
        assert_eq!(normalize_name("requests"), "requests");
    }

    #[test]
    fn test_package_fat_path() {
        let registry = Path::new("/tmp/registry");
        assert_eq!(
            package_fat_path(registry, "markdown"),
            PathBuf::from("/tmp/registry/lib/pkg-markdown.fat")
        );
        assert_eq!(
            package_fat_path(registry, "My_Package"),
            PathBuf::from("/tmp/registry/lib/pkg-my-package.fat")
        );
    }
}
