//! TCL script hook handler implementation

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult, HookError,
    ExecutionResult, TclScriptConfig,
};
use crate::tcl_executor::TclCommand;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

/// TCL script hook handler
pub struct TclScriptHandler {
    /// Handler name
    name: String,
    /// TCL script configuration
    config: TclScriptConfig,
    /// TCL executor channel
    executor: mpsc::Sender<TclCommand>,
}

impl TclScriptHandler {
    /// Create a new TCL script handler
    pub fn new(
        name: impl Into<String>,
        config: TclScriptConfig,
        executor: mpsc::Sender<TclCommand>,
    ) -> Self {
        Self {
            name: name.into(),
            config,
            executor,
        }
    }
    
    /// Build TCL script with variable substitutions
    fn build_script(&self, context: &HookContext, payload: &HookPayload) -> String {
        let script = self.config.script.clone();
        
        // Create TCL variables for context
        let mut tcl_vars = String::new();
        
        // Add hook payload as JSON
        tcl_vars.push_str(&format!(
            "set hook_type \"{}\"\n",
            payload.hook_type.to_string()
        ));
        tcl_vars.push_str(&format!(
            "set hook_data {}\n",
            serde_json::to_string(&payload.data).unwrap_or_default()
        ));
        
        // Add context metadata
        if let Some(request_id_value) = context.get_state("request_id") {
            if let Some(request_id) = request_id_value.as_str() {
                tcl_vars.push_str(&format!("set request_id \"{}\"\n", request_id));
            }
        }
        
        if let Some(user_value) = context.get_state("user") {
            if let Some(user) = user_value.as_str() {
                tcl_vars.push_str(&format!("set user \"{}\"\n", user));
            }
        }
        
        // Add custom variables from config
        for (key, value) in &self.config.variables {
            let tcl_value = match value {
                Value::String(s) => format!("\"{}\"", s),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                Value::Null => "\"\"".to_string(),
                _ => format!("{}", serde_json::to_string(value).unwrap_or_default()),
            };
            tcl_vars.push_str(&format!("set {} {}\n", key, tcl_value));
        }
        
        // Prepend variables to script
        format!("{}\n{}", tcl_vars, script)
    }
    
    /// Parse TCL result into ExecutionResult
    fn parse_result(&self, tcl_result: String) -> Result<ExecutionResult, HookError> {
        // Try to parse as JSON first
        if let Ok(json_result) = serde_json::from_str::<Value>(&tcl_result) {
            // Check if it's a structured result
            if let Some(result_type) = json_result.get("type").and_then(|v| v.as_str()) {
                match result_type {
                    "continue" => Ok(ExecutionResult::Continue),
                    "stop" => {
                        let data = json_result.get("data").cloned();
                        Ok(ExecutionResult::Stop(data))
                    }
                    "replace" => {
                        let data = json_result.get("data").cloned()
                            .ok_or_else(|| HookError::execution_failed(
                                &self.name,
                                "Replace result missing 'data' field",
                            ))?;
                        Ok(ExecutionResult::Replace(data))
                    }
                    "error" => {
                        let message = json_result.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error")
                            .to_string();
                        let details = json_result.get("code")
                            .map(|code| json!({ "code": code }));
                        Ok(ExecutionResult::Error { message, details })
                    }
                    _ => {
                        // Unknown type, treat as continue with data
                        Ok(ExecutionResult::Replace(json_result))
                    }
                }
            } else {
                // No type field, treat as data replacement
                Ok(ExecutionResult::Replace(json_result))
            }
        } else {
            // Not JSON, treat as simple string result
            if tcl_result.trim().is_empty() || tcl_result.trim() == "ok" {
                Ok(ExecutionResult::Continue)
            } else {
                // Non-empty string, treat as data replacement
                Ok(ExecutionResult::Replace(json!(tcl_result)))
            }
        }
    }
}

#[async_trait]
impl AsyncHookHandler for TclScriptHandler {
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        debug!(
            "Executing TCL hook handler '{}' for hook type '{:?}'",
            self.name, payload.hook_type
        );
        
        // Build script with substitutions
        let script = self.build_script(context, payload);
        
        // Create response channel
        let (tx, rx) = oneshot::channel();
        
        // Send execute command
        let command = TclCommand::Execute { script, response: tx };
        
        if let Err(e) = self.executor.send(command).await {
            error!("Failed to send TCL command: {}", e);
            return Err(HookError::execution_failed(
                &self.name,
                format!("Failed to send TCL command: {}", e),
            ));
        }
        
        // Wait for response
        match rx.await {
            Ok(Ok(result)) => {
                debug!("TCL script returned: {}", result);
                self.parse_result(result)
            }
            Ok(Err(e)) => {
                error!("TCL script error: {}", e);
                Err(HookError::execution_failed(
                    &self.name,
                    format!("TCL script error: {}", e),
                ))
            }
            Err(e) => {
                error!("Failed to receive TCL response: {}", e);
                Err(HookError::execution_failed(
                    &self.name,
                    format!("Failed to receive TCL response: {}", e),
                ))
            }
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn should_run(&self, context: &HookContext, _payload: &HookPayload) -> bool {
        // Check if handler is enabled in context
        if let Some(enabled) = context.get_state(&format!("handler.{}.enabled", self.name))
            .and_then(|v| v.as_bool()) {
            if !enabled {
                return false;
            }
        }
        
        // Could add more conditions based on context
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookType;
    use std::collections::HashMap;
    
    #[test]
    fn test_build_script() {
        let config = TclScriptConfig {
            script: "puts \"Hook: $hook_type, User: $user\"".to_string(),
            variables: HashMap::from([
                ("debug".to_string(), json!(true)),
                ("version".to_string(), json!("1.0")),
            ]),
        };
        
        let (tx, _rx) = mpsc::channel(1);
        let handler = TclScriptHandler::new("test", config, tx);
        
        let context = HookContext::builder()
            .with_state("user".to_string(), json!("alice"))
            .build();
        
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            json!({"tool": "test_tool"}),
        );
        
        let script = handler.build_script(&context, &payload);
        
        assert!(script.contains("set hook_type \"tool_pre_execution\""));
        assert!(script.contains("set user \"alice\""));
        assert!(script.contains("set debug 1"));
        assert!(script.contains("set version \"1.0\""));
    }
    
    #[test]
    fn test_parse_result_json() {
        let config = TclScriptConfig {
            script: String::new(),
            variables: HashMap::new(),
        };
        let (tx, _rx) = mpsc::channel(1);
        let handler = TclScriptHandler::new("test", config, tx);
        
        // Test continue result
        let result = handler.parse_result(r#"{"type": "continue"}"#.to_string()).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Test stop result
        let result = handler.parse_result(
            r#"{"type": "stop", "data": {"message": "stopped"}}"#.to_string()
        ).unwrap();
        assert!(matches!(result, ExecutionResult::Stop(_)));
        
        // Test replace result
        let result = handler.parse_result(
            r#"{"type": "replace", "data": {"new": "value"}}"#.to_string()
        ).unwrap();
        assert!(matches!(result, ExecutionResult::Replace(_)));
        
        // Test error result
        let result = handler.parse_result(
            r#"{"type": "error", "message": "test error"}"#.to_string()
        ).unwrap();
        assert!(matches!(result, ExecutionResult::Error { .. }));
    }
    
    #[test]
    fn test_parse_result_string() {
        let config = TclScriptConfig {
            script: String::new(),
            variables: HashMap::new(),
        };
        let (tx, _rx) = mpsc::channel(1);
        let handler = TclScriptHandler::new("test", config, tx);
        
        // Empty string = continue
        let result = handler.parse_result("".to_string()).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // "ok" = continue
        let result = handler.parse_result("ok".to_string()).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Other string = replace with string as JSON
        let result = handler.parse_result("hello world".to_string()).unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data, json!("hello world"));
            }
            _ => panic!("Expected Replace result"),
        }
    }
}