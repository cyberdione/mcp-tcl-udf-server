//! Data transformation hook handler

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult,
    ExecutionResult, BuiltInConfig,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use base64::{Engine as _, engine::general_purpose};

/// Built-in transform handler
pub struct TransformHandler {
    name: String,
    config: BuiltInConfig,
}

impl TransformHandler {
    /// Create a new transform handler
    pub fn new(name: impl Into<String>, config: BuiltInConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }
    
    /// Apply transformations to data
    fn transform(&self, data: Value) -> HookResult<Value> {
        let mut result = data;
        
        // Get transformation pipeline
        let transforms = self.config.config
            .get("transforms")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        
        for transform in transforms {
            if let Some(transform_type) = transform.get("type").and_then(|v| v.as_str()) {
                result = match transform_type {
                    "rename_field" => self.rename_field(result, &transform)?,
                    "remove_field" => self.remove_field(result, &transform)?,
                    "add_field" => self.add_field(result, &transform)?,
                    "base64_encode" => self.base64_encode(result, &transform)?,
                    "base64_decode" => self.base64_decode(result, &transform)?,
                    "lowercase" => self.lowercase(result, &transform)?,
                    "uppercase" => self.uppercase(result, &transform)?,
                    "truncate" => self.truncate(result, &transform)?,
                    "redact" => self.redact(result, &transform)?,
                    "merge" => self.merge(result, &transform)?,
                    _ => result, // Unknown transform, skip
                };
            }
        }
        
        Ok(result)
    }
    
    fn rename_field(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let (Some(from), Some(to)) = (
            transform.get("from").and_then(|v| v.as_str()),
            transform.get("to").and_then(|v| v.as_str())
        ) {
            if let Value::Object(ref mut map) = data {
                if let Some(value) = map.remove(from) {
                    map.insert(to.to_string(), value);
                }
            }
        }
        Ok(data)
    }
    
    fn remove_field(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            if let Value::Object(ref mut map) = data {
                map.remove(field);
            }
        }
        Ok(data)
    }
    
    fn add_field(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let (Some(field), Some(value)) = (
            transform.get("field").and_then(|v| v.as_str()),
            transform.get("value")
        ) {
            if let Value::Object(ref mut map) = data {
                map.insert(field.to_string(), value.clone());
            }
        }
        Ok(data)
    }
    
    fn base64_encode(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            if let Value::Object(ref mut map) = data {
                if let Some(Value::String(s)) = map.get(field) {
                    let encoded = general_purpose::STANDARD.encode(s);
                    map.insert(field.to_string(), Value::String(encoded));
                }
            }
        }
        Ok(data)
    }
    
    fn base64_decode(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            if let Value::Object(ref mut map) = data {
                if let Some(Value::String(s)) = map.get(field) {
                    match general_purpose::STANDARD.decode(s) {
                        Ok(decoded) => {
                            if let Ok(decoded_str) = String::from_utf8(decoded) {
                                map.insert(field.to_string(), Value::String(decoded_str));
                            }
                        }
                        Err(_) => {
                            // Keep original on decode error
                        }
                    }
                }
            }
        }
        Ok(data)
    }
    
    fn lowercase(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            if let Value::Object(ref mut map) = data {
                if let Some(Value::String(s)) = map.get_mut(field) {
                    *s = s.to_lowercase();
                }
            }
        }
        Ok(data)
    }
    
    fn uppercase(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            if let Value::Object(ref mut map) = data {
                if let Some(Value::String(s)) = map.get_mut(field) {
                    *s = s.to_uppercase();
                }
            }
        }
        Ok(data)
    }
    
    fn truncate(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let (Some(field), Some(length)) = (
            transform.get("field").and_then(|v| v.as_str()),
            transform.get("length").and_then(|v| v.as_u64())
        ) {
            if let Value::Object(ref mut map) = data {
                if let Some(Value::String(s)) = map.get_mut(field) {
                    if s.len() > length as usize {
                        *s = s.chars().take(length as usize).collect();
                    }
                }
            }
        }
        Ok(data)
    }
    
    fn redact(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(field) = transform.get("field").and_then(|v| v.as_str()) {
            let replacement = transform.get("replacement")
                .and_then(|v| v.as_str())
                .unwrap_or("***REDACTED***");
            
            if let Value::Object(ref mut map) = data {
                if map.contains_key(field) {
                    map.insert(field.to_string(), Value::String(replacement.to_string()));
                }
            }
        }
        Ok(data)
    }
    
    fn merge(&self, mut data: Value, transform: &Value) -> HookResult<Value> {
        if let Some(merge_data) = transform.get("data") {
            if let (Value::Object(ref mut target), Value::Object(source)) = (&mut data, merge_data) {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(data)
    }
}

#[async_trait]
impl AsyncHookHandler for TransformHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        match self.transform(payload.data.clone()) {
            Ok(transformed) => Ok(ExecutionResult::Replace(transformed)),
            Err(e) => Ok(ExecutionResult::Error {
                message: format!("Transform failed: {}", e),
                details: Some(json!({ "code": "TRANSFORM_ERROR" })),
            }),
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
    async fn test_rename_field() {
        let config = config_from_json("transform", json!({
            "transforms": [{
                "type": "rename_field",
                "from": "old_name",
                "to": "new_name"
            }]
        }));
        
        let handler = TransformHandler::new("rename_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestProcessed,
            json!({
                "old_name": "value",
                "other": "data"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["new_name"], "value");
                assert_eq!(data["other"], "data");
                assert!(data.get("old_name").is_none());
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_remove_field() {
        let config = config_from_json("transform", json!({
            "transforms": [{
                "type": "remove_field",
                "field": "sensitive"
            }]
        }));
        
        let handler = TransformHandler::new("remove_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ResponseSent,
            json!({
                "sensitive": "secret",
                "public": "visible"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert!(data.get("sensitive").is_none());
                assert_eq!(data["public"], "visible");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_add_field() {
        let config = config_from_json("transform", json!({
            "transforms": [{
                "type": "add_field",
                "field": "timestamp",
                "value": "2024-01-01T00:00:00Z"
            }]
        }));
        
        let handler = TransformHandler::new("add_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({ "data": "test" })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["timestamp"], "2024-01-01T00:00:00Z");
                assert_eq!(data["data"], "test");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_base64_encode_decode() {
        let config = config_from_json("transform", json!({
            "transforms": [
                {
                    "type": "base64_encode",
                    "field": "password"
                },
                {
                    "type": "base64_decode",
                    "field": "encoded"
                }
            ]
        }));
        
        let handler = TransformHandler::new("base64_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "password": "secret123",
                "encoded": "aGVsbG8gd29ybGQ=" // "hello world"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["password"], "c2VjcmV0MTIz"); // base64 of "secret123"
                assert_eq!(data["encoded"], "hello world");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_case_transforms() {
        let config = config_from_json("transform", json!({
            "transforms": [
                {
                    "type": "lowercase",
                    "field": "upper"
                },
                {
                    "type": "uppercase",
                    "field": "lower"
                }
            ]
        }));
        
        let handler = TransformHandler::new("case_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            json!({
                "upper": "HELLO WORLD",
                "lower": "hello world"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["upper"], "hello world");
                assert_eq!(data["lower"], "HELLO WORLD");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_truncate() {
        let config = config_from_json("transform", json!({
            "transforms": [{
                "type": "truncate",
                "field": "message",
                "length": 10
            }]
        }));
        
        let handler = TransformHandler::new("truncate_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestProcessed,
            json!({
                "message": "This is a very long message that needs truncation"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["message"], "This is a ");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_redact() {
        let config = config_from_json("transform", json!({
            "transforms": [
                {
                    "type": "redact",
                    "field": "ssn"
                },
                {
                    "type": "redact",
                    "field": "api_key",
                    "replacement": "[HIDDEN]"
                }
            ]
        }));
        
        let handler = TransformHandler::new("redact_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ResponseSent,
            json!({
                "ssn": "123-45-6789",
                "api_key": "sk-1234567890",
                "public": "visible"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["ssn"], "***REDACTED***");
                assert_eq!(data["api_key"], "[HIDDEN]");
                assert_eq!(data["public"], "visible");
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_merge() {
        let config = config_from_json("transform", json!({
            "transforms": [{
                "type": "merge",
                "data": {
                    "version": "1.0",
                    "timestamp": "2024-01-01",
                    "environment": "production"
                }
            }]
        }));
        
        let handler = TransformHandler::new("merge_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ServerInitialized,
            json!({
                "service": "api",
                "environment": "staging" // This will be overwritten
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["service"], "api");
                assert_eq!(data["version"], "1.0");
                assert_eq!(data["timestamp"], "2024-01-01");
                assert_eq!(data["environment"], "production"); // Overwritten
            }
            _ => panic!("Expected Replace result"),
        }
    }
    
    #[tokio::test]
    async fn test_multiple_transforms() {
        let config = config_from_json("transform", json!({
            "transforms": [
                {
                    "type": "rename_field",
                    "from": "username",
                    "to": "user"
                },
                {
                    "type": "uppercase",
                    "field": "user"
                },
                {
                    "type": "add_field",
                    "field": "processed",
                    "value": true
                },
                {
                    "type": "remove_field",
                    "field": "internal_id"
                }
            ]
        }));
        
        let handler = TransformHandler::new("multi_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestProcessed,
            json!({
                "username": "alice",
                "internal_id": "12345",
                "data": "test"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert_eq!(data["user"], "ALICE");
                assert_eq!(data["processed"], true);
                assert_eq!(data["data"], "test");
                assert!(data.get("username").is_none());
                assert!(data.get("internal_id").is_none());
            }
            _ => panic!("Expected Replace result"),
        }
    }
}