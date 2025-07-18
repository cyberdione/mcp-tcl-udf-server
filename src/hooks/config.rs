//! TOML configuration for hooks system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::hooks::HookType;

/// Main hooks configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// System-wide configuration
    pub system: SystemConfig,
    
    /// Individual hook handlers
    pub handlers: Vec<HandlerConfig>,
}

/// System-wide hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Whether hooks are enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Handler timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub handler_timeout_ms: u64,
    
    /// Maximum concurrent hooks
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_hooks: usize,
    
    /// Enable parallel execution
    #[serde(default = "default_true")]
    pub enable_parallel_execution: bool,
    
    /// Enable handler pooling
    #[serde(default = "default_true")]
    pub enable_handler_pooling: bool,
    
    /// Enable result caching
    #[serde(default = "default_true")]
    pub enable_result_caching: bool,
    
    /// Security configuration
    pub security: SecurityConfig,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Require signed handlers
    #[serde(default)]
    pub require_signed_handlers: bool,
    
    /// Sandbox handlers
    #[serde(default = "default_true")]
    pub sandbox_handlers: bool,
    
    /// Allowed namespaces
    #[serde(default = "default_namespaces")]
    pub allowed_namespaces: Vec<String>,
}

/// Individual handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerConfig {
    /// Handler name
    pub name: String,
    
    /// Handler type
    pub handler_type: HandlerType,
    
    /// Hook types this handler responds to
    pub hook_types: Vec<HookType>,
    
    /// Handler priority (0 = highest)
    #[serde(default = "default_priority")]
    pub priority: u16,
    
    /// Whether handler is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    
    /// Handler-specific configuration
    pub config: HandlerTypeConfig,
}

/// Handler type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandlerType {
    TclScript,
    ExternalCommand,
    BuiltIn,
}

/// Handler-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HandlerTypeConfig {
    TclScript(TclScriptConfig),
    ExternalCommand(ExternalCommandConfig),
    BuiltIn(BuiltInConfig),
}

/// TCL script handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TclScriptConfig {
    /// TCL script content
    pub script: String,
    
    /// Variables to inject
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

/// External command handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalCommandConfig {
    /// Command to execute
    pub command: String,
    
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// Timeout in milliseconds
    #[serde(default = "default_command_timeout")]
    pub timeout_ms: u64,
}

/// Built-in handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInConfig {
    /// Built-in handler name
    pub handler_name: String,
    
    /// Handler-specific configuration
    #[serde(flatten)]
    pub config: HashMap<String, serde_json::Value>,
}

impl HooksConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self {
            system: SystemConfig::default(),
            handlers: Vec::new(),
        }
    }
    
    /// Load configuration from TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
    
    /// Save configuration to TOML string
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Check for duplicate handler names
        let mut names = std::collections::HashSet::new();
        for handler in &self.handlers {
            if !names.insert(&handler.name) {
                return Err(format!("Duplicate handler name: {}", handler.name));
            }
        }
        
        // Validate handler configurations
        for handler in &self.handlers {
            handler.validate()?;
        }
        
        Ok(())
    }
    
    /// Get handlers for a specific hook type
    pub fn handlers_for_hook(&self, hook_type: HookType) -> Vec<&HandlerConfig> {
        self.handlers
            .iter()
            .filter(|h| h.enabled && h.hook_types.contains(&hook_type))
            .collect()
    }
}

impl HandlerConfig {
    /// Validate handler configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Handler name cannot be empty".to_string());
        }
        
        if self.hook_types.is_empty() {
            return Err(format!("Handler '{}' has no hook types", self.name));
        }
        
        Ok(())
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            handler_timeout_ms: 5000,
            max_concurrent_hooks: 10,
            enable_parallel_execution: true,
            enable_handler_pooling: true,
            enable_result_caching: true,
            security: SecurityConfig::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_signed_handlers: false,
            sandbox_handlers: true,
            allowed_namespaces: vec!["system".to_string(), "user".to_string(), "custom".to_string()],
        }
    }
}

// Default value functions for serde
fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    5000
}

fn default_max_concurrent() -> usize {
    10
}

fn default_namespaces() -> Vec<String> {
    vec!["system".to_string(), "user".to_string(), "custom".to_string()]
}

fn default_priority() -> u16 {
    500
}

fn default_command_timeout() -> u64 {
    2000
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_serialization() {
        let config = HooksConfig::new();
        let toml = config.to_toml().unwrap();
        let parsed = HooksConfig::from_toml(&toml).unwrap();
        
        assert_eq!(parsed.system.enabled, config.system.enabled);
    }
    
    #[test]
    fn test_handler_validation() {
        let mut handler = HandlerConfig {
            name: "test".to_string(),
            handler_type: HandlerType::TclScript,
            hook_types: vec![HookType::ServerStartup],
            priority: 100,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            config: HandlerTypeConfig::TclScript(TclScriptConfig {
                script: "proc test {ctx payload} { return [dict create type continue] }".to_string(),
                variables: HashMap::new(),
            }),
        };
        
        assert!(handler.validate().is_ok());
        
        handler.name.clear();
        assert!(handler.validate().is_err());
    }
}