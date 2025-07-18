//! Hook handler traits and implementations

use crate::hooks::{HookContext, HookPayload, ExecutionResult, HookError, HookResult};
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for synchronous hook handlers
pub trait HookHandler: Send + Sync {
    /// Execute the hook handler
    fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult>;
    
    /// Get the handler name
    fn name(&self) -> &str;
    
    /// Check if the handler should run for the given context
    fn should_run(&self, _context: &HookContext, _payload: &HookPayload) -> bool {
        true
    }
}

/// Trait for asynchronous hook handlers
#[async_trait]
pub trait AsyncHookHandler: Send + Sync {
    /// Execute the hook handler asynchronously
    async fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult>;
    
    /// Get the handler name
    fn name(&self) -> &str;
    
    /// Check if the handler should run for the given context
    fn should_run(&self, _context: &HookContext, _payload: &HookPayload) -> bool {
        true
    }
}

/// Wrapper to use sync handlers as async
pub struct SyncToAsyncHandler<H: HookHandler> {
    inner: Arc<H>,
}

impl<H: HookHandler> SyncToAsyncHandler<H> {
    pub fn new(handler: H) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }
}

#[async_trait]
impl<H: HookHandler + 'static> AsyncHookHandler for SyncToAsyncHandler<H> {
    async fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult> {
        let handler = self.inner.clone();
        let handler_name = handler.name().to_string();
        let context = context.clone();
        let payload = payload.clone();
        
        tokio::task::spawn_blocking(move || handler.execute(&context, &payload))
            .await
            .map_err(|e| HookError::execution_failed(handler_name, e))?
    }
    
    fn name(&self) -> &str {
        self.inner.name()
    }
    
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        self.inner.should_run(context, payload)
    }
}

/// A handler that logs hook execution
pub struct LoggingHandler {
    name: String,
    log_level: tracing::Level,
}

impl LoggingHandler {
    pub fn new(name: impl Into<String>, log_level: tracing::Level) -> Self {
        Self {
            name: name.into(),
            log_level,
        }
    }
}

impl HookHandler for LoggingHandler {
    fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult> {
        let user_id = context.user_id().unwrap_or("anonymous");
        
        match self.log_level {
            tracing::Level::TRACE => tracing::trace!(
                hook_type = %payload.hook_type,
                user_id = %user_id,
                execution_id = %payload.execution_id,
                "Hook executed"
            ),
            tracing::Level::DEBUG => tracing::debug!(
                hook_type = %payload.hook_type,
                user_id = %user_id,
                execution_id = %payload.execution_id,
                "Hook executed"
            ),
            tracing::Level::INFO => tracing::info!(
                hook_type = %payload.hook_type,
                user_id = %user_id,
                execution_id = %payload.execution_id,
                "Hook executed"
            ),
            tracing::Level::WARN => tracing::warn!(
                hook_type = %payload.hook_type,
                user_id = %user_id,
                execution_id = %payload.execution_id,
                "Hook executed"
            ),
            tracing::Level::ERROR => tracing::error!(
                hook_type = %payload.hook_type,
                user_id = %user_id,
                execution_id = %payload.execution_id,
                "Hook executed"
            ),
        }
        
        Ok(ExecutionResult::Continue)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookType;
    use serde_json::json;
    
    struct TestHandler {
        name: String,
        result: ExecutionResult,
    }
    
    impl HookHandler for TestHandler {
        fn execute(&self, _context: &HookContext, _payload: &HookPayload) -> HookResult<ExecutionResult> {
            Ok(self.result.clone())
        }
        
        fn name(&self) -> &str {
            &self.name
        }
    }
    
    #[test]
    fn test_sync_handler() {
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::Continue,
        };
        
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ServerStartup, json!({}));
        
        let result = handler.execute(&context, &payload).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
    
    #[tokio::test]
    async fn test_sync_to_async() {
        let handler = TestHandler {
            name: "test".to_string(),
            result: ExecutionResult::stop_with_data(json!({"stopped": true})),
        };
        
        let async_handler = SyncToAsyncHandler::new(handler);
        
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ServerStartup, json!({}));
        
        let result = async_handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Stop(Some(_))));
    }
}