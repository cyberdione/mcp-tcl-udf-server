//! Validation hook handler

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult,
    ExecutionResult, BuiltInConfig,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use jsonschema::{Draft, JSONSchema};

/// Built-in validation handler
pub struct ValidationHandler {
    name: String,
    config: BuiltInConfig,
    schema: Option<JSONSchema>,
}

impl ValidationHandler {
    /// Create a new validation handler
    pub fn new(name: impl Into<String>, config: BuiltInConfig) -> Self {
        // Try to compile JSON schema if provided
        let schema = config.config
            .get("schema")
            .cloned()
            .and_then(|schema_value| {
                JSONSchema::options()
                    .with_draft(Draft::Draft7)
                    .compile(&schema_value)
                    .ok()
            });
        
        Self {
            name: name.into(),
            config,
            schema,
        }
    }
    
    /// Validate data against rules
    fn validate_rules(&self, data: &Value) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Check required fields
        if let Some(required) = self.config.config.get("required_fields").and_then(|v| v.as_array()) {
            for field in required {
                if let Some(field_name) = field.as_str() {
                    if !data.get(field_name).is_some() {
                        errors.push(format!("Missing required field: {}", field_name));
                    }
                }
            }
        }
        
        // Check forbidden fields
        if let Some(forbidden) = self.config.config.get("forbidden_fields").and_then(|v| v.as_array()) {
            for field in forbidden {
                if let Some(field_name) = field.as_str() {
                    if data.get(field_name).is_some() {
                        errors.push(format!("Forbidden field present: {}", field_name));
                    }
                }
            }
        }
        
        // Check field types
        if let Some(types) = self.config.config.get("field_types").and_then(|v| v.as_object()) {
            for (field_name, expected_type) in types {
                if let Some(field_value) = data.get(field_name) {
                    let actual_type = match field_value {
                        Value::Null => "null",
                        Value::Bool(_) => "boolean",
                        Value::Number(_) => "number",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    };
                    
                    if let Some(expected) = expected_type.as_str() {
                        if actual_type != expected {
                            errors.push(format!(
                                "Field '{}' has wrong type: expected {}, got {}",
                                field_name, expected, actual_type
                            ));
                        }
                    }
                }
            }
        }
        
        // Check value constraints
        if let Some(constraints) = self.config.config.get("constraints").and_then(|v| v.as_object()) {
            for (field_name, constraint) in constraints {
                if let Some(field_value) = data.get(field_name) {
                    // Min/max for numbers
                    if let Some(num_value) = field_value.as_f64() {
                        if let Some(min) = constraint.get("min").and_then(|v| v.as_f64()) {
                            if num_value < min {
                                errors.push(format!("Field '{}' below minimum: {} < {}", field_name, num_value, min));
                            }
                        }
                        if let Some(max) = constraint.get("max").and_then(|v| v.as_f64()) {
                            if num_value > max {
                                errors.push(format!("Field '{}' above maximum: {} > {}", field_name, num_value, max));
                            }
                        }
                    }
                    
                    // Length constraints for strings and arrays
                    let length = match field_value {
                        Value::String(s) => Some(s.len()),
                        Value::Array(a) => Some(a.len()),
                        _ => None,
                    };
                    
                    if let Some(len) = length {
                        if let Some(min_len) = constraint.get("min_length").and_then(|v| v.as_u64()) {
                            if len < min_len as usize {
                                errors.push(format!("Field '{}' too short: {} < {}", field_name, len, min_len));
                            }
                        }
                        if let Some(max_len) = constraint.get("max_length").and_then(|v| v.as_u64()) {
                            if len > max_len as usize {
                                errors.push(format!("Field '{}' too long: {} > {}", field_name, len, max_len));
                            }
                        }
                    }
                    
                    // Pattern matching for strings
                    if let (Some(pattern), Some(str_value)) = (
                        constraint.get("pattern").and_then(|v| v.as_str()),
                        field_value.as_str()
                    ) {
                        if let Ok(re) = regex::Regex::new(pattern) {
                            if !re.is_match(str_value) {
                                errors.push(format!("Field '{}' doesn't match pattern: {}", field_name, pattern));
                            }
                        }
                    }
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[async_trait]
impl AsyncHookHandler for ValidationHandler {
    async fn execute(
        &self,
        _context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        let data_to_validate = self.config.config
            .get("validate_payload")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
            .then(|| &payload.data)
            .unwrap_or(&Value::Null);
        
        // Validate against JSON schema if available
        if let Some(schema) = &self.schema {
            if let Err(errors) = schema.validate(data_to_validate) {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{}: {}", e.instance_path, e))
                    .collect();
                
                return Ok(ExecutionResult::Error {
                    message: format!("Schema validation failed: {}", error_messages.join(", ")),
                    details: Some(json!({ "code": "SCHEMA_VALIDATION_FAILED", "errors": error_messages })),
                });
            }
        }
        
        // Validate against custom rules
        if let Err(errors) = self.validate_rules(data_to_validate) {
            return Ok(ExecutionResult::Error {
                message: format!("Validation failed: {}", errors.join(", ")),
                details: Some(json!({ "code": "VALIDATION_FAILED", "errors": errors })),
            });
        }
        
        // Check if we should add validation status to data
        if self.config.config.get("add_validation_status").and_then(|v| v.as_bool()).unwrap_or(false) {
            let mut result = payload.data.clone();
            if let Value::Object(ref mut map) = result {
                map.insert("_validated".to_string(), json!({
                    "handler": self.name,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "schema_used": self.schema.is_some(),
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
    async fn test_json_schema_validation_success() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "number" }
            },
            "required": ["name"]
        });
        
        let config = config_from_json("validation", json!({
            "schema": schema,
        }));
        
        let handler = ValidationHandler::new("schema_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "name": "John",
                "age": 30
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_json_schema_validation_failure() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "number" }
            },
            "required": ["name", "age"]
        });
        
        let config = config_from_json("validation", json!({
            "schema": schema,
        }));
        
        let handler = ValidationHandler::new("schema_fail_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "name": "John"
                // Missing required "age"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, details } => {
                assert!(message.contains("Schema validation failed"));
                assert!(details.is_some());
            }
            _ => panic!("Expected Error result"),
        }
    }
    
    #[tokio::test]
    async fn test_required_fields_validation() {
        let config = config_from_json("validation", json!({
            "required_fields": ["id", "timestamp"],
        }));
        
        let handler = ValidationHandler::new("required_test", config);
        let context = HookContext::new();
        
        // Test with missing field
        let payload = HookPayload::new(
            HookType::ToolPreExecution,
            json!({
                "id": "123"
                // Missing timestamp
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, details } => {
                assert!(message.contains("Missing required field: timestamp"));
                assert!(details.is_some());
            }
            _ => panic!("Expected Error result"),
        }
    }
    
    #[tokio::test]
    async fn test_forbidden_fields_validation() {
        let config = config_from_json("validation", json!({
            "forbidden_fields": ["password", "secret"],
        }));
        
        let handler = ValidationHandler::new("forbidden_test", config);
        let context = HookContext::new();
        
        let payload = HookPayload::new(
            HookType::RequestProcessed,
            json!({
                "username": "user",
                "password": "hidden" // Forbidden field
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, .. } => {
                assert!(message.contains("Forbidden field present: password"));
            }
            _ => panic!("Expected Error result"),
        }
    }
    
    #[tokio::test]
    async fn test_field_type_validation() {
        let config = config_from_json("validation", json!({
            "field_types": {
                "count": "number",
                "active": "boolean",
                "name": "string"
            },
        }));
        
        let handler = ValidationHandler::new("type_test", config);
        let context = HookContext::new();
        
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "count": "not_a_number", // Wrong type
                "active": true,
                "name": "test"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, .. } => {
                assert!(message.contains("Field 'count' has wrong type"));
                assert!(message.contains("expected number, got string"));
            }
            _ => panic!("Expected Error result"),
        }
    }
    
    #[tokio::test]
    async fn test_constraint_validation() {
        let config = config_from_json("validation", json!({
            "constraints": {
                "age": {
                    "min": 18,
                    "max": 100
                },
                "name": {
                    "min_length": 2,
                    "max_length": 50,
                    "pattern": "^[a-zA-Z\\s]+$"
                }
            },
        }));
        
        let handler = ValidationHandler::new("constraint_test", config);
        let context = HookContext::new();
        
        // Test min constraint violation
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "age": 15, // Below minimum
                "name": "John Doe"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, .. } => {
                assert!(message.contains("Field 'age' below minimum: 15 < 18"));
            }
            _ => panic!("Expected Error result"),
        }
        
        // Test pattern constraint violation
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "age": 25,
                "name": "John123" // Contains numbers
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Error { message, .. } => {
                assert!(message.contains("doesn't match pattern"));
            }
            _ => panic!("Expected Error result"),
        }
    }
    
    #[tokio::test]
    async fn test_add_validation_status() {
        let config = config_from_json("validation", json!({
            "required_fields": ["id"],
            "add_validation_status": true,
        }));
        
        let handler = ValidationHandler::new("status_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::ToolPostExecution,
            json!({
                "id": "test-123",
                "data": "value"
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        match result {
            ExecutionResult::Replace(data) => {
                assert!(data.get("_validated").is_some());
                let status = &data["_validated"];
                assert_eq!(status["handler"], "status_test");
                assert!(status["timestamp"].is_string());
                assert_eq!(status["schema_used"], false);
            }
            _ => panic!("Expected Replace result"),
        }
    }
}