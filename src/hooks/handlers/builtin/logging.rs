//! Logging hook handler

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult,
    ExecutionResult, BuiltInConfig,
};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, error, info, warn, Level};

/// Built-in logging handler
pub struct LoggingHandler {
    name: String,
    config: BuiltInConfig,
    level: Level,
    format: LogFormat,
}

#[derive(Debug, Clone, Copy)]
enum LogFormat {
    Json,
    Pretty,
    Compact,
}

impl LoggingHandler {
    /// Create a new logging handler
    pub fn new(name: impl Into<String>, config: BuiltInConfig) -> Self {
        // Parse log level from config
        let level = config.config
            .get("level")
            .and_then(|v| v.as_str())
            .and_then(|s| match s.to_lowercase().as_str() {
                "error" => Some(Level::ERROR),
                "warn" | "warning" => Some(Level::WARN),
                "info" => Some(Level::INFO),
                "debug" => Some(Level::DEBUG),
                "trace" => Some(Level::TRACE),
                _ => None,
            })
            .unwrap_or(Level::INFO);
        
        // Parse format from config
        let format = config.config
            .get("format")
            .and_then(|v| v.as_str())
            .and_then(|s| match s.to_lowercase().as_str() {
                "json" => Some(LogFormat::Json),
                "pretty" => Some(LogFormat::Pretty),
                "compact" => Some(LogFormat::Compact),
                _ => None,
            })
            .unwrap_or(LogFormat::Pretty);
        
        Self {
            name: name.into(),
            config,
            level,
            format,
        }
    }
    
    /// Format the log message
    fn format_message(&self, context: &HookContext, payload: &HookPayload) -> String {
        match self.format {
            LogFormat::Json => {
                {
                    let mut json_obj = serde_json::json!({
                        "hook_type": payload.hook_type.to_string(),
                        "handler": self.name,
                        "data": payload.data,
                    });
                    
                    if let Value::Object(ref mut map) = json_obj {
                        let mut context_obj = serde_json::Map::new();
                        
                        if let Some(request_id_value) = context.get_state("request_id") {
                            if let Some(request_id) = request_id_value.as_str() {
                                context_obj.insert("request_id".to_string(), Value::String(request_id.to_string()));
                            }
                        }
                        
                        if let Some(user_value) = context.get_state("user") {
                            if let Some(user) = user_value.as_str() {
                                context_obj.insert("user".to_string(), Value::String(user.to_string()));
                            }
                        }
                        
                        map.insert("context".to_string(), Value::Object(context_obj));
                    }
                    
                    json_obj.to_string()
                }
            }
            LogFormat::Pretty => {
                format!(
                    "Hook: {} | Handler: {} | Data: {}",
                    payload.hook_type.to_string(),
                    self.name,
                    serde_json::to_string_pretty(&payload.data).unwrap_or_default()
                )
            }
            LogFormat::Compact => {
                format!(
                    "[{}] {}: {}",
                    payload.hook_type.to_string(),
                    self.name,
                    serde_json::to_string(&payload.data).unwrap_or_default()
                )
            }
        }
    }
}

#[async_trait]
impl AsyncHookHandler for LoggingHandler {
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        let message = self.format_message(context, payload);
        
        // Log at configured level
        match self.level {
            Level::ERROR => error!("{}", message),
            Level::WARN => warn!("{}", message),
            Level::INFO => info!("{}", message),
            Level::DEBUG => debug!("{}", message),
            Level::TRACE => tracing::trace!("{}", message),
        }
        
        // Check if we should include data in result
        if self.config.config.get("include_in_result").and_then(|v| v.as_bool()).unwrap_or(false) {
            let log_entry = serde_json::json!({
                "logged": true,
                "level": self.level.to_string(),
                "message": message,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            
            // Add log entry to data
            if let Value::Object(mut map) = payload.data.clone() {
                map.insert("_log".to_string(), log_entry);
                Ok(ExecutionResult::Replace(Value::Object(map)))
            } else {
                Ok(ExecutionResult::Replace(serde_json::json!({
                    "_original": payload.data,
                    "_log": log_entry,
                })))
            }
        } else {
            // Just continue without modifying data
            Ok(ExecutionResult::Continue)
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{HookContext, HookPayload, HookType};
    use serde_json::json;
    use tracing_test::traced_test;
    use std::collections::HashMap;
    
    fn config_from_json(handler_name: &str, json_config: Value) -> BuiltInConfig {
        let mut config = HashMap::new();
        if let Value::Object(map) = json_config {
            for (k, v) in map {
                config.insert(k, v);
            }
        }
        BuiltInConfig {
            handler_name: handler_name.to_string(),
            config,
        }
    }
    
    #[tokio::test]
    #[traced_test]
    async fn test_json_format_logging() {
        let config = config_from_json("logging", json!({
            "level": "info",
            "format": "json",
        }));
        
        let handler = LoggingHandler::new("test_logger", config);
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::RequestReceived, json!({"test": true}));
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Check that log was emitted (tracing_test captures logs)
        assert!(logs_contain("test_logger"));
        assert!(logs_contain("request_received"));
    }
    
    #[tokio::test]
    #[traced_test] 
    async fn test_pretty_format_logging() {
        let config = config_from_json("logging", json!({
            "level": "debug",
            "format": "pretty",
        }));
        
        let handler = LoggingHandler::new("pretty_logger", config);
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ToolPreExecution, json!({"tool": "test_tool"}));
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        assert!(logs_contain("Hook: tool_pre_execution"));
        assert!(logs_contain("pretty_logger"));
    }
    
    #[tokio::test]
    async fn test_logging_with_context() {
        let config = config_from_json("logging", json!({
            "level": "info",
            "format": "json",
        }));
        
        let handler = LoggingHandler::new("context_logger", config);
        
        let context = HookContext::builder()
            .with_state("request_id".to_string(), json!("req-123"))
            .with_state("user".to_string(), json!("test_user"))
            .build();
        
        let payload = HookPayload::new(HookType::RequestProcessed, json!({"status": 200}));
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_include_in_result() {
        let config = config_from_json("logging", json!({
            "level": "info",
            "format": "compact",
            "include_in_result": true,
        }));
        
        let handler = LoggingHandler::new("result_logger", config);
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ResponseSent, json!({"response": "data"}));
        
        let result = handler.execute(&context, &payload).await.unwrap();
        
        match result {
            ExecutionResult::Replace(data) => {
                assert!(data.get("_log").is_some());
                let log_entry = &data["_log"];
                assert_eq!(log_entry["logged"], true);
                assert_eq!(log_entry["level"], "INFO");
                assert!(log_entry["timestamp"].is_string());
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_different_log_levels() {
        
        let levels = vec!["error", "warn", "info", "debug", "trace"];
        
        for level_str in levels {
            let config = config_from_json("logging", json!({
                "level": level_str,
                "format": "compact",
            }));
            
            let handler = LoggingHandler::new(format!("{}_logger", level_str), config);
            let context = HookContext::new();
            let payload = HookPayload::new(HookType::ServerStartup, json!({}));
            
            let result = handler.execute(&context, &payload).await.unwrap();
            assert!(matches!(result, ExecutionResult::Continue));
        }
    }
}