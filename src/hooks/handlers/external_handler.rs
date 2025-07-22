//! External command hook handler implementation

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult, HookError,
    ExecutionResult, ExternalCommandConfig,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, warn};

/// External command hook handler
pub struct ExternalCommandHandler {
    /// Handler name
    name: String,
    /// External command configuration
    config: ExternalCommandConfig,
}

impl ExternalCommandHandler {
    /// Create a new external command handler
    pub fn new(name: impl Into<String>, config: ExternalCommandConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }
    
    /// Build environment variables for the command
    fn build_env(&self, context: &HookContext, payload: &HookPayload) -> HashMap<String, String> {
        let mut env = self.config.env.clone();
        
        // Add hook information
        env.insert("HOOK_TYPE".to_string(), payload.hook_type.to_string());
        env.insert("HOOK_DATA".to_string(), 
            serde_json::to_string(&payload.data).unwrap_or_default());
        
        // Add context information
        if let Some(request_id_value) = context.get_state("request_id") {
            if let Some(request_id) = request_id_value.as_str() {
                env.insert("HOOK_REQUEST_ID".to_string(), request_id.to_string());
            }
        }
        
        if let Some(user_value) = context.get_state("user") {
            if let Some(user) = user_value.as_str() {
                env.insert("HOOK_USER".to_string(), user.to_string());
            }
        }
        
        // Add handler name
        env.insert("HOOK_HANDLER".to_string(), self.name.clone());
        
        env
    }
    
    /// Build command arguments with substitutions
    fn build_args(&self, context: &HookContext, payload: &HookPayload) -> Vec<String> {
        self.config.args.iter().map(|arg| {
            let mut processed = arg.clone();
            
            // Simple template substitution
            processed = processed.replace("{hook_type}", &payload.hook_type.to_string());
            processed = processed.replace("{handler_name}", &self.name);
            
            // Replace context values
            if let Some(request_id_value) = context.get_state("request_id") {
                if let Some(request_id) = request_id_value.as_str() {
                    processed = processed.replace("{request_id}", request_id);
                }
            }
            
            if let Some(user_value) = context.get_state("user") {
                if let Some(user) = user_value.as_str() {
                    processed = processed.replace("{user}", user);
                }
            }
            
            processed
        }).collect()
    }
    
    /// Parse command output into ExecutionResult
    fn parse_output(&self, stdout: String, stderr: String, exit_code: i32) -> HookResult<ExecutionResult> {
        // Log stderr if present
        if !stderr.trim().is_empty() {
            warn!("Command stderr: {}", stderr);
        }
        
        // Check exit code
        if exit_code != 0 {
            return Ok(ExecutionResult::Error {
                message: format!("Command exited with code {}: {}", exit_code, stderr),
                details: Some(json!({ "exit_code": exit_code, "stderr": stderr })),
            });
        }
        
        // Try to parse stdout as JSON
        if let Ok(json_result) = serde_json::from_str::<Value>(&stdout) {
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
                        // Unknown type, treat as data
                        Ok(ExecutionResult::Replace(json_result))
                    }
                }
            } else {
                // No type field, treat as data
                Ok(ExecutionResult::Replace(json_result))
            }
        } else {
            // Not JSON, check for special strings
            let output = stdout.trim();
            if output.is_empty() || output == "ok" || output == "continue" {
                Ok(ExecutionResult::Continue)
            } else {
                // Treat as string data
                Ok(ExecutionResult::Replace(json!(output)))
            }
        }
    }
}

#[async_trait]
impl AsyncHookHandler for ExternalCommandHandler {
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        debug!(
            "Executing external command handler '{}' for hook type '{:?}'",
            self.name, payload.hook_type
        );
        
        // Build command
        let mut cmd = Command::new(&self.config.command);
        
        // Add arguments
        let args = self.build_args(context, payload);
        cmd.args(&args);
        
        // Set environment
        let env = self.build_env(context, payload);
        for (key, value) in env {
            cmd.env(key, value);
        }
        
        // Set up pipes
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        // Spawn process
        let mut child = cmd.spawn().map_err(|e| {
            error!("Failed to spawn command '{}': {}", self.config.command, e);
            HookError::execution_failed(
                &self.name,
                format!("Failed to spawn command: {}", e),
            )
        })?;
        
        // Write hook data to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let data = serde_json::to_string(&payload.data).unwrap_or_default();
            if let Err(e) = stdin.write_all(data.as_bytes()).await {
                warn!("Failed to write to command stdin: {}", e);
            }
        }
        
        // Wait for completion with timeout
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);
        let output = match timeout(timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                error!("Command execution error: {}", e);
                return Err(HookError::execution_failed(
                    &self.name,
                    format!("Command execution error: {}", e),
                ));
            }
            Err(_) => {
                error!("Command timed out after {}ms", self.config.timeout_ms);
                
                // Can't kill the process here as child has been moved
                // The process will be killed when it's dropped
                
                return Err(HookError::Timeout {
                    handler: self.name.clone(),
                    duration: timeout_duration,
                });
            }
        };
        
        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        debug!("Command exited with code {}", exit_code);
        
        self.parse_output(stdout, stderr, exit_code)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn should_run(&self, context: &HookContext, _payload: &HookPayload) -> bool {
        // Check if handler is enabled in context
        if let Some(enabled_value) = context.get_state(&format!("handler.{}.enabled", self.name)) {
            if let Some(enabled) = enabled_value.as_bool() {
                if !enabled {
                    return false;
                }
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookType;
    
    #[test]
    fn test_build_env() {
        let config = ExternalCommandConfig {
            command: "/bin/test".to_string(),
            args: vec![],
            env: HashMap::from([
                ("CUSTOM_VAR".to_string(), "custom_value".to_string()),
            ]),
            timeout_ms: 5000,
        };
        
        let handler = ExternalCommandHandler::new("test", config);
        
        let context = HookContext::builder()
            .with_state("user".to_string(), json!("alice"))
            .with_state("request_id".to_string(), json!("req-123"))
            .build();
        
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            json!({"tool": "test_tool"}),
        );
        
        let env = handler.build_env(&context, &payload);
        
        assert_eq!(env.get("CUSTOM_VAR"), Some(&"custom_value".to_string()));
        assert_eq!(env.get("HOOK_TYPE"), Some(&"tool_pre_execution".to_string()));
        assert_eq!(env.get("HOOK_USER"), Some(&"alice".to_string()));
        assert_eq!(env.get("HOOK_REQUEST_ID"), Some(&"req-123".to_string()));
        assert_eq!(env.get("HOOK_HANDLER"), Some(&"test".to_string()));
    }
    
    #[test]
    fn test_build_args() {
        let config = ExternalCommandConfig {
            command: "/bin/test".to_string(),
            args: vec![
                "--hook".to_string(),
                "{hook_type}".to_string(),
                "--user".to_string(),
                "{user}".to_string(),
                "--handler".to_string(),
                "{handler_name}".to_string(),
            ],
            env: HashMap::new(),
            timeout_ms: 5000,
        };
        
        let handler = ExternalCommandHandler::new("test_handler", config);
        
        let context = HookContext::builder()
            .with_state("user".to_string(), json!("bob"))
            .build();
        
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({}),
        );
        
        let args = handler.build_args(&context, &payload);
        
        assert_eq!(args, vec![
            "--hook",
            "request_received",
            "--user",
            "bob",
            "--handler",
            "test_handler",
        ]);
    }
    
    #[test]
    fn test_parse_output() {
        let config = ExternalCommandConfig {
            command: "/bin/test".to_string(),
            args: vec![],
            env: HashMap::new(),
            timeout_ms: 2000,
        };
        let handler = ExternalCommandHandler::new("test", config);
        
        // Test successful continue
        let result = handler.parse_output("continue".to_string(), "".to_string(), 0).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Test JSON response
        let result = handler.parse_output(
            r#"{"type": "replace", "data": {"new": "value"}}"#.to_string(),
            "".to_string(),
            0
        ).unwrap();
        assert!(matches!(result, ExecutionResult::Replace(_)));
        
        // Test error exit code
        let result = handler.parse_output(
            "".to_string(),
            "error message".to_string(),
            1
        ).unwrap();
        assert!(matches!(result, ExecutionResult::Error { .. }));
    }
}