//! Resource limits for hook execution

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes
    pub max_memory: Option<u64>,
    
    /// Maximum CPU time
    pub max_cpu_time: Option<Duration>,
    
    /// Maximum file size for operations
    pub max_file_size: Option<u64>,
    
    /// Maximum number of file operations
    pub max_file_operations: Option<u32>,
    
    /// Maximum number of network calls
    pub max_network_calls: Option<u32>,
    
    /// Maximum execution time
    pub max_execution_time: Option<Duration>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory: Some(100 * 1024 * 1024), // 100MB
            max_cpu_time: Some(Duration::from_secs(5)),
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_file_operations: Some(100),
            max_network_calls: Some(10),
            max_execution_time: Some(Duration::from_secs(30)),
        }
    }
}

impl ResourceLimits {
    /// Create minimal resource limits
    pub fn minimal() -> Self {
        Self {
            max_memory: Some(10 * 1024 * 1024), // 10MB
            max_cpu_time: Some(Duration::from_secs(1)),
            max_file_size: Some(1024 * 1024), // 1MB
            max_file_operations: Some(10),
            max_network_calls: Some(0),
            max_execution_time: Some(Duration::from_secs(5)),
        }
    }
    
    /// Create relaxed resource limits
    pub fn relaxed() -> Self {
        Self {
            max_memory: Some(1024 * 1024 * 1024), // 1GB
            max_cpu_time: Some(Duration::from_secs(60)),
            max_file_size: Some(100 * 1024 * 1024), // 100MB
            max_file_operations: Some(1000),
            max_network_calls: Some(100),
            max_execution_time: Some(Duration::from_secs(300)),
        }
    }
}