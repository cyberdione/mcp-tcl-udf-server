//! Sandboxing for hook handlers

use crate::hooks::security::limits::ResourceLimits;
use std::path::PathBuf;
use std::collections::HashSet;

/// Sandbox configuration
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Resource limits
    pub resource_limits: ResourceLimits,
    
    /// Allowed file paths
    pub allowed_paths: HashSet<PathBuf>,
    
    /// Allowed network hosts
    pub allowed_hosts: HashSet<String>,
    
    /// Allowed system calls (Linux)
    pub allowed_syscalls: Option<HashSet<String>>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            resource_limits: ResourceLimits::default(),
            allowed_paths: HashSet::new(),
            allowed_hosts: HashSet::new(),
            allowed_syscalls: None,
        }
    }
}

/// Sandbox trait for different sandbox implementations
pub trait Sandbox: Send + Sync {
    /// Enter the sandbox
    fn enter(&self) -> Result<(), String>;
    
    /// Exit the sandbox
    fn exit(&self) -> Result<(), String>;
    
    /// Check if a path is allowed
    fn is_path_allowed(&self, path: &PathBuf) -> bool;
    
    /// Check if a host is allowed
    fn is_host_allowed(&self, host: &str) -> bool;
}

/// No-op sandbox (for testing)
pub struct NoOpSandbox;

impl Sandbox for NoOpSandbox {
    fn enter(&self) -> Result<(), String> {
        Ok(())
    }
    
    fn exit(&self) -> Result<(), String> {
        Ok(())
    }
    
    fn is_path_allowed(&self, _path: &PathBuf) -> bool {
        true
    }
    
    fn is_host_allowed(&self, _host: &str) -> bool {
        true
    }
}