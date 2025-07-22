//! Central hook manager implementation

use crate::hooks::{
    AsyncHookHandler, HookContext, HookError, HookLifecycle, HookPayload,
    HookPriority, HookResult, HookType, ExecutionResult,
    types::HookStats,
};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Entry for a registered hook handler
struct HandlerEntry {
    handler: Arc<dyn AsyncHookHandler>,
    priority: HookPriority,
    stats: HookStats,
    enabled: bool,
    rate_limit: Option<RateLimit>,
}

/// Rate limiting state
struct RateLimit {
    max_calls: u32,
    window: Duration,
    calls: Vec<Instant>,
}

impl RateLimit {
    fn new(max_calls: u32, window: Duration) -> Self {
        Self {
            max_calls,
            window,
            calls: Vec::new(),
        }
    }
    
    fn check_and_update(&mut self) -> bool {
        let now = Instant::now();
        
        // Remove old calls outside the window
        self.calls.retain(|&call_time| now.duration_since(call_time) < self.window);
        
        // Check if we can make another call
        if self.calls.len() < self.max_calls as usize {
            self.calls.push(now);
            true
        } else {
            false
        }
    }
}

/// Central hook manager
pub struct HookManager {
    /// Registered handlers by hook type
    handlers: Arc<DashMap<HookType, Vec<String>>>,
    
    /// Handler entries by name
    entries: Arc<DashMap<String, HandlerEntry>>,
    
    /// Hook lifecycle manager
    lifecycle: Arc<HookLifecycle>,
    
    /// Global timeout for hook execution
    global_timeout: Duration,
    
    /// Whether hooks are enabled globally
    enabled: bool,
    
    /// Execution history for debugging
    history: Arc<tokio::sync::Mutex<Vec<ExecutionHistory>>>,
    
    /// Maximum history entries
    max_history: usize,
}

/// Execution history entry
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used for logging/debugging
struct ExecutionHistory {
    timestamp: Instant,
    hook_type: HookType,
    handler: String,
    duration: Duration,
    result: String,
}

impl HookManager {
    /// Create a new hook manager
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(DashMap::new()),
            entries: Arc::new(DashMap::new()),
            lifecycle: Arc::new(HookLifecycle::new()),
            global_timeout: Duration::from_secs(5),
            enabled: true,
            history: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            max_history: 1000,
        }
    }
    
    /// Register a synchronous hook handler
    pub fn register_sync<H: crate::hooks::HookHandler + 'static>(
        &self,
        name: impl Into<String>,
        hook_types: Vec<HookType>,
        handler: H,
        priority: HookPriority,
    ) -> HookResult<()> {
        let async_handler = crate::hooks::handler::SyncToAsyncHandler::new(handler);
        self.register(name, hook_types, async_handler, priority)
    }
    
    /// Register an async hook handler
    pub fn register<H: AsyncHookHandler + 'static>(
        &self,
        name: impl Into<String>,
        hook_types: Vec<HookType>,
        handler: H,
        priority: HookPriority,
    ) -> HookResult<()> {
        let name = name.into();
        
        // Check for duplicate registration
        if self.entries.contains_key(&name) {
            return Err(HookError::RegistrationFailed(format!(
                "Handler '{}' already registered",
                name
            )));
        }
        
        // Create handler entry
        let entry = HandlerEntry {
            handler: Arc::new(handler),
            priority,
            stats: HookStats::default(),
            enabled: true,
            rate_limit: None,
        };
        
        // Register handler
        self.entries.insert(name.clone(), entry);
        
        // Register for each hook type
        for hook_type in hook_types {
            let mut handlers = self.handlers.entry(hook_type).or_insert_with(Vec::new);
            handlers.push(name.clone());
            
            // Sort by priority
            let entries = &self.entries;
            handlers.sort_by_key(|h| {
                entries
                    .get(h)
                    .map(|e| e.priority)
                    .unwrap_or(HookPriority::NORMAL)
            });
        }
        
        Ok(())
    }
    
    /// Unregister a hook handler
    pub fn unregister(&self, name: &str) -> HookResult<()> {
        // Remove from entries
        if self.entries.remove(name).is_none() {
            return Err(HookError::HandlerNotFound(name.to_string()));
        }
        
        // Remove from all hook type registrations
        for mut handlers in self.handlers.iter_mut() {
            handlers.retain(|h| h != name);
        }
        
        Ok(())
    }
    
    /// Execute hooks for a given type
    pub async fn execute(
        &self,
        hook_type: HookType,
        context: &HookContext,
        mut data: serde_json::Value,
    ) -> HookResult<serde_json::Value> {
        if !self.enabled {
            return Ok(data);
        }
        
        // Get handlers for this hook type
        let handler_names = self
            .handlers
            .get(&hook_type)
            .map(|h| h.clone())
            .unwrap_or_default();
        
        // Execute handlers in order
        for handler_name in handler_names {
            // Create payload with current data state
            let payload = HookPayload::new(hook_type.clone(), data.clone());
            let mut entry = match self.entries.get_mut(&handler_name) {
                Some(entry) => entry,
                None => continue,
            };
            
            // Skip disabled handlers
            if !entry.enabled {
                self.lifecycle.skipped(&handler_name);
                continue;
            }
            
            // Check rate limit
            if let Some(ref mut rate_limit) = entry.rate_limit {
                if !rate_limit.check_and_update() {
                    return Err(HookError::rate_limit_exceeded(
                        &handler_name,
                        rate_limit.max_calls,
                        rate_limit.window,
                    ));
                }
            }
            
            // Check if handler should run
            if !entry.handler.should_run(context, &payload) {
                self.lifecycle.skipped(&handler_name);
                continue;
            }
            
            // Execute handler
            let start = Instant::now();
            self.lifecycle.pre_execution(&handler_name);
            self.lifecycle.executing(&handler_name);
            
            let result = match timeout(
                self.global_timeout,
                entry.handler.execute(context, &payload),
            )
            .await
            {
                Ok(Ok(result)) => {
                    let duration = start.elapsed();
                    entry.stats.record_success(duration);
                    self.lifecycle.post_execution(&handler_name);
                    self.record_history(hook_type.clone(), handler_name.clone(), duration, "success");
                    result
                }
                Ok(Err(e)) => {
                    let duration = start.elapsed();
                    entry.stats.record_failure(duration);
                    self.lifecycle.failed(&handler_name, e.to_string());
                    self.record_history(hook_type.clone(), handler_name.clone(), duration, "error");
                    return Err(e);
                }
                Err(_) => {
                    let duration = start.elapsed();
                    entry.stats.record_failure(duration);
                    let error = HookError::timeout(&handler_name, self.global_timeout);
                    self.lifecycle.failed(&handler_name, error.to_string());
                    self.record_history(hook_type.clone(), handler_name.clone(), duration, "timeout");
                    return Err(error);
                }
            };
            
            // Handle execution result
            match result {
                ExecutionResult::Continue => continue,
                ExecutionResult::Stop(return_data) => {
                    return Ok(return_data.unwrap_or(data));
                }
                ExecutionResult::Replace(new_data) => {
                    data = new_data;
                }
                ExecutionResult::Retry { delay: _, max_attempts: _ } => {
                    // TODO: Implement retry logic
                    continue;
                }
                ExecutionResult::Error { message, .. } => {
                    return Err(HookError::execution_failed(&handler_name, message));
                }
            }
        }
        
        Ok(data)
    }
    
    /// Enable or disable a specific handler
    pub fn set_handler_enabled(&self, name: &str, enabled: bool) -> HookResult<()> {
        self.entries
            .get_mut(name)
            .map(|mut entry| {
                entry.enabled = enabled;
            })
            .ok_or_else(|| HookError::HandlerNotFound(name.to_string()))
    }
    
    /// Set rate limit for a handler
    pub fn set_rate_limit(
        &self,
        name: &str,
        max_calls: u32,
        window: Duration,
    ) -> HookResult<()> {
        self.entries
            .get_mut(name)
            .map(|mut entry| {
                entry.rate_limit = Some(RateLimit::new(max_calls, window));
            })
            .ok_or_else(|| HookError::HandlerNotFound(name.to_string()))
    }
    
    /// Get statistics for a handler
    pub fn get_stats(&self, name: &str) -> Option<HookStats> {
        self.entries.get(name).map(|entry| entry.stats.clone())
    }
    
    /// Get all registered handlers
    pub fn list_handlers(&self) -> Vec<(String, Vec<HookType>, HookPriority, bool)> {
        let mut result = Vec::new();
        
        for entry in self.entries.iter() {
            let name = entry.key().clone();
            let priority = entry.priority;
            let enabled = entry.enabled;
            
            // Find which hook types this handler is registered for
            let mut hook_types = Vec::new();
            for handlers_entry in self.handlers.iter() {
                if handlers_entry.value().contains(&name) {
                    hook_types.push(handlers_entry.key().clone());
                }
            }
            
            result.push((name, hook_types, priority, enabled));
        }
        
        result
    }
    
    /// Set global timeout
    pub fn set_global_timeout(&mut self, timeout: Duration) {
        self.global_timeout = timeout;
    }
    
    /// Enable or disable all hooks
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Get the lifecycle manager
    pub fn lifecycle(&self) -> Arc<HookLifecycle> {
        self.lifecycle.clone()
    }
    
    fn record_history(&self, hook_type: HookType, handler: String, duration: Duration, result: &str) {
        let history_entry = ExecutionHistory {
            timestamp: Instant::now(),
            hook_type,
            handler,
            duration,
            result: result.to_string(),
        };
        
        tokio::spawn({
            let history = self.history.clone();
            let max_history = self.max_history;
            async move {
                let mut hist = history.lock().await;
                hist.push(history_entry);
                
                // Trim history if too large
                if hist.len() > max_history {
                    let drain_count = hist.len() - max_history;
                    hist.drain(0..drain_count);
                }
            }
        });
    }
    
    /// Get execution history
    pub async fn get_history(&self, limit: Option<usize>) -> Vec<(String, Duration, String)> {
        let history = self.history.lock().await;
        let limit = limit.unwrap_or(history.len());
        
        history
            .iter()
            .rev()
            .take(limit)
            .map(|h| (h.handler.clone(), h.duration, h.result.clone()))
            .collect()
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::AsyncHookHandler;
    use async_trait::async_trait;
    use serde_json::json;
    
    struct TestHandler {
        name: String,
        result: ExecutionResult,
    }
    
    #[async_trait]
    impl AsyncHookHandler for TestHandler {
        async fn execute(&self, _context: &HookContext, _payload: &HookPayload) -> HookResult<ExecutionResult> {
            Ok(self.result.clone())
        }
        
        fn name(&self) -> &str {
            &self.name
        }
    }
    
    #[tokio::test]
    async fn test_hook_registration() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::Continue,
        };
        
        manager
            .register(
                "test",
                vec![HookType::ServerStartup],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        let handlers = manager.list_handlers();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].0, "test");
        assert_eq!(handlers[0].1, vec![HookType::ServerStartup]);
    }
    
    #[tokio::test]
    async fn test_hook_execution() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::Replace(json!({"modified": true})),
        };
        
        manager
            .register(
                "test",
                vec![HookType::RequestReceived],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        let context = HookContext::new();
        let result = manager
            .execute(HookType::RequestReceived, &context, json!({"original": true}))
            .await
            .unwrap();
        
        assert_eq!(result, json!({"modified": true}));
    }
    
    #[tokio::test]
    async fn test_hook_priority() {
        let manager = HookManager::new();
        
        // Register handlers with different priorities
        let handler1 = TestHandler {
            name: "high".to_string(),
            result: ExecutionResult::Continue,
        };
        
        let handler2 = TestHandler {
            name: "low".to_string(),
            result: ExecutionResult::stop_with_data(json!({"stopped": "by_low"})),
        };
        
        manager
            .register(
                "low",
                vec![HookType::ToolPreExecution],
                handler2,
                HookPriority::LOW,
            )
            .unwrap();
        
        manager
            .register(
                "high",
                vec![HookType::ToolPreExecution],
                handler1,
                HookPriority::HIGH,
            )
            .unwrap();
        
        // High priority handler should execute first and continue
        // Low priority handler should then stop execution
        let context = HookContext::new();
        let result = manager
            .execute(HookType::ToolPreExecution, &context, json!({}))
            .await
            .unwrap();
        
        assert_eq!(result, json!({"stopped": "by_low"}));
    }
    
    #[tokio::test]
    async fn test_handler_enable_disable() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::Replace(json!({"executed": true})),
        };
        
        manager
            .register(
                "test",
                vec![HookType::RequestProcessed],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        // Handler is enabled by default
        let context = HookContext::new();
        let result = manager
            .execute(HookType::RequestProcessed, &context, json!({}))
            .await
            .unwrap();
        assert_eq!(result, json!({"executed": true}));
        
        // Disable handler
        manager.set_handler_enabled("test", false).unwrap();
        
        // Should not execute
        let result = manager
            .execute(HookType::RequestProcessed, &context, json!({"original": true}))
            .await
            .unwrap();
        assert_eq!(result, json!({"original": true}));
        
        // Re-enable handler
        manager.set_handler_enabled("test", true).unwrap();
        
        // Should execute again
        let result = manager
            .execute(HookType::RequestProcessed, &context, json!({}))
            .await
            .unwrap();
        assert_eq!(result, json!({"executed": true}));
    }
    
    #[tokio::test]
    async fn test_handler_unregister() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::Continue,
        };
        
        manager
            .register(
                "test",
                vec![HookType::ServerShutdown],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        assert_eq!(manager.list_handlers().len(), 1);
        
        // Unregister handler
        manager.unregister("test").unwrap();
        
        assert_eq!(manager.list_handlers().len(), 0);
        
        // Should error on duplicate unregister
        assert!(manager.unregister("test").is_err());
    }
    
    #[tokio::test]
    async fn test_multiple_handlers_same_hook() {
        let manager = HookManager::new();
        
        let handler1 = TestHandler {
            name: "handler1".to_string(),
            result: ExecutionResult::Replace(json!({"step": 1})),
        };
        
        let handler2 = TestHandler {
            name: "handler2".to_string(),
            result: ExecutionResult::Replace(json!({"step": 2})),
        };
        
        manager
            .register(
                "handler1",
                vec![HookType::TclPreExecution],
                handler1,
                HookPriority(100),
            )
            .unwrap();
        
        manager
            .register(
                "handler2",
                vec![HookType::TclPreExecution],
                handler2,
                HookPriority(200),
            )
            .unwrap();
        
        let context = HookContext::new();
        let result = manager
            .execute(HookType::TclPreExecution, &context, json!({"original": true}))
            .await
            .unwrap();
        
        // Second handler should win as it executes after first
        assert_eq!(result, json!({"step": 2}));
    }
    
    #[tokio::test]
    async fn test_handler_error_propagation() {
        struct ErrorHandler;
        
        #[async_trait]
        impl AsyncHookHandler for ErrorHandler {
            async fn execute(&self, _context: &HookContext, _payload: &HookPayload) -> HookResult<ExecutionResult> {
                Err(HookError::execution_failed("error_handler", "Test error"))
            }
            
            fn name(&self) -> &str {
                "error_handler"
            }
        }
        
        let manager = HookManager::new();
        manager
            .register(
                "error",
                vec![HookType::TclError],
                ErrorHandler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        let context = HookContext::new();
        let result = manager
            .execute(HookType::TclError, &context, json!({}))
            .await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Test error"));
    }
    
    #[tokio::test]
    async fn test_handler_stats() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "stats_test".to_string(),
            result: ExecutionResult::Continue,
        };
        
        manager
            .register(
                "stats_test",
                vec![HookType::RequestReceived],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        let context = HookContext::new();
        
        // Execute handler multiple times
        for _ in 0..3 {
            let _ = manager
                .execute(HookType::RequestReceived, &context, json!({}))
                .await;
        }
        
        // Check stats
        let stats = manager.get_stats("stats_test").unwrap();
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.successful_executions, 3);
        assert_eq!(stats.failed_executions, 0);
        assert!(stats.average_duration.is_some());
    }
    
    #[tokio::test]
    async fn test_execution_history() {
        let manager = HookManager::new();
        
        let handler = TestHandler {
            name: "history_test".to_string(),
            result: ExecutionResult::Continue,
        };
        
        manager
            .register(
                "history_test",
                vec![HookType::ServerInitialized],
                handler,
                HookPriority::NORMAL,
            )
            .unwrap();
        
        let context = HookContext::new();
        
        // Execute handler
        let _ = manager
            .execute(HookType::ServerInitialized, &context, json!({}))
            .await;
        
        // Wait a bit for history to be recorded
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Check history
        let history = manager.get_history(Some(10)).await;
        assert!(!history.is_empty());
        assert_eq!(history[0].0, "history_test");
        assert_eq!(history[0].2, "success");
    }
}