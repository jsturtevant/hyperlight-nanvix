#![allow(non_local_definitions)]

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::Arc;

use crate::runtime::{Runtime, RuntimeConfig};

/// Python wrapper for hyperlight-nanvix Runtime
#[pyclass]
pub struct NanvixSandbox {
    runtime: Arc<Runtime>,
}

/// Configuration options for creating a sandbox
#[pyclass]
#[derive(Clone)]
pub struct SandboxConfig {
    #[pyo3(get, set)]
    pub log_directory: Option<String>,
    #[pyo3(get, set)]
    pub tmp_directory: Option<String>,
    #[pyo3(get, set)]
    /// Path to local nanvix directory (overrides registry downloads)
    pub nanvix_registry: Option<String>,
    #[pyo3(get, set)]
    /// File mounts: list of (host_path, guest_path) tuples
    /// Nanvix copies these files into the FAT filesystem automatically
    pub file_mounts: Option<Vec<(String, String)>>,
}

#[pymethods]
impl SandboxConfig {
    #[new]
    #[pyo3(signature = (log_directory=None, tmp_directory=None, nanvix_registry=None, file_mounts=None))]
    fn new(
        log_directory: Option<String>,
        tmp_directory: Option<String>,
        nanvix_registry: Option<String>,
        file_mounts: Option<Vec<(String, String)>>,
    ) -> Self {
        Self {
            log_directory,
            tmp_directory,
            nanvix_registry,
            file_mounts,
        }
    }
}

/// Workload execution result
#[pyclass]
#[derive(Clone)]
pub struct WorkloadResult {
    #[pyo3(get)]
    pub success: bool,
    #[pyo3(get)]
    pub stdout: String,
    #[pyo3(get)]
    pub stderr: String,
    #[pyo3(get)]
    pub error: Option<String>,
}

#[pymethods]
impl WorkloadResult {
    fn __repr__(&self) -> String {
        match &self.error {
            Some(err) => format!("WorkloadResult(success={}, error='{}')", self.success, err),
            None => format!("WorkloadResult(success={}, stdout_len={})", self.success, self.stdout.len()),
        }
    }
}

#[pymethods]
impl NanvixSandbox {
    /// Create a new sandbox instance
    ///
    /// Args:
    ///     config: Optional SandboxConfig with log_directory and tmp_directory
    ///
    /// Returns:
    ///     A new NanvixSandbox instance
    ///
    /// Example:
    ///     >>> sandbox = NanvixSandbox()
    ///     >>> config = SandboxConfig(log_directory="/tmp/logs")
    ///     >>> sandbox = NanvixSandbox(config)
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<SandboxConfig>) -> PyResult<Self> {
        let runtime_config = match config {
            Some(cfg) => {
                let mut runtime_config = RuntimeConfig::new();
                if let Some(log_dir) = cfg.log_directory {
                    runtime_config = runtime_config.with_log_directory(log_dir);
                }
                if let Some(tmp_dir) = cfg.tmp_directory {
                    runtime_config = runtime_config.with_tmp_directory(tmp_dir);
                }
                // Apply nanvix registry
                if let Some(registry) = cfg.nanvix_registry {
                    runtime_config = runtime_config.with_nanvix_registry(registry);
                }
                // Apply file mounts
                if let Some(mounts) = cfg.file_mounts {
                    runtime_config = runtime_config.with_file_mounts(mounts);
                }
                runtime_config
            }
            None => RuntimeConfig::new(),
        };

        let runtime = Runtime::new(runtime_config)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;

        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Run a workload in the sandbox
    ///
    /// Args:
    ///     workload_path: Path to the workload file (JavaScript, Python, or binary)
    ///
    /// Returns:
    ///     WorkloadResult indicating success or failure
    ///
    /// Example:
    ///     >>> result = await sandbox.run("script.py")
    ///     >>> if result.success:
    ///     ...     print("Success!")
    fn run<'py>(&self, py: Python<'py>, workload_path: String) -> PyResult<&'py PyAny> {
        let runtime = Arc::clone(&self.runtime);

        pyo3_asyncio::tokio::future_into_py(py, async move {
            match runtime.run(&workload_path).await {
                Ok(()) => Ok(WorkloadResult {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: None,
                }),
                Err(e) => Ok(WorkloadResult {
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(format!("Workload execution failed: {}", e)),
                }),
            }
        })
    }

    /// Clear the binary cache
    ///
    /// Returns:
    ///     True if cache was cleared successfully
    ///
    /// Example:
    ///     >>> success = await sandbox.clear_cache()
    fn clear_cache<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let runtime = Arc::clone(&self.runtime);

        pyo3_asyncio::tokio::future_into_py(py, async move {
            runtime
                .clear_cache()
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to clear cache: {}", e)))?;
            Ok(true)
        })
    }

    fn __repr__(&self) -> String {
        "NanvixSandbox()".to_string()
    }
}

/// Initialize the Python module
#[pymodule]
fn hyperlight_nanvix(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<NanvixSandbox>()?;
    m.add_class::<SandboxConfig>()?;
    m.add_class::<WorkloadResult>()?;
    Ok(())
}
