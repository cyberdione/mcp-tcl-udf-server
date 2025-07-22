//! Integration tests for the hooks system with TCL MCP Server

use tcl_mcp_server::hooks::{
    HookManager, HookType, HookContext, HookPayload, HookPriority,
    AsyncHookHandler, HookResult, ExecutionResult,
    handlers::{LoggingHandler, MetricsHandler, ValidationHandler},
    BuiltInConfig,
};
use tcl_mcp_server::server::TclMcpServer;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

/// Test fixture for integration tests
struct TestFixture {
    server: TclMcpServer,
    hook_manager: Arc<HookManager>,
    _temp_dir: TempDir,
}

impl TestFixture {
    async fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        
        // Create server with standard configuration
        let server = TclMcpServer::new(false);
        let hook_manager = Arc::new(HookManager::new());
        
        // Note: In a real integration, we would need to modify TclMcpServer
        // to accept a hook manager. For now, we'll test the hooks independently
        // and demonstrate how they would integrate.
        
        Self {
            server,
            hook_manager,
            _temp_dir: temp_dir,
        }
    }
}

#[tokio::test]
async fn test_server_lifecycle_hooks() {
    let fixture = TestFixture::new().await;
    
    // Track hook calls
    let calls = Arc::new(RwLock::new(Vec::new()));
    let calls_clone = calls.clone();
    
    // Register startup hook
    fixture.hook_manager.register(
        "startup_handler",
        vec![HookType::ServerStartup],
        TestHandler::new("startup", calls_clone.clone()),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Register initialized hook
    fixture.hook_manager.register(
        "initialized_handler",
        vec![HookType::ServerInitialized],
        TestHandler::new("initialized", calls_clone.clone()),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Simulate server startup by manually triggering hooks
    // In a real integration, these would be called by the server
    let context = HookContext::new();
    
    // Trigger server startup hook
    fixture.hook_manager.execute(
        HookType::ServerStartup,
        &context,
        json!({"server": "tcl-mcp-server", "version": "1.0.0"})
    ).await.unwrap();
    
    // Trigger server initialized hook
    fixture.hook_manager.execute(
        HookType::ServerInitialized,
        &context,
        json!({"capabilities": {"tools": {}}})
    ).await.unwrap();
    
    // Check hooks were called
    let hook_calls = calls.read().await;
    assert!(hook_calls.contains(&"startup".to_string()));
    assert!(hook_calls.contains(&"initialized".to_string()));
}

#[tokio::test]
async fn test_tool_execution_hooks() {
    let fixture = TestFixture::new().await;
    
    // Create a validation handler for tool execution
    let validation_config = BuiltInConfig {
        handler_name: "validation".to_string(),
        config: HashMap::from([
            ("required_fields".to_string(), json!(["tool_name", "params"])),
        ]),
    };
    
    fixture.hook_manager.register(
        "tool_validator",
        vec![HookType::ToolPreExecution],
        ValidationHandler::new("tool_validator", validation_config),
        HookPriority::HIGH,
    ).unwrap();
    
    // Create a logging handler
    let logging_config = BuiltInConfig {
        handler_name: "logging".to_string(),
        config: HashMap::from([
            ("level".to_string(), json!("info")),
            ("format".to_string(), json!("json")),
        ]),
    };
    
    fixture.hook_manager.register(
        "tool_logger",
        vec![HookType::ToolPostExecution],
        LoggingHandler::new("tool_logger", logging_config),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Simulate tool execution
    let context = HookContext::builder()
        .with_state("user".to_string(), json!("test_user"))
        .with_state("request_id".to_string(), json!("req-123"))
        .build();
    
    // Pre-execution hook with valid data
    let pre_payload = HookPayload::new(
        HookType::ToolPreExecution,
        json!({
            "tool_name": "test_tool",
            "params": {"arg": "value"}
        })
    );
    
    let result = fixture.hook_manager.execute(
        HookType::ToolPreExecution,
        &context,
        pre_payload.data.clone()
    ).await.unwrap();
    
    // Should pass validation
    assert_eq!(result, pre_payload.data);
    
    // Post-execution hook
    let post_payload = HookPayload::new(
        HookType::ToolPostExecution,
        json!({
            "tool_name": "test_tool",
            "result": {"status": "success"},
            "duration_ms": 42
        })
    );
    
    let result = fixture.hook_manager.execute(
        HookType::ToolPostExecution,
        &context,
        post_payload.data.clone()
    ).await.unwrap();
    
    // Should log and continue
    assert_eq!(result, post_payload.data);
}

#[tokio::test]
async fn test_request_processing_hooks() {
    let fixture = TestFixture::new().await;
    
    // Create metrics handler for request tracking
    let metrics_config = BuiltInConfig {
        handler_name: "metrics".to_string(),
        config: HashMap::from([
            ("metric_type".to_string(), json!("counter")),
            ("metric_name".to_string(), json!("requests_total")),
            ("labels".to_string(), json!({"endpoint": "{data.endpoint}"})),
        ]),
    };
    
    fixture.hook_manager.register(
        "request_counter",
        vec![HookType::RequestReceived],
        MetricsHandler::new("request_counter", metrics_config),
        HookPriority::HIGH,
    ).unwrap();
    
    // Test request received
    let context = HookContext::new();
    let payload = HookPayload::new(
        HookType::RequestReceived,
        json!({
            "endpoint": "/api/test",
            "method": "GET"
        })
    );
    
    let result = fixture.hook_manager.execute(
        HookType::RequestReceived,
        &context,
        payload.data.clone()
    ).await.unwrap();
    
    // Metrics handler should not modify data
    assert_eq!(result, payload.data);
}

#[tokio::test]
async fn test_hook_chain_with_transformation() {
    let fixture = TestFixture::new().await;
    
    // Create a chain of handlers that transform data
    let _calls = Arc::new(RwLock::new(Vec::<String>::new()));
    
    // First handler adds a field
    fixture.hook_manager.register(
        "add_timestamp",
        vec![HookType::RequestProcessed],
        TransformHandler::new("add_timestamp", TransformType::AddField {
            name: "timestamp".to_string(),
            value: json!("2024-01-01T00:00:00Z"),
        }),
        HookPriority::HIGH,
    ).unwrap();
    
    // Second handler modifies a field
    fixture.hook_manager.register(
        "add_processed",
        vec![HookType::RequestProcessed],
        TransformHandler::new("add_processed", TransformType::AddField {
            name: "processed".to_string(),
            value: json!(true),
        }),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Execute hooks
    let context = HookContext::new();
    let initial_data = json!({
        "request_id": "req-123",
        "status": "success"
    });
    
    let result = fixture.hook_manager.execute(
        HookType::RequestProcessed,
        &context,
        initial_data
    ).await.unwrap();
    
    
    // Check transformations were applied
    assert_eq!(result["request_id"], "req-123");
    assert_eq!(result["status"], "success");
    assert_eq!(result["timestamp"], "2024-01-01T00:00:00Z");
    assert_eq!(result["processed"], true);
}

#[tokio::test]
async fn test_hook_error_handling() {
    let fixture = TestFixture::new().await;
    
    // Register a handler that returns an error
    fixture.hook_manager.register(
        "error_handler",
        vec![HookType::TclError],
        ErrorHandler::new("error_handler"),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Register a handler after the error handler
    let calls = Arc::new(RwLock::new(Vec::new()));
    fixture.hook_manager.register(
        "after_error",
        vec![HookType::TclError],
        TestHandler::new("after_error", calls.clone()),
        HookPriority::LOW,
    ).unwrap();
    
    // Execute hooks
    let context = HookContext::new();
    let payload = json!({
        "error": "test error"
    });
    
    let result = fixture.hook_manager.execute(
        HookType::TclError,
        &context,
        payload
    ).await;
    
    // Should get error from first handler
    assert!(result.is_err());
    
    // Second handler should not be called
    let hook_calls = calls.read().await;
    assert!(hook_calls.is_empty());
}

#[tokio::test]
async fn test_hook_stop_execution() {
    let fixture = TestFixture::new().await;
    
    // Register a handler that stops execution
    fixture.hook_manager.register(
        "security_stop",
        vec![HookType::SecurityCheck],
        StopHandler::new("security_stop"),
        HookPriority::HIGH,
    ).unwrap();
    
    // Register a handler after the stop handler
    let calls = Arc::new(RwLock::new(Vec::new()));
    fixture.hook_manager.register(
        "after_stop",
        vec![HookType::SecurityCheck],
        TestHandler::new("after_stop", calls.clone()),
        HookPriority::NORMAL,
    ).unwrap();
    
    // Execute hooks
    let context = HookContext::new();
    let payload = json!({
        "action": "dangerous_operation"
    });
    
    let result = fixture.hook_manager.execute(
        HookType::SecurityCheck,
        &context,
        payload
    ).await.unwrap();
    
    // Should get the stop result
    assert_eq!(result, json!({"blocked": true, "reason": "Security check failed"}));
    
    // Second handler should not be called
    let hook_calls = calls.read().await;
    assert!(hook_calls.is_empty());
}

// Helper handlers for testing

struct TestHandler {
    name: String,
    calls: Arc<RwLock<Vec<String>>>,
}

impl TestHandler {
    fn new(name: &str, calls: Arc<RwLock<Vec<String>>>) -> Self {
        Self {
            name: name.to_string(),
            calls,
        }
    }
}

#[async_trait::async_trait]
impl AsyncHookHandler for TestHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        _payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        self.calls.write().await.push(self.name.clone());
        Ok(ExecutionResult::Continue)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

enum TransformType {
    AddField { name: String, value: serde_json::Value },
}

struct TransformHandler {
    name: String,
    transform: TransformType,
}

impl TransformHandler {
    fn new(name: &str, transform: TransformType) -> Self {
        Self {
            name: name.to_string(),
            transform,
        }
    }
}

#[async_trait::async_trait]
impl AsyncHookHandler for TransformHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        // The payload.data contains the actual data passed to execute()
        let mut data = payload.data.clone();
        
        
        match &self.transform {
            TransformType::AddField { name, value } => {
                if let serde_json::Value::Object(ref mut map) = data {
                    map.insert(name.clone(), value.clone());
                }
            }
        }
        
        Ok(ExecutionResult::Replace(data))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

struct ErrorHandler {
    name: String,
}

impl ErrorHandler {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl AsyncHookHandler for ErrorHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        _payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        Ok(ExecutionResult::Error {
            message: "Simulated error".to_string(),
            details: Some(json!({ "handler": self.name })),
        })
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

struct StopHandler {
    name: String,
}

impl StopHandler {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl AsyncHookHandler for StopHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        _payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        Ok(ExecutionResult::Stop(Some(json!({
            "blocked": true,
            "reason": "Security check failed"
        }))))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}