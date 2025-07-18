//! Core types for the hooks system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use chrono::{DateTime, Utc};

/// Defines the various hook types available in the system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    // Server lifecycle hooks
    ServerStartup,
    ServerShutdown,
    ServerInitialized,
    
    // Request processing hooks
    RequestReceived,
    RequestProcessed,
    ResponseSent,
    
    // Tool execution hooks
    ToolPreExecution,
    ToolPostExecution,
    ToolRegistered,
    ToolRemoved,
    
    // TCL execution hooks
    TclPreExecution,
    TclPostExecution,
    TclError,
    
    // MCP server hooks
    McpServerConnected,
    McpServerDisconnected,
    McpServerError,
    
    // Security hooks
    SecurityCheck,
    AccessDenied,
    
    // Custom hooks (for extensions)
    Custom(String),
}

impl HookType {
    /// Get all built-in hook types
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::ServerStartup,
            Self::ServerShutdown,
            Self::ServerInitialized,
            Self::RequestReceived,
            Self::RequestProcessed,
            Self::ResponseSent,
            Self::ToolPreExecution,
            Self::ToolPostExecution,
            Self::ToolRegistered,
            Self::ToolRemoved,
            Self::TclPreExecution,
            Self::TclPostExecution,
            Self::TclError,
            Self::McpServerConnected,
            Self::McpServerDisconnected,
            Self::McpServerError,
            Self::SecurityCheck,
            Self::AccessDenied,
        ]
    }
    
    /// Get a human-readable description of the hook type
    pub fn description(&self) -> &str {
        match self {
            Self::ServerStartup => "Server initialization",
            Self::ServerShutdown => "Server cleanup",
            Self::ServerInitialized => "After all components loaded",
            Self::RequestReceived => "Before processing any request",
            Self::RequestProcessed => "After successful processing",
            Self::ResponseSent => "After response sent to client",
            Self::ToolPreExecution => "Before tool execution",
            Self::ToolPostExecution => "After tool execution",
            Self::ToolRegistered => "When new tool added",
            Self::ToolRemoved => "When tool removed",
            Self::TclPreExecution => "Before TCL script execution",
            Self::TclPostExecution => "After TCL script execution",
            Self::TclError => "On TCL execution error",
            Self::McpServerConnected => "MCP server connection established",
            Self::McpServerDisconnected => "MCP server disconnected",
            Self::McpServerError => "MCP server connection error",
            Self::SecurityCheck => "Security validation request",
            Self::AccessDenied => "Access denied event",
            Self::Custom(name) => name,
        }
    }
}

impl fmt::Display for HookType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom(name) => write!(f, "custom:{}", name),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Hook execution priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HookPriority(pub u16);

impl HookPriority {
    pub const HIGHEST: Self = Self(0);
    pub const HIGH: Self = Self(100);
    pub const NORMAL: Self = Self(500);
    pub const LOW: Self = Self(900);
    pub const LOWEST: Self = Self(1000);
}

impl Default for HookPriority {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Payload data passed to hook handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    /// Hook type being executed
    pub hook_type: HookType,
    
    /// Timestamp when the hook was triggered
    pub timestamp: DateTime<Utc>,
    
    /// Unique identifier for this hook execution
    pub execution_id: String,
    
    /// Hook-specific data
    pub data: serde_json::Value,
    
    /// Optional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl HookPayload {
    /// Create a new hook payload
    pub fn new(hook_type: HookType, data: serde_json::Value) -> Self {
        Self {
            hook_type,
            timestamp: Utc::now(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            data,
            metadata: HashMap::new(),
        }
    }
    
    /// Add metadata to the payload
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Get data as a specific type
    pub fn get_data<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.data.clone())
    }
}

/// Result of hook execution that controls flow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ExecutionResult {
    /// Continue normal execution
    Continue,
    
    /// Stop execution and return early
    Stop(Option<serde_json::Value>),
    
    /// Replace the current data with new data
    Replace(serde_json::Value),
    
    /// Retry the operation
    Retry {
        delay: Option<Duration>,
        max_attempts: Option<u32>,
    },
    
    /// Error occurred during hook execution
    Error {
        message: String,
        details: Option<serde_json::Value>,
    },
}

impl ExecutionResult {
    /// Create a continue result
    pub fn continue_execution() -> Self {
        Self::Continue
    }
    
    /// Create a stop result
    pub fn stop_execution() -> Self {
        Self::Stop(None)
    }
    
    /// Create a stop result with data
    pub fn stop_with_data(data: serde_json::Value) -> Self {
        Self::Stop(Some(data))
    }
    
    /// Create a replace result
    pub fn replace_data(data: serde_json::Value) -> Self {
        Self::Replace(data)
    }
    
    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
            details: None,
        }
    }
    
    /// Create an error result with details
    pub fn error_with_details(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self::Error {
            message: message.into(),
            details: Some(details),
        }
    }
}

/// Configuration for hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Maximum execution time
    pub timeout: Option<Duration>,
    
    /// Whether to run this hook asynchronously
    pub async_execution: bool,
    
    /// Rate limiting configuration
    pub rate_limit: Option<RateLimit>,
    
    /// Conditional execution
    pub condition: Option<String>,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(5)),
            async_execution: false,
            rate_limit: None,
            condition: None,
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// Maximum number of executions
    pub max_calls: u32,
    
    /// Time window for rate limiting
    pub window: Duration,
}

/// Hook execution statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookStats {
    /// Total number of executions
    pub total_executions: u64,
    
    /// Number of successful executions
    pub successful_executions: u64,
    
    /// Number of failed executions
    pub failed_executions: u64,
    
    /// Average execution time
    pub average_duration: Option<Duration>,
    
    /// Maximum execution time
    pub max_duration: Option<Duration>,
    
    /// Last execution time
    pub last_execution: Option<DateTime<Utc>>,
}

impl HookStats {
    /// Record a successful execution
    pub fn record_success(&mut self, duration: Duration) {
        self.total_executions += 1;
        self.successful_executions += 1;
        self.last_execution = Some(Utc::now());
        self.update_duration_stats(duration);
    }
    
    /// Record a failed execution
    pub fn record_failure(&mut self, duration: Duration) {
        self.total_executions += 1;
        self.failed_executions += 1;
        self.last_execution = Some(Utc::now());
        self.update_duration_stats(duration);
    }
    
    fn update_duration_stats(&mut self, duration: Duration) {
        // Update average duration
        if let Some(avg) = self.average_duration {
            let total_nanos = avg.as_nanos() * (self.total_executions - 1) as u128;
            let new_total = total_nanos + duration.as_nanos();
            self.average_duration = Some(Duration::from_nanos(
                (new_total / self.total_executions as u128) as u64
            ));
        } else {
            self.average_duration = Some(duration);
        }
        
        // Update max duration
        if let Some(max) = self.max_duration {
            if duration > max {
                self.max_duration = Some(duration);
            }
        } else {
            self.max_duration = Some(duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hook_type_display() {
        assert_eq!(HookType::ServerStartup.to_string(), "ServerStartup");
        assert_eq!(HookType::Custom("test".to_string()).to_string(), "custom:test");
    }
    
    #[test]
    fn test_hook_priority_ordering() {
        assert!(HookPriority::HIGHEST < HookPriority::HIGH);
        assert!(HookPriority::HIGH < HookPriority::NORMAL);
        assert!(HookPriority::NORMAL < HookPriority::LOW);
        assert!(HookPriority::LOW < HookPriority::LOWEST);
    }
    
    #[test]
    fn test_hook_payload_creation() {
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            serde_json::json!({"tool": "test_tool"})
        );
        
        assert_eq!(payload.hook_type, HookType::ToolPreExecution);
        assert!(!payload.execution_id.is_empty());
        assert!(payload.metadata.is_empty());
    }
    
    #[test]
    fn test_execution_result_helpers() {
        let cont = ExecutionResult::continue_execution();
        matches!(cont, ExecutionResult::Continue);
        
        let stop = ExecutionResult::stop_execution();
        matches!(stop, ExecutionResult::Stop(None));
        
        let error = ExecutionResult::error("test error");
        if let ExecutionResult::Error { message, .. } = error {
            assert_eq!(message, "test error");
        } else {
            panic!("Expected error result");
        }
    }
    
    #[test]
    fn test_hook_stats() {
        let mut stats = HookStats::default();
        
        stats.record_success(Duration::from_millis(100));
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 0);
        assert_eq!(stats.average_duration, Some(Duration::from_millis(100)));
        
        stats.record_failure(Duration::from_millis(200));
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 1);
        assert_eq!(stats.average_duration, Some(Duration::from_millis(150)));
        assert_eq!(stats.max_duration, Some(Duration::from_millis(200)));
    }
}