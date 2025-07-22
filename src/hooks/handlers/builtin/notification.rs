//! Notification hook handler

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult,
    ExecutionResult, BuiltInConfig,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use std::path::PathBuf;

/// Built-in notification handler
pub struct NotificationHandler {
    name: String,
    config: BuiltInConfig,
}

impl NotificationHandler {
    /// Create a new notification handler
    pub fn new(name: impl Into<String>, config: BuiltInConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }
    
    /// Send notification based on configured method
    async fn send_notification(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<()> {
        let method = self.config.config
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("log");
        
        match method {
            "log" => self.notify_log(context, payload).await,
            "file" => self.notify_file(context, payload).await,
            "webhook" => self.notify_webhook(context, payload).await,
            _ => Ok(()),
        }
    }
    
    /// Log notification
    async fn notify_log(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<()> {
        let message = self.format_message(context, payload);
        tracing::info!("Hook Notification: {}", message);
        Ok(())
    }
    
    /// File notification
    async fn notify_file(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<()> {
        let file_path = self.config.config
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/hook_notifications.log"));
        
        let message = self.format_message(context, payload);
        let timestamp = chrono::Utc::now().to_rfc3339();
        let line = format!("[{}] {}\n", timestamp, message);
        
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    tracing::error!("Failed to write notification to file: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to open notification file: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Webhook notification
    async fn notify_webhook(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<()> {
        let url = match self.config.config.get("webhook_url").and_then(|v| v.as_str()) {
            Some(url) => url,
            None => {
                tracing::warn!("Webhook notification configured but no URL provided");
                return Ok(());
            }
        };
        
        let mut webhook_payload = json!({
            "handler": self.name,
            "hook_type": payload.hook_type.to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "data": payload.data,
        });
        
        // Add context separately to avoid borrow issues
        if let Value::Object(ref mut map) = webhook_payload {
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
        
        // Use reqwest for webhook (would need to be added to dependencies)
        match reqwest::Client::new()
            .post(url)
            .json(&webhook_payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if !response.status().is_success() {
                    tracing::warn!(
                        "Webhook notification failed with status: {}",
                        response.status()
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to send webhook notification: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Format notification message
    fn format_message(&self, context: &HookContext, payload: &HookPayload) -> String {
        let template = self.config.config
            .get("message_template")
            .and_then(|v| v.as_str())
            .unwrap_or("Hook {hook_type} triggered by handler {handler}");
        
        let mut message = template.to_string();
        message = message.replace("{hook_type}", &payload.hook_type.to_string());
        message = message.replace("{handler}", &self.name);
        
        // Replace context values
        if let Some(request_id_value) = context.get_state("request_id") {
            if let Some(request_id) = request_id_value.as_str() {
                message = message.replace("{request_id}", request_id);
            }
        }
        
        if let Some(user_value) = context.get_state("user") {
            if let Some(user) = user_value.as_str() {
                message = message.replace("{user}", user);
            }
        }
        
        // Replace data values (simple implementation)
        if let Value::Object(map) = &payload.data {
            for (key, value) in map {
                let placeholder = format!("{{data.{}}}", key);
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                message = message.replace(&placeholder, &value_str);
            }
        }
        
        message
    }
}

#[async_trait]
impl AsyncHookHandler for NotificationHandler {
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        // Send notification
        if let Err(e) = self.send_notification(context, payload).await {
            tracing::error!("Notification failed: {}", e);
            // Don't fail the hook chain on notification error
        }
        
        // Check if we should add notification status
        if self.config.config.get("add_status").and_then(|v| v.as_bool()).unwrap_or(false) {
            let mut result = payload.data.clone();
            if let Value::Object(ref mut map) = result {
                map.insert("_notified".to_string(), json!({
                    "handler": self.name,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "method": self.config.config.get("method"),
                }));
            }
            Ok(ExecutionResult::Replace(result))
        } else {
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
    use std::collections::HashMap;
    use tempfile::NamedTempFile;
    use tokio::fs;
    
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
    async fn test_notification_log_method() {
        let config = config_from_json("notification", json!({
            "method": "log",
            "message_template": "Event {hook_type} from {handler}"
        }));
        
        let handler = NotificationHandler::new("log_notifier", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ServerStartup,
            json!({ "server": "test" })
        );
        
        // This should log without errors
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_notification_file_method() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_str().unwrap();
        
        let config = config_from_json("notification", json!({
            "method": "file",
            "file_path": file_path,
            "message_template": "Hook: {hook_type}, Handler: {handler}"
        }));
        
        let handler = NotificationHandler::new("file_notifier", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({ "endpoint": "/api/test" })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Verify file was written
        let contents = fs::read_to_string(file_path).await.unwrap();
        assert!(contents.contains("Hook: request_received"));
        assert!(contents.contains("Handler: file_notifier"));
    }
    
    #[tokio::test]
    async fn test_notification_with_context() {
        let config = config_from_json("notification", json!({
            "method": "log",
            "message_template": "User {user} triggered {hook_type} (Request: {request_id})"
        }));
        
        let handler = NotificationHandler::new("context_notifier", config);
        
        let context = HookContext::builder()
            .with_state("user".to_string(), json!("alice"))
            .with_state("request_id".to_string(), json!("req-123"))
            .build();
        
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            json!({ "tool": "test_tool" })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_notification_with_data_substitution() {
        let config = config_from_json("notification", json!({
            "method": "log",
            "message_template": "Tool {data.tool_name} executed with status {data.status}"
        }));
        
        let handler = NotificationHandler::new("data_notifier", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ToolPostExecution,
            json!({
                "tool_name": "list_dir",
                "status": "success",
                "duration_ms": 42
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_notification_add_status() {
        let config = config_from_json("notification", json!({
            "method": "log",
            "add_status": true
        }));
        
        let handler = NotificationHandler::new("status_notifier", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ResponseSent,
            json!({
                "response_code": 200,
                "response_time": 15
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert!(data.get("_notified").is_some());
                let notified = &data["_notified"];
                assert_eq!(notified["handler"], "status_notifier");
                assert!(notified["timestamp"].is_string());
                assert_eq!(notified["method"], "log");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_webhook_notification_no_url() {
        let config = config_from_json("notification", json!({
            "method": "webhook"
            // No webhook_url provided
        }));
        
        let handler = NotificationHandler::new("webhook_notifier", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::McpServerConnected,
            json!({ "server": "test_server" })
        );
        
        // Should not fail even without URL
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_default_message_template() {
        let config = config_from_json("notification", json!({
            "method": "log"
            // No message_template, should use default
        }));
        
        let handler = NotificationHandler::new("default_template", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ServerShutdown,
            json!({})
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_file_notification_default_path() {
        let config = config_from_json("notification", json!({
            "method": "file"
            // No file_path, should use default
        }));
        
        let handler = NotificationHandler::new("default_file", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::TclError,
            json!({ "error": "test error" })
        );
        
        // Should not fail even with default path
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
}