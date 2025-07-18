//! Additional traits for hook handlers

use crate::hooks::{HookHandler, AsyncHookHandler, HookContext, HookPayload, ExecutionResult, HookResult};
use async_trait::async_trait;
use std::sync::Arc;

/// Trait for handlers that can be chained
pub trait ChainableHandler: HookHandler {
    /// Chain this handler with another
    fn chain<H: HookHandler + 'static>(self, next: H) -> ChainedHandler
    where
        Self: Sized + 'static,
    {
        ChainedHandler {
            first: Arc::new(self),
            second: Arc::new(next),
        }
    }
}

impl<T: HookHandler> ChainableHandler for T {}

/// A handler that chains two handlers together
pub struct ChainedHandler {
    first: Arc<dyn HookHandler>,
    second: Arc<dyn HookHandler>,
}

impl HookHandler for ChainedHandler {
    fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult> {
        match self.first.execute(context, payload)? {
            ExecutionResult::Continue => self.second.execute(context, payload),
            result => Ok(result),
        }
    }
    
    fn name(&self) -> &str {
        "ChainedHandler"
    }
    
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        self.first.should_run(context, payload) || self.second.should_run(context, payload)
    }
}

/// Trait for handlers that can be conditionally executed
pub trait ConditionalHandler: HookHandler {
    /// Execute only if condition is met
    fn when<F>(self, condition: F) -> ConditionalWrapper<Self>
    where
        Self: Sized,
        F: Fn(&HookContext, &HookPayload) -> bool + Send + Sync + 'static,
    {
        ConditionalWrapper {
            handler: self,
            condition: Box::new(condition),
        }
    }
}

impl<T: HookHandler> ConditionalHandler for T {}

/// Wrapper for conditional execution
pub struct ConditionalWrapper<H: HookHandler> {
    handler: H,
    condition: Box<dyn Fn(&HookContext, &HookPayload) -> bool + Send + Sync>,
}

impl<H: HookHandler> HookHandler for ConditionalWrapper<H> {
    fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult> {
        if (self.condition)(context, payload) {
            self.handler.execute(context, payload)
        } else {
            Ok(ExecutionResult::Continue)
        }
    }
    
    fn name(&self) -> &str {
        self.handler.name()
    }
    
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        (self.condition)(context, payload) && self.handler.should_run(context, payload)
    }
}

/// Trait for async handlers that can be chained
#[async_trait]
pub trait AsyncChainableHandler: AsyncHookHandler {
    /// Chain this handler with another
    fn chain<H: AsyncHookHandler + 'static>(self, next: H) -> AsyncChainedHandler
    where
        Self: Sized + 'static,
    {
        AsyncChainedHandler {
            first: Arc::new(self),
            second: Arc::new(next),
        }
    }
}

impl<T: AsyncHookHandler> AsyncChainableHandler for T {}

/// An async handler that chains two handlers together
pub struct AsyncChainedHandler {
    first: Arc<dyn AsyncHookHandler>,
    second: Arc<dyn AsyncHookHandler>,
}

#[async_trait]
impl AsyncHookHandler for AsyncChainedHandler {
    async fn execute(&self, context: &HookContext, payload: &HookPayload) -> HookResult<ExecutionResult> {
        match self.first.execute(context, payload).await? {
            ExecutionResult::Continue => self.second.execute(context, payload).await,
            result => Ok(result),
        }
    }
    
    fn name(&self) -> &str {
        "AsyncChainedHandler"
    }
    
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        self.first.should_run(context, payload) || self.second.should_run(context, payload)
    }
}

/// Factory trait for creating hook handlers
pub trait HandlerFactory: Send + Sync {
    /// Create a new handler instance
    fn create(&self) -> Box<dyn HookHandler>;
    
    /// Get the factory name
    fn name(&self) -> &str;
}

/// Factory trait for creating async hook handlers
#[async_trait]
pub trait AsyncHandlerFactory: Send + Sync {
    /// Create a new async handler instance
    async fn create(&self) -> Box<dyn AsyncHookHandler>;
    
    /// Get the factory name
    fn name(&self) -> &str;
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
    fn test_chained_handlers() {
        let handler1 = TestHandler {
            name: "handler1".to_string(),
            result: ExecutionResult::Continue,
        };
        
        let handler2 = TestHandler {
            name: "handler2".to_string(),
            result: ExecutionResult::stop_execution(),
        };
        
        let chained = handler1.chain(handler2);
        
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ServerStartup, json!({}));
        
        let result = chained.execute(&context, &payload).unwrap();
        assert!(matches!(result, ExecutionResult::Stop(None)));
    }
    
    #[test]
    fn test_conditional_handler() {
        let handler = TestHandler {
            name: "conditional".to_string(),
            result: ExecutionResult::stop_execution(),
        };
        
        let conditional = handler.when(|_ctx, payload| {
            payload.hook_type == HookType::ServerStartup
        });
        
        let context = HookContext::new();
        
        // Should execute for ServerStartup
        let payload = HookPayload::new(HookType::ServerStartup, json!({}));
        let result = conditional.execute(&context, &payload).unwrap();
        assert!(matches!(result, ExecutionResult::Stop(None)));
        
        // Should not execute for other hook types
        let payload = HookPayload::new(HookType::ServerShutdown, json!({}));
        let result = conditional.execute(&context, &payload).unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
    }
}