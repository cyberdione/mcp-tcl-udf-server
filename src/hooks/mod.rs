//! TCL MCP Server Hooks System
//! 
//! This module provides a comprehensive hooks system for the TCL MCP Server,
//! enabling extensible application behavior through configurable hook handlers.

pub mod config;
pub mod context;
pub mod errors;
pub mod handler;
pub mod lifecycle;
pub mod manager;
pub mod platform;
pub mod security;
pub mod traits;
pub mod types;
pub mod watcher;

// Re-export commonly used types
pub use self::config::{HooksConfig, HandlerConfig, SystemConfig};
pub use self::context::{HookContext, HookContextBuilder};
pub use self::errors::{HookError, HookResult};
pub use self::handler::{HookHandler, AsyncHookHandler};
pub use self::lifecycle::{HookLifecycle, HookPhase};
pub use self::manager::HookManager;
pub use self::platform::PlatformDirs;
pub use self::types::{HookType, HookPayload, HookPriority, ExecutionResult};

/// Prelude for convenient imports
pub mod prelude {
    pub use super::{
        HookType, HookPayload, HookContext, HookContextBuilder,
        HookHandler, AsyncHookHandler, HookManager,
        HookError, HookResult, ExecutionResult,
        HookPriority, HookPhase, HookLifecycle,
    };
}