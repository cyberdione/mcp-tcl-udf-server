# TCL MCP Server Hooks System

## Overview

The TCL MCP Server Hooks System provides a flexible, async-first architecture for extending server functionality through customizable hook points. Hooks allow you to intercept and modify behavior at various stages of server operation without modifying core code.

## Table of Contents

1. [Features](#features)
2. [Architecture](#architecture)
3. [Hook Types](#hook-types)
4. [Handler Types](#handler-types)
5. [Quick Start](#quick-start)
6. [API Reference](#api-reference)
7. [Examples](#examples)
8. [Best Practices](#best-practices)

## Features

- **Async-First Design**: All hooks run asynchronously for maximum performance
- **Multiple Handler Types**: Built-in, TCL script, and external command handlers
- **Priority-Based Execution**: Control handler execution order
- **Chain Processing**: Handlers can transform data through the chain
- **Rate Limiting**: Prevent handler abuse with configurable limits
- **Statistics Tracking**: Monitor handler performance and reliability
- **Dynamic Management**: Add, remove, and configure hooks at runtime
- **Tool Integration**: Manage hooks through MCP tools

## Architecture

### Core Components

1. **HookManager**: Central coordinator for all hook operations
2. **AsyncHookHandler**: Trait that all handlers must implement
3. **HookContext**: Shared state passed to all handlers
4. **HookPayload**: Event-specific data for each hook execution
5. **ExecutionResult**: Controls flow through the handler chain

### Execution Flow

```
Event Triggered
    ↓
HookManager.execute()
    ↓
For each registered handler (by priority):
    ↓
    Check if enabled
    ↓
    Check rate limits
    ↓
    Execute handler
    ↓
    Process result:
    - Continue → next handler
    - Replace → update data, next handler
    - Stop → return immediately
    - Error → abort chain
    ↓
Return final data
```

## Hook Types

### Server Lifecycle

- `server_startup`: Server initialization phase
- `server_initialized`: After all components loaded
- `server_shutdown`: Server cleanup phase

### Request Processing

- `request_received`: Before processing any request
- `request_processed`: After successful processing
- `response_sent`: After response sent to client

### Tool Execution

- `tool_pre_execution`: Before tool execution
- `tool_post_execution`: After tool execution
- `tool_registered`: When new tool added
- `tool_removed`: When tool removed

### TCL Execution

- `tcl_pre_execution`: Before TCL script execution
- `tcl_post_execution`: After TCL script execution
- `tcl_error`: On TCL execution error

### MCP Server Events

- `mcp_server_connected`: MCP server connection established
- `mcp_server_disconnected`: MCP server disconnected
- `mcp_server_error`: MCP server connection error

### Security

- `security_check`: Security validation request
- `access_denied`: Access denied event

### Custom

- `custom:<name>`: User-defined hook types

## Handler Types

### 1. Built-in Handlers

Pre-built handlers for common tasks:

- **LoggingHandler**: Log hook events
- **MetricsHandler**: Track metrics
- **ValidationHandler**: Validate data
- **TransformHandler**: Transform data
- **NotificationHandler**: Send notifications

### 2. TCL Script Handler

Execute TCL scripts with full access to:
- Hook context
- Event payload
- Return flow control

### 3. External Command Handler

Run external processes with:
- JSON input/output
- Timeout control
- Environment variables

## Quick Start

### 1. Register a Simple Logger

```rust
use tcl_mcp_server::hooks::*;

// Create hook manager
let hook_manager = HookManager::new();

// Create logging handler
let config = BuiltInConfig {
    handler_name: "logging".to_string(),
    config: HashMap::from([
        ("level".to_string(), json!("info")),
        ("format".to_string(), json!("json")),
    ]),
};

let handler = LoggingHandler::new("request_logger", config);

// Register for request events
hook_manager.register(
    "request_logger",
    vec![HookType::RequestReceived],
    handler,
    HookPriority::NORMAL,
)?;
```

### 2. Add a TCL Script Hook

```tcl
# Save as ~/.local/share/tcl-mcp-server/hooks/scripts/auth_check.tcl
proc hook_execute {context payload} {
    set user [dict get $payload user]
    
    if {$user eq "admin"} {
        return [list continue {}]
    } else {
        return [list stop [dict create authorized false]]
    }
}
```

Register the script:

```rust
let script_config = TclScriptConfig {
    script_path: PathBuf::from("auth_check.tcl"),
    timeout: Some(Duration::from_secs(1)),
};

let handler = TclScriptHandler::new("auth_checker", script_config)?;

hook_manager.register(
    "auth_checker",
    vec![HookType::SecurityCheck],
    handler,
    HookPriority::HIGH,
)?;
```

### 3. Transform Data in Chain

```rust
// First handler adds timestamp
let add_timestamp = TransformHandler::new_add_field(
    "timestamp",
    json!(chrono::Utc::now().to_rfc3339())
);

// Second handler adds request ID
let add_request_id = TransformHandler::new_add_field(
    "request_id",
    json!(uuid::Uuid::new_v4().to_string())
);

// Register in order
hook_manager.register(
    "add_timestamp",
    vec![HookType::RequestReceived],
    add_timestamp,
    HookPriority::HIGH,
)?;

hook_manager.register(
    "add_request_id",
    vec![HookType::RequestReceived],
    add_request_id,
    HookPriority::NORMAL,
)?;
```

## API Reference

### HookManager

```rust
impl HookManager {
    /// Create new hook manager
    pub fn new() -> Self
    
    /// Register a hook handler
    pub fn register<H: AsyncHookHandler + 'static>(
        &self,
        name: impl Into<String>,
        hook_types: Vec<HookType>,
        handler: H,
        priority: HookPriority,
    ) -> HookResult<()>
    
    /// Execute hooks for an event
    pub async fn execute(
        &self,
        hook_type: HookType,
        context: &HookContext,
        data: serde_json::Value,
    ) -> HookResult<serde_json::Value>
    
    /// Enable/disable the hook system
    pub fn set_enabled(&self, enabled: bool)
    
    /// Enable/disable specific handler
    pub fn set_handler_enabled(&self, name: &str, enabled: bool) -> HookResult<()>
}
```

### AsyncHookHandler Trait

```rust
#[async_trait]
pub trait AsyncHookHandler: Send + Sync {
    /// Execute the hook handler
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult>;
    
    /// Get handler name
    fn name(&self) -> &str;
    
    /// Check if handler should run (optional)
    fn should_run(&self, _context: &HookContext, _payload: &HookPayload) -> bool {
        true
    }
}
```

### ExecutionResult

```rust
pub enum ExecutionResult {
    /// Continue to next handler
    Continue,
    
    /// Stop execution and return data
    Stop(Option<serde_json::Value>),
    
    /// Replace data and continue
    Replace(serde_json::Value),
    
    /// Retry the operation
    Retry {
        delay: Option<Duration>,
        max_attempts: Option<u32>,
    },
    
    /// Error occurred
    Error {
        message: String,
        details: Option<serde_json::Value>,
    },
}
```

### HookContext

```rust
impl HookContext {
    /// Create new context
    pub fn new() -> Self
    
    /// Builder pattern
    pub fn builder() -> HookContextBuilder
    
    /// Get state value
    pub fn get_state(&self, key: &str) -> Option<&serde_json::Value>
    
    /// Set state value  
    pub fn set_state(&mut self, key: String, value: serde_json::Value)
    
    /// Get metadata
    pub fn metadata(&self) -> &HashMap<String, serde_json::Value>
}
```

## Examples

See the [examples](../examples/hooks/) directory for complete examples:

- [Basic Logging](../examples/hooks/basic_logging.rs)
- [Request Validation](../examples/hooks/request_validation.rs)
- [TCL Script Integration](../examples/hooks/tcl_scripts/)
- [External Commands](../examples/hooks/external_commands/)
- [Complex Chains](../examples/hooks/complex_chains.rs)

## Best Practices

### 1. Handler Design

- Keep handlers focused on a single responsibility
- Use appropriate timeouts for external operations
- Handle errors gracefully
- Log important events for debugging

### 2. Performance

- Use HIGH priority sparingly
- Enable rate limiting for expensive operations
- Consider async execution for non-critical hooks
- Monitor handler statistics

### 3. Security

- Validate all input data
- Use external commands carefully
- Limit TCL script capabilities in production
- Implement proper access controls

### 4. Testing

- Unit test individual handlers
- Integration test handler chains
- Test error scenarios
- Verify timeout behavior

### 5. Configuration

- Use configuration files for complex setups
- Document all custom hooks
- Version control hook scripts
- Plan for handler updates

## Troubleshooting

### Common Issues

1. **Handler Not Executing**
   - Check if handler is enabled
   - Verify hook type registration
   - Check rate limits
   - Review handler priority

2. **Data Not Transformed**
   - Ensure Replace result is used
   - Check handler execution order
   - Verify data structure matches

3. **Performance Issues**
   - Review handler statistics
   - Check for blocking operations
   - Consider async handlers
   - Implement caching

4. **TCL Script Errors**
   - Check script syntax
   - Verify return format
   - Test in isolation
   - Check script permissions

## Future Enhancements

- Conditional execution based on expressions
- Handler dependency management
- Distributed hook execution
- Hook recording and replay
- Visual hook chain editor