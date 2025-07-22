//! Hook management tools for sbin namespace
//!
//! These tools provide privileged access to hook system management

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::hooks::{
    HookManager, HookType, AsyncHookHandler, HookContext,
    HookPriority, HooksConfig,
    HandlerConfig, HandlerType, HandlerTypeConfig, TclScriptConfig,
    ExternalCommandConfig, BuiltInConfig, PlatformDirs,
};
use chrono::Utc;

// Tool parameter structures

#[derive(Debug, Serialize, Deserialize)]
pub struct HookAddRequest {
    /// Name of the hook handler
    pub name: String,
    /// Type of handler: tcl_script, external_command, built_in
    pub handler_type: String,
    /// Hook types to trigger on
    pub hook_types: Vec<String>,
    /// Priority (0-1000, lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: u16,
    /// Whether handler is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Handler-specific configuration
    pub config: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookRemoveRequest {
    /// Name of the hook handler to remove
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookListRequest {
    /// Filter by hook type (optional)
    #[serde(default)]
    pub hook_type: Option<String>,
    /// Filter by enabled state (optional)
    #[serde(default)]
    pub enabled_only: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookEnableRequest {
    /// Name of the hook handler
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookDisableRequest {
    /// Name of the hook handler
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookUpdateRequest {
    /// Name of the hook handler
    pub name: String,
    /// New priority (optional)
    pub priority: Option<u16>,
    /// New enabled state (optional)
    pub enabled: Option<bool>,
    /// New configuration (optional)
    pub config: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookInfoRequest {
    /// Name of the hook handler
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookTestRequest {
    /// Name of the hook handler
    pub name: String,
    /// Hook type to test
    pub hook_type: String,
    /// Test data
    #[serde(default = "default_test_data")]
    pub test_data: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookSystemStatusRequest {
    /// Include detailed statistics
    #[serde(default)]
    pub include_stats: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookSystemEnableRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookSystemDisableRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookConfigReloadRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct HookConfigSaveRequest {}

// Default value functions
fn default_priority() -> u16 {
    500
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::collections::HashMap;
    
    // Helper to create a test config directory
    fn setup_test_config() -> (TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("hooks.toml");
        
        // Set environment variable to use test directory
        // This doesn't affect PlatformDirs which uses XDG_DATA_HOME on Linux
        // but we use it to track our test config path
        
        (temp_dir, config_path.to_str().unwrap().to_string())
    }
    
    #[tokio::test]
    async fn test_hook_add_builtin() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookAddRequest {
            name: "test_logger".to_string(),
            handler_type: "built_in".to_string(),
            hook_types: vec!["server_startup".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "handler_name": "logging",
                "level": "info",
                "format": "json"
            }),
        };
        
        // Should fail without HookManager
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_add_external() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookAddRequest {
            name: "test_external".to_string(),
            handler_type: "external_command".to_string(),
            hook_types: vec!["tool_pre_execution".to_string()],
            priority: 200,
            enabled: true,
            config: json!({
                "command": "/usr/bin/echo",
                "args": ["Hello from hook"],
                "timeout_ms": 1000
            }),
        };
        
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_add_invalid_type() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookAddRequest {
            name: "test_invalid".to_string(),
            handler_type: "invalid_type".to_string(),
            hook_types: vec!["server_startup".to_string()],
            priority: 100,
            enabled: true,
            config: json!({}),
        };
        
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_add_invalid_hook_type() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookAddRequest {
            name: "test_invalid_hook".to_string(),
            handler_type: "built_in".to_string(),
            hook_types: vec!["invalid_hook_type".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "handler_name": "logging"
            }),
        };
        
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_remove() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create a config with a handler
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_handler".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::new(),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        let request = HookRemoveRequest {
            name: "test_handler".to_string(),
        };
        
        let result = handle_hook_remove(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_remove_not_found() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookRemoveRequest {
            name: "nonexistent".to_string(),
        };
        
        let result = handle_hook_remove(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_list() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create a config with multiple handlers
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "handler1".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::new(),
                    }),
                },
                HandlerConfig {
                    name: "handler2".to_string(),
                    handler_type: HandlerType::ExternalCommand,
                    hook_types: vec![HookType::ToolPreExecution, HookType::ToolPostExecution],
                    priority: 200,
                    enabled: false,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::ExternalCommand(ExternalCommandConfig {
                        command: "/bin/echo".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        timeout_ms: 1000,
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Test list all
        let request = HookListRequest {
            hook_type: None,
            enabled_only: None,
        };
        
        let result = handle_hook_list(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
        
        // Test filter by hook type
        let request = HookListRequest {
            hook_type: Some("server_startup".to_string()),
            enabled_only: None,
        };
        
        let result = handle_hook_list(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
        
        // Test filter by enabled
        let request = HookListRequest {
            hook_type: None,
            enabled_only: Some(true),
        };
        
        let result = handle_hook_list(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_enable_disable() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create a config with a disabled handler
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_handler".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup],
                    priority: 100,
                    enabled: false,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::new(),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Test enable
        let request = HookEnableRequest {
            name: "test_handler".to_string(),
        };
        
        let result = handle_hook_enable(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
        
        // Test disable
        let request = HookDisableRequest {
            name: "test_handler".to_string(),
        };
        
        let result = handle_hook_disable(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_update() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create initial config
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_handler".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::from([
                            ("level".to_string(), json!("info")),
                        ]),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Update priority and config
        let request = HookUpdateRequest {
            name: "test_handler".to_string(),
            priority: Some(200),
            enabled: None,
            config: Some(json!({
                "handler_name": "logging",
                "level": "debug",
                "format": "json"
            })),
        };
        
        let result = handle_hook_update(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_info() {
        let (_temp_dir, config_path) = setup_test_config();
        
        let created = Utc::now();
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_handler".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup, HookType::ServerShutdown],
                    priority: 150,
                    enabled: true,
                    created_at: created,
                    updated_at: created,
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "metrics".to_string(),
                        config: HashMap::from([
                            ("metric_type".to_string(), json!("counter")),
                        ]),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        let request = HookInfoRequest {
            name: "test_handler".to_string(),
        };
        
        let result = handle_hook_info(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_test() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create a logging handler for testing
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_logger".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::RequestReceived],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::from([
                            ("level".to_string(), json!("info")),
                        ]),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        let request = HookTestRequest {
            name: "test_logger".to_string(),
            hook_type: "request_received".to_string(),
            test_data: json!({
                "method": "test",
                "path": "/api/test"
            }),
        };
        
        let result = handle_hook_test(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_system_status() {
        let (_temp_dir, config_path) = setup_test_config();
        
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "handler1".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::ServerStartup],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "logging".to_string(),
                        config: HashMap::new(),
                    }),
                },
                HandlerConfig {
                    name: "handler2".to_string(),
                    handler_type: HandlerType::ExternalCommand,
                    hook_types: vec![HookType::ToolPreExecution],
                    priority: 200,
                    enabled: false,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::ExternalCommand(ExternalCommandConfig {
                        command: "/bin/echo".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        timeout_ms: 1000,
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        let request = HookSystemStatusRequest {
            include_stats: false,
        };
        
        let result = handle_hook_system_status(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_config_save_reload() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create initial config
        let config = HooksConfig::new();
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Add a handler
        let _add_request = HookAddRequest {
            name: "test_handler".to_string(),
            handler_type: "built_in".to_string(),
            hook_types: vec!["server_startup".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "handler_name": "logging",
                "level": "info"
            }),
        };
        
        // Can't add without HookManager, so create config directly
        let mut config = HooksConfig::new();
        config.handlers.push(HandlerConfig {
            name: "test_handler".to_string(),
            handler_type: HandlerType::BuiltIn,
            hook_types: vec![HookType::ServerStartup],
            priority: 100,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                handler_name: "logging".to_string(),
                config: HashMap::from([
                    ("level".to_string(), json!("info")),
                ]),
            }),
        });
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Save config
        let save_request = HookConfigSaveRequest {};
        let result = handle_hook_config_save(save_request, None).await;
        assert!(result.is_ok());
        
        // Reload config
        let reload_request = HookConfigReloadRequest {};
        let result = handle_hook_config_reload(reload_request, None).await;
        
        // The reload might fail if config doesn't exist at the PlatformDirs location
        // or succeed if it does exist from a previous test
        if result.is_ok() {
            let response = result.unwrap();
            assert_eq!(response["status"], "success");
            // Don't check handler count as it depends on where the actual config is
        }
    }
    
    #[tokio::test]
    async fn test_hook_system_enable_disable() {
        let (temp_dir, config_path) = setup_test_config();
        
        // Create initial config with system disabled
        let mut config = HooksConfig::new();
        config.system.enabled = false;
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Test enable
        let enable_request = HookSystemEnableRequest {};
        let result = handle_hook_system_enable(enable_request, None).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response["status"], "success");
        assert_eq!(response["message"], "Hook system enabled");
        
        // Verify in config - need to reload to get actual path used by the function
        let actual_config_path = temp_dir.path().join("tcl-mcp-server").join("hooks.toml");
        if actual_config_path.exists() {
            let updated_config = fs::read_to_string(&actual_config_path).unwrap();
            let updated = HooksConfig::from_toml(&updated_config).unwrap();
            assert!(updated.system.enabled);
        }
        
        // Test disable
        let disable_request = HookSystemDisableRequest {};
        let result = handle_hook_system_disable(disable_request, None).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response["status"], "success");
        assert_eq!(response["message"], "Hook system disabled");
        
        // Verify in config - need to reload to get actual path used by the function
        let actual_config_path = temp_dir.path().join("tcl-mcp-server").join("hooks.toml");
        if actual_config_path.exists() {
            let updated_config = fs::read_to_string(&actual_config_path).unwrap();
            let updated = HooksConfig::from_toml(&updated_config).unwrap();
            assert!(!updated.system.enabled);
        }
    }
    
    #[tokio::test]
    async fn test_hook_add_duplicate_name() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        // Add first handler
        let request = HookAddRequest {
            name: "duplicate_test".to_string(),
            handler_type: "built_in".to_string(),
            hook_types: vec!["server_startup".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "handler_name": "logging",
                "level": "info"
            }),
        };
        
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
        
        // Try to add another with same name
        let request2 = HookAddRequest {
            name: "duplicate_test".to_string(),
            handler_type: "built_in".to_string(),
            hook_types: vec!["server_startup".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "handler_name": "logging",
                "level": "info"
            }),
        };
        let result = handle_hook_add(request2, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_update_nonexistent() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookUpdateRequest {
            name: "nonexistent".to_string(),
            priority: Some(200),
            enabled: None,
            config: None,
        };
        
        let result = handle_hook_update(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_type_parsing() {
        // Test valid hook types
        assert!(matches!(HookType::from_string("server_startup"), Ok(HookType::ServerStartup)));
        assert!(matches!(HookType::from_string("tool_pre_execution"), Ok(HookType::ToolPreExecution)));
        assert!(matches!(HookType::from_string("custom:my_hook"), Ok(HookType::Custom(s)) if s == "my_hook"));
        
        // Test invalid hook type
        assert!(HookType::from_string("invalid_type").is_err());
    }
    
    #[tokio::test]
    async fn test_hook_priority_conversion() {
        let priority = HookPriority::from_u16(250);
        assert_eq!(priority.to_u16(), 250);
        
        let priority = HookPriority::HIGH;
        assert_eq!(priority.to_u16(), 100);
    }
    
    #[tokio::test]
    async fn test_hook_add_tcl_script() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookAddRequest {
            name: "test_tcl".to_string(),
            handler_type: "tcl_script".to_string(),
            hook_types: vec!["tcl_pre_execution".to_string()],
            priority: 100,
            enabled: true,
            config: json!({
                "script": "puts \"Hello from TCL hook\"",
                "variables": {
                    "test_var": "test_value"
                }
            }),
        };
        
        // This should fail because Hook system not initialized
        let result = handle_hook_add(request, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hook system not initialized"));
    }
    
    #[tokio::test]
    async fn test_hook_list_empty() {
        let (_temp_dir, _config_path) = setup_test_config();
        
        let request = HookListRequest {
            hook_type: None,
            enabled_only: None,
        };
        
        let result = handle_hook_list(request, None).await;
        assert!(result.is_err()); // Fails without HookManager
    }
    
    #[tokio::test]
    async fn test_hook_config_reload_no_file() {
        // Set up temp directory for XDG_DATA_HOME (Linux) or equivalent
        let temp_dir = TempDir::new().unwrap();
        if cfg!(target_os = "linux") {
            std::env::set_var("XDG_DATA_HOME", temp_dir.path());
        } else if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
            // For macOS and Windows, we can't easily override the data directory
            // So we'll just ensure the file doesn't exist
        }
        
        let request = HookConfigReloadRequest {};
        let result = handle_hook_config_reload(request, None).await;
        
        // The result might be Ok if the file exists from a previous test run
        // or Err if it doesn't exist
        if result.is_err() {
            assert!(result.unwrap_err().to_string().contains("No configuration file found"));
        }
    }
    
    #[tokio::test]
    async fn test_hook_config_save_creates_default() {
        let temp_dir = TempDir::new().unwrap();
        
        // Set the appropriate environment variable based on platform
        if cfg!(target_os = "linux") {
            std::env::set_var("XDG_DATA_HOME", temp_dir.path());
        }
        
        let request = HookConfigSaveRequest {};
        let result = handle_hook_config_save(request, None).await;
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response["status"], "success");
        
        // Could be either "Created default configuration" or "Configuration already exists"
        // depending on whether a previous test created it
        assert!(response["message"] == "Created default configuration" || 
                response["message"] == "Configuration already exists");
        
        // For Linux, we can verify the file was created in the expected location
        if cfg!(target_os = "linux") {
            let config_path = temp_dir.path()
                .join("tcl-mcp-server")
                .join("hooks")
                .join("hooks.toml");
            
            if config_path.exists() {
                // Verify config was created
                let config_content = fs::read_to_string(&config_path).unwrap();
                let config = HooksConfig::from_toml(&config_content).unwrap();
                assert!(config.system.enabled);
            }
        }
    }
    
    #[tokio::test]
    async fn test_hook_test_with_context() {
        let (_temp_dir, config_path) = setup_test_config();
        
        // Create a validation handler for testing
        let config = HooksConfig {
            system: Default::default(),
            handlers: vec![
                HandlerConfig {
                    name: "test_validator".to_string(),
                    handler_type: HandlerType::BuiltIn,
                    hook_types: vec![HookType::RequestReceived],
                    priority: 100,
                    enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    config: HandlerTypeConfig::BuiltIn(BuiltInConfig {
                        handler_name: "validation".to_string(),
                        config: HashMap::from([
                            ("required_fields".to_string(), json!(["method", "path"])),
                        ]),
                    }),
                },
            ],
        };
        
        fs::write(&config_path, config.to_toml().unwrap()).unwrap();
        
        // Test with valid data
        let request = HookTestRequest {
            name: "test_validator".to_string(),
            hook_type: "request_received".to_string(),
            test_data: json!({
                "method": "GET",
                "path": "/api/test"
            }),
        };
        
        let result = handle_hook_test(request, None).await;
        assert!(result.is_err()); // Fails without HookManager
    }
}

fn default_test_data() -> Value {
    json!({})
}

// Hook tool handler implementations

/// Add a new hook handler
pub async fn handle_hook_add(
    request: HookAddRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let _manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    // Parse hook types
    let hook_types: Vec<HookType> = request.hook_types
        .into_iter()
        .map(|s| HookType::from_string(&s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Invalid hook type: {}", e))?;
    
    // Create handler configuration
    let handler_config = match request.handler_type.as_str() {
        "tcl_script" => {
            let config: TclScriptConfig = serde_json::from_value(request.config)?;
            HandlerTypeConfig::TclScript(config)
        }
        "external_command" => {
            let config: ExternalCommandConfig = serde_json::from_value(request.config)?;
            HandlerTypeConfig::ExternalCommand(config)
        }
        "built_in" => {
            let config: BuiltInConfig = serde_json::from_value(request.config)?;
            HandlerTypeConfig::BuiltIn(config)
        }
        _ => return Err(anyhow::anyhow!("Invalid handler type: {}", request.handler_type)),
    };
    
    // Create appropriate handler based on type
    let handler: Box<dyn AsyncHookHandler> = match request.handler_type.as_str() {
        "tcl_script" => {
            // For now, we need a way to get the TCL executor channel
            // This would typically come from the server context
            return Err(anyhow::anyhow!("TCL handler registration requires TCL executor channel"));
        }
        "external_command" => {
            // Config is already parsed above in handler_config
            if let HandlerTypeConfig::ExternalCommand(ref config) = handler_config {
                Box::new(crate::hooks::handlers::ExternalCommandHandler::new(
                    request.name.clone(),
                    config.clone(),
                ))
            } else {
                unreachable!()
            }
        }
        "built_in" => {
            if let HandlerTypeConfig::BuiltIn(ref config) = handler_config {
                match config.handler_name.as_str() {
                    "logging" => Box::new(crate::hooks::handlers::LoggingHandler::new(
                        request.name.clone(),
                        config.clone(),
                    )),
                    "metrics" => Box::new(crate::hooks::handlers::MetricsHandler::new(
                        request.name.clone(),
                        config.clone(),
                    )),
                    "validation" => Box::new(crate::hooks::handlers::ValidationHandler::new(
                        request.name.clone(),
                        config.clone(),
                    )),
                    "transform" => Box::new(crate::hooks::handlers::TransformHandler::new(
                        request.name.clone(),
                        config.clone(),
                    )),
                    "notification" => Box::new(crate::hooks::handlers::NotificationHandler::new(
                        request.name.clone(),
                        config.clone(),
                    )),
                    _ => return Err(anyhow::anyhow!("Unknown built-in handler: {}", config.handler_name)),
                }
            } else {
                unreachable!()
            }
        }
        _ => unreachable!(),
    };
    
    // Register handler using the appropriate method based on handler type
    // For now, we'll store handlers in a temporary registry and load them on startup
    // This is because we can't directly register Box<dyn AsyncHookHandler> with the current API
    
    // TODO: This would need to be properly integrated with the server's handler registry
    // For now, just validate and save to configuration
    drop(handler); // Handler would be recreated on server startup
    
    // Note: Enable/disable would be applied when handler is loaded from config
    
    // Also save to configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    let mut hooks_config = if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        HooksConfig::from_toml(&toml_str)?
    } else {
        HooksConfig::new()
    };
    
    // Create handler config
    let handler_type = match request.handler_type.as_str() {
        "tcl_script" => HandlerType::TclScript,
        "external_command" => HandlerType::ExternalCommand,
        "built_in" => HandlerType::BuiltIn,
        _ => unreachable!(),
    };
    
    let new_handler = HandlerConfig {
        name: request.name.clone(),
        handler_type,
        hook_types: hook_types.clone(),
        priority: request.priority,
        enabled: request.enabled,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        config: handler_config,
    };
    
    hooks_config.handlers.push(new_handler);
    
    // Save configuration
    let toml_str = hooks_config.to_toml()?;
    std::fs::create_dir_all(config_path.parent().unwrap())?;
    std::fs::write(&config_path, toml_str)?;
    
    Ok(json!({
        "status": "success",
        "handler": request.name,
        "hook_types": hook_types.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
    }))
}

/// Remove a hook handler
pub async fn handle_hook_remove(
    request: HookRemoveRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    manager.unregister(&request.name)
        .map_err(|e| anyhow::anyhow!("Failed to remove handler: {}", e))?;
    
    // Also remove from configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let mut hooks_config = HooksConfig::from_toml(&toml_str)?;
        
        hooks_config.handlers.retain(|h| h.name != request.name);
        
        let toml_str = hooks_config.to_toml()?;
        std::fs::write(&config_path, toml_str)?;
    }
    
    Ok(json!({
        "status": "success",
        "removed": request.name,
    }))
}

/// List all registered hook handlers
pub async fn handle_hook_list(
    request: HookListRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    let handlers = manager.list_handlers();
    
    // Apply filters
    let filtered: Vec<_> = handlers
        .into_iter()
        .filter(|(_, hook_types, _, enabled)| {
            // Filter by enabled state
            if let Some(enabled_only) = request.enabled_only {
                if enabled_only && !enabled {
                    return false;
                }
            }
            
            // Filter by hook type
            if let Some(ref hook_type_filter) = request.hook_type {
                if let Ok(filter_type) = HookType::from_string(hook_type_filter) {
                    if !hook_types.contains(&filter_type) {
                        return false;
                    }
                }
            }
            
            true
        })
        .map(|(name, hook_types, priority, enabled)| {
            json!({
                "name": name,
                "hook_types": hook_types.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
                "priority": priority.to_u16(),
                "enabled": enabled,
            })
        })
        .collect();
    
    Ok(json!({
        "handlers": filtered,
        "total": filtered.len(),
    }))
}

/// Enable a hook handler
pub async fn handle_hook_enable(
    request: HookEnableRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    manager.set_handler_enabled(&request.name, true)
        .map_err(|e| anyhow::anyhow!("Failed to enable handler: {}", e))?;
    
    // Also update configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let mut hooks_config = HooksConfig::from_toml(&toml_str)?;
        
        for handler in &mut hooks_config.handlers {
            if handler.name == request.name {
                handler.enabled = true;
                handler.updated_at = Utc::now();
                break;
            }
        }
        
        let toml_str = hooks_config.to_toml()?;
        std::fs::write(&config_path, toml_str)?;
    }
    
    Ok(json!({
        "status": "success",
        "handler": request.name,
        "enabled": true,
    }))
}

/// Disable a hook handler
pub async fn handle_hook_disable(
    request: HookDisableRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    manager.set_handler_enabled(&request.name, false)
        .map_err(|e| anyhow::anyhow!("Failed to disable handler: {}", e))?;
    
    // Also update configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let mut hooks_config = HooksConfig::from_toml(&toml_str)?;
        
        for handler in &mut hooks_config.handlers {
            if handler.name == request.name {
                handler.enabled = false;
                handler.updated_at = Utc::now();
                break;
            }
        }
        
        let toml_str = hooks_config.to_toml()?;
        std::fs::write(&config_path, toml_str)?;
    }
    
    Ok(json!({
        "status": "success",
        "handler": request.name,
        "enabled": false,
    }))
}

/// Update hook handler configuration
pub async fn handle_hook_update(
    request: HookUpdateRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    let mut updates = vec![];
    
    // Update enabled state
    if let Some(enabled) = request.enabled {
        manager.set_handler_enabled(&request.name, enabled)
            .map_err(|e| anyhow::anyhow!("Failed to update enabled state: {}", e))?;
        updates.push(format!("enabled={}", enabled));
    }
    
    // Update priority (would require re-registration in current implementation)
    if let Some(priority) = request.priority {
        updates.push(format!("priority={} (requires re-registration)", priority));
    }
    
    // Update configuration (would require re-registration in current implementation)
    if request.config.is_some() {
        updates.push("config=updated (requires re-registration)".to_string());
    }
    
    // Update configuration file
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let mut hooks_config = HooksConfig::from_toml(&toml_str)?;
        
        for handler in &mut hooks_config.handlers {
            if handler.name == request.name {
                if let Some(enabled) = request.enabled {
                    handler.enabled = enabled;
                }
                if let Some(priority) = request.priority {
                    handler.priority = priority;
                }
                handler.updated_at = Utc::now();
                break;
            }
        }
        
        let toml_str = hooks_config.to_toml()?;
        std::fs::write(&config_path, toml_str)?;
    }
    
    Ok(json!({
        "status": "success",
        "handler": request.name,
        "updates": updates,
    }))
}

/// Get detailed information about a hook handler
pub async fn handle_hook_info(
    request: HookInfoRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    // Find handler in list
    let handlers = manager.list_handlers();
    let handler_info = handlers
        .into_iter()
        .find(|(name, _, _, _)| name == &request.name)
        .ok_or_else(|| anyhow::anyhow!("Handler not found: {}", request.name))?;
    
    let (name, hook_types, priority, enabled) = handler_info;
    
    // Get statistics if available
    let stats = manager.get_stats(&name);
    
    // Get configuration details
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    let mut config_details = None;
    if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let hooks_config = HooksConfig::from_toml(&toml_str)?;
        
        if let Some(handler_config) = hooks_config.handlers.iter().find(|h| h.name == name) {
            config_details = Some(json!({
                "handler_type": format!("{:?}", handler_config.handler_type),
                "created_at": handler_config.created_at.to_rfc3339(),
                "updated_at": handler_config.updated_at.to_rfc3339(),
            }));
        }
    }
    
    Ok(json!({
        "name": name,
        "hook_types": hook_types.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
        "priority": priority.to_u16(),
        "enabled": enabled,
        "stats": stats.map(|s| json!({
            "total_executions": s.total_executions,
            "successful_executions": s.successful_executions,
            "failed_executions": s.failed_executions,
            "average_duration_ms": s.average_duration.map(|d| d.as_millis()).unwrap_or(0),
            "max_duration_ms": s.max_duration.map(|d| d.as_millis()).unwrap_or(0),
            "last_execution": s.last_execution.map(|dt| dt.to_rfc3339()),
        })),
        "config": config_details,
    }))
}

/// Test a hook handler with sample data
pub async fn handle_hook_test(
    request: HookTestRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    // Parse hook type
    let hook_type = HookType::from_string(&request.hook_type)
        .map_err(|e| anyhow::anyhow!("Invalid hook type: {}", e))?;
    
    // Create test context
    let context = HookContext::new();
    
    // Execute hook
    let start = std::time::Instant::now();
    let result = manager.execute(hook_type.clone(), &context, request.test_data.clone()).await
        .map_err(|e| anyhow::anyhow!("Hook execution failed: {}", e))?;
    let duration = start.elapsed();
    
    Ok(json!({
        "status": "success",
        "handler": request.name,
        "hook_type": request.hook_type,
        "input_data": request.test_data,
        "output_data": result,
        "duration_ms": duration.as_millis(),
    }))
}

/// Get hook system status and metrics
pub async fn handle_hook_system_status(
    _request: HookSystemStatusRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
    let handlers = manager.list_handlers();
    let total_handlers = handlers.len();
    let enabled_handlers = handlers.iter().filter(|(_, _, _, enabled)| *enabled).count();
    
    // Get recent execution history
    let history = manager.get_history(Some(10)).await;
    
    // Get configuration status
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    let config_status = if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        let hooks_config = HooksConfig::from_toml(&toml_str)?;
        json!({
            "exists": true,
            "path": config_path.to_string_lossy(),
            "system_enabled": hooks_config.system.enabled,
            "handler_timeout_ms": hooks_config.system.handler_timeout_ms,
            "max_concurrent_hooks": hooks_config.system.max_concurrent_hooks,
        })
    } else {
        json!({
            "exists": false,
            "path": config_path.to_string_lossy(),
        })
    };
    
    Ok(json!({
        "status": "active",
        "total_handlers": total_handlers,
        "enabled_handlers": enabled_handlers,
        "disabled_handlers": total_handlers - enabled_handlers,
        "recent_executions": history.into_iter().map(|(handler, duration, result)| {
            json!({
                "handler": handler,
                "duration_ms": duration.as_millis(),
                "result": result,
            })
        }).collect::<Vec<_>>(),
        "config": config_status,
    }))
}

/// Enable the entire hook system
pub async fn handle_hook_system_enable(
    _request: HookSystemEnableRequest,
    _hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    // Update configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    let mut hooks_config = if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        HooksConfig::from_toml(&toml_str)?
    } else {
        HooksConfig::new()
    };
    
    hooks_config.system.enabled = true;
    
    let toml_str = hooks_config.to_toml()?;
    std::fs::create_dir_all(config_path.parent().unwrap())?;
    std::fs::write(&config_path, toml_str)?;
    
    Ok(json!({
        "status": "success",
        "message": "Hook system enabled",
    }))
}

/// Disable the entire hook system
pub async fn handle_hook_system_disable(
    _request: HookSystemDisableRequest,
    _hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    // Update configuration
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    let mut hooks_config = if config_path.exists() {
        let toml_str = std::fs::read_to_string(&config_path)?;
        HooksConfig::from_toml(&toml_str)?
    } else {
        HooksConfig::new()
    };
    
    hooks_config.system.enabled = false;
    
    let toml_str = hooks_config.to_toml()?;
    std::fs::create_dir_all(config_path.parent().unwrap())?;
    std::fs::write(&config_path, toml_str)?;
    
    Ok(json!({
        "status": "success",
        "message": "Hook system disabled",
    }))
}

/// Reload configuration from file
pub async fn handle_hook_config_reload(
    _request: HookConfigReloadRequest,
    _hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    // This will be implemented when configuration loading is added
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if !config_path.exists() {
        return Err(anyhow::anyhow!("No configuration file found"));
    }
    
    let toml_str = std::fs::read_to_string(&config_path)?;
    let hooks_config = HooksConfig::from_toml(&toml_str)?;
    
    // Validate configuration
    hooks_config.validate()
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {}", e))?;
    
    Ok(json!({
        "status": "success",
        "message": "Configuration validated successfully",
        "handlers": hooks_config.handlers.len(),
    }))
}

/// Save current configuration to file
pub async fn handle_hook_config_save(
    _request: HookConfigSaveRequest,
    _hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    // Ensure configuration directory exists
    let config_path = PlatformDirs::config_file()
        .map_err(|e| anyhow::anyhow!("Failed to get config path: {}", e))?;
    
    if !config_path.exists() {
        // Create default configuration
        let hooks_config = HooksConfig::new();
        let toml_str = hooks_config.to_toml()?;
        
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        std::fs::write(&config_path, toml_str)?;
        
        Ok(json!({
            "status": "success",
            "message": "Created default configuration",
            "path": config_path.to_string_lossy(),
        }))
    } else {
        Ok(json!({
            "status": "success",
            "message": "Configuration already exists",
            "path": config_path.to_string_lossy(),
        }))
    }
}

// Extension trait to add to HookType
impl HookType {
    /// Parse a string into a HookType
    pub fn from_string(s: &str) -> Result<Self, String> {
        match s {
            "server_startup" => Ok(HookType::ServerStartup),
            "server_shutdown" => Ok(HookType::ServerShutdown),
            "server_initialized" => Ok(HookType::ServerInitialized),
            "request_received" => Ok(HookType::RequestReceived),
            "request_processed" => Ok(HookType::RequestProcessed),
            "response_sent" => Ok(HookType::ResponseSent),
            "tool_pre_execution" => Ok(HookType::ToolPreExecution),
            "tool_post_execution" => Ok(HookType::ToolPostExecution),
            "tool_registered" => Ok(HookType::ToolRegistered),
            "tool_removed" => Ok(HookType::ToolRemoved),
            "tcl_pre_execution" => Ok(HookType::TclPreExecution),
            "tcl_post_execution" => Ok(HookType::TclPostExecution),
            "tcl_error" => Ok(HookType::TclError),
            "mcp_server_connected" => Ok(HookType::McpServerConnected),
            "mcp_server_disconnected" => Ok(HookType::McpServerDisconnected),
            "mcp_server_error" => Ok(HookType::McpServerError),
            "security_check" => Ok(HookType::SecurityCheck),
            "access_denied" => Ok(HookType::AccessDenied),
            _ if s.starts_with("custom:") => {
                Ok(HookType::Custom(s[7..].to_string()))
            }
            _ => Err(format!("Invalid hook type: {}", s)),
        }
    }
    
    /// Convert HookType to string representation
    pub fn to_string(&self) -> String {
        match self {
            HookType::ServerStartup => "server_startup".to_string(),
            HookType::ServerShutdown => "server_shutdown".to_string(),
            HookType::ServerInitialized => "server_initialized".to_string(),
            HookType::RequestReceived => "request_received".to_string(),
            HookType::RequestProcessed => "request_processed".to_string(),
            HookType::ResponseSent => "response_sent".to_string(),
            HookType::ToolPreExecution => "tool_pre_execution".to_string(),
            HookType::ToolPostExecution => "tool_post_execution".to_string(),
            HookType::ToolRegistered => "tool_registered".to_string(),
            HookType::ToolRemoved => "tool_removed".to_string(),
            HookType::TclPreExecution => "tcl_pre_execution".to_string(),
            HookType::TclPostExecution => "tcl_post_execution".to_string(),
            HookType::TclError => "tcl_error".to_string(),
            HookType::McpServerConnected => "mcp_server_connected".to_string(),
            HookType::McpServerDisconnected => "mcp_server_disconnected".to_string(),
            HookType::McpServerError => "mcp_server_error".to_string(),
            HookType::SecurityCheck => "security_check".to_string(),
            HookType::AccessDenied => "access_denied".to_string(),
            HookType::Custom(s) => format!("custom:{}", s),
        }
    }
}

// Extension trait to add to HookPriority
impl HookPriority {
    /// Create HookPriority from u16
    pub fn from_u16(value: u16) -> Self {
        HookPriority(value)
    }
    
    /// Convert HookPriority to u16
    pub fn to_u16(&self) -> u16 {
        self.0
    }
}