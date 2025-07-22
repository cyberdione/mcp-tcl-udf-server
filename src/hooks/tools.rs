//! Hook management tools for sbin namespace
//!
//! These tools provide privileged access to hook system management

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::hooks::{
    HookManager, HookType, AsyncHookHandler, HookContext,
    HookPriority, HookError, ExecutionResult, HookPayload, HooksConfig,
    HandlerConfig, HandlerType, HandlerTypeConfig, TclScriptConfig,
    ExternalCommandConfig, BuiltInConfig, PlatformDirs,
};
use chrono::Utc;
use async_trait::async_trait;

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

fn default_test_data() -> Value {
    json!({})
}

// Hook tool handler implementations

/// Add a new hook handler
pub async fn handle_hook_add(
    request: HookAddRequest,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<Value, anyhow::Error> {
    let manager = hook_manager.ok_or_else(|| anyhow::anyhow!("Hook system not initialized"))?;
    
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
    
    // Create a simple handler for now (Phase 3 will implement proper handlers)
    struct PlaceholderHandler {
        name: String,
    }
    
    #[async_trait]
    impl AsyncHookHandler for PlaceholderHandler {
        async fn execute(&self, _context: &HookContext, _payload: &HookPayload) -> Result<ExecutionResult, HookError> {
            Ok(ExecutionResult::Continue)
        }
        
        fn name(&self) -> &str {
            &self.name
        }
    }
    
    let handler = PlaceholderHandler { name: request.name.clone() };
    
    // Register handler
    manager.register(
        request.name.clone(),
        hook_types.clone(),
        handler,
        HookPriority::from_u16(request.priority),
    ).map_err(|e| anyhow::anyhow!("Failed to register handler: {}", e))?;
    
    // Enable/disable based on request
    if !request.enabled {
        manager.set_handler_enabled(&request.name, false)
            .map_err(|e| anyhow::anyhow!("Failed to set enabled state: {}", e))?;
    }
    
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