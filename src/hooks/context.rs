//! Hook execution context

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::any::{Any, TypeId};
use serde_json::Value;
use chrono::{DateTime, Utc};

/// Context passed to hook handlers during execution
#[derive(Clone)]
pub struct HookContext {
    /// Shared state across all hook executions
    shared_state: Arc<RwLock<HashMap<String, Value>>>,
    
    /// Type-safe storage for context values
    typed_storage: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
    
    /// Request-specific data
    request_data: Option<Value>,
    
    /// User information if available
    user_id: Option<String>,
    
    /// Execution start time
    start_time: DateTime<Utc>,
    
    /// Cancellation token
    cancelled: Arc<RwLock<bool>>,
    
    /// Parent context (for nested hooks)
    parent: Option<Box<HookContext>>,
}

impl HookContext {
    /// Create a new hook context
    pub fn new() -> Self {
        Self {
            shared_state: Arc::new(RwLock::new(HashMap::new())),
            typed_storage: Arc::new(RwLock::new(HashMap::new())),
            request_data: None,
            user_id: None,
            start_time: Utc::now(),
            cancelled: Arc::new(RwLock::new(false)),
            parent: None,
        }
    }
    
    /// Create a builder for the context
    pub fn builder() -> HookContextBuilder {
        HookContextBuilder::default()
    }
    
    /// Get a value from shared state
    pub fn get_state(&self, key: &str) -> Option<Value> {
        self.shared_state.read().ok()?.get(key).cloned()
    }
    
    /// Set a value in shared state
    pub fn set_state(&self, key: String, value: Value) -> Result<(), String> {
        self.shared_state
            .write()
            .map_err(|_| "Failed to acquire write lock")?
            .insert(key, value);
        Ok(())
    }
    
    /// Get a typed value from storage
    pub fn get_typed<T: Any + Send + Sync + Clone>(&self) -> Option<T> {
        let storage = self.typed_storage.read().ok()?;
        let type_id = TypeId::of::<T>();
        storage.get(&type_id)?.downcast_ref::<T>().cloned()
    }
    
    /// Store a typed value
    pub fn set_typed<T: Any + Send + Sync + Clone>(&self, value: T) -> Result<(), String> {
        let mut storage = self.typed_storage
            .write()
            .map_err(|_| "Failed to acquire write lock")?;
        let type_id = TypeId::of::<T>();
        storage.insert(type_id, Box::new(value));
        Ok(())
    }
    
    /// Get request data
    pub fn request_data(&self) -> Option<&Value> {
        self.request_data.as_ref()
    }
    
    /// Get user ID
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
    
    /// Get execution start time
    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }
    
    /// Check if execution has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.read().map(|c| *c).unwrap_or(false)
    }
    
    /// Cancel the execution
    pub fn cancel(&self) {
        if let Ok(mut cancelled) = self.cancelled.write() {
            *cancelled = true;
        }
    }
    
    /// Create a child context
    pub fn create_child(&self) -> Self {
        Self {
            shared_state: self.shared_state.clone(),
            typed_storage: Arc::new(RwLock::new(HashMap::new())),
            request_data: self.request_data.clone(),
            user_id: self.user_id.clone(),
            start_time: Utc::now(),
            cancelled: self.cancelled.clone(),
            parent: Some(Box::new(self.clone())),
        }
    }
    
    /// Get the parent context
    pub fn parent(&self) -> Option<&HookContext> {
        self.parent.as_deref()
    }
}

impl Default for HookContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for HookContext
#[derive(Default)]
pub struct HookContextBuilder {
    shared_state: HashMap<String, Value>,
    request_data: Option<Value>,
    user_id: Option<String>,
    parent: Option<Box<HookContext>>,
}

impl HookContextBuilder {
    /// Set shared state
    pub fn with_state(mut self, key: String, value: Value) -> Self {
        self.shared_state.insert(key, value);
        self
    }
    
    /// Set request data
    pub fn with_request_data(mut self, data: Value) -> Self {
        self.request_data = Some(data);
        self
    }
    
    /// Set user ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }
    
    /// Set parent context
    pub fn with_parent(mut self, parent: HookContext) -> Self {
        self.parent = Some(Box::new(parent));
        self
    }
    
    /// Build the context
    pub fn build(self) -> HookContext {
        let mut context = HookContext::new();
        
        if let Ok(mut state) = context.shared_state.write() {
            *state = self.shared_state;
        }
        
        context.request_data = self.request_data;
        context.user_id = self.user_id;
        context.parent = self.parent;
        
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_context_state() {
        let context = HookContext::new();
        
        // Set and get state
        context.set_state("key".to_string(), json!("value")).unwrap();
        assert_eq!(context.get_state("key"), Some(json!("value")));
        
        // Non-existent key
        assert_eq!(context.get_state("missing"), None);
    }
    
    #[test]
    fn test_typed_storage() {
        #[derive(Clone, Debug, PartialEq)]
        struct TestData {
            value: String,
        }
        
        let context = HookContext::new();
        let data = TestData { value: "test".to_string() };
        
        context.set_typed(data.clone()).unwrap();
        let retrieved: Option<TestData> = context.get_typed();
        assert_eq!(retrieved, Some(data));
    }
    
    #[test]
    fn test_context_builder() {
        let context = HookContext::builder()
            .with_state("key".to_string(), json!("value"))
            .with_user_id("user123".to_string())
            .with_request_data(json!({"method": "test"}))
            .build();
        
        assert_eq!(context.get_state("key"), Some(json!("value")));
        assert_eq!(context.user_id(), Some("user123"));
        assert_eq!(context.request_data(), Some(&json!({"method": "test"})));
    }
    
    #[test]
    fn test_cancellation() {
        let context = HookContext::new();
        
        assert!(!context.is_cancelled());
        context.cancel();
        assert!(context.is_cancelled());
    }
    
    #[test]
    fn test_child_context() {
        let parent = HookContext::builder()
            .with_state("parent_key".to_string(), json!("parent_value"))
            .build();
        
        let child = parent.create_child();
        
        // Child inherits shared state
        assert_eq!(child.get_state("parent_key"), Some(json!("parent_value")));
        
        // Child can modify shared state
        child.set_state("child_key".to_string(), json!("child_value")).unwrap();
        assert_eq!(parent.get_state("child_key"), Some(json!("child_value")));
        
        // Child has reference to parent
        assert!(child.parent().is_some());
    }
}