//! TCL MCP Server Hooks System
//! 
//! This module provides a comprehensive hooks system for the TCL MCP Server,
//! enabling extensible application behavior through configurable hook handlers.

pub mod config;
pub mod context;
pub mod errors;
pub mod handler;
pub mod handlers;
pub mod lifecycle;
pub mod manager;
pub mod platform;
pub mod security;
pub mod tools;
pub mod traits;
pub mod types;
pub mod watcher;

// Re-export commonly used types
pub use self::config::{
    HooksConfig, HandlerConfig, SystemConfig, HandlerType, HandlerTypeConfig,
    TclScriptConfig, ExternalCommandConfig, BuiltInConfig,
};
pub use self::context::{HookContext, HookContextBuilder};
pub use self::errors::{HookError, HookResult};
pub use self::handler::{HookHandler, AsyncHookHandler};
pub use self::handlers::{
    TclScriptHandler, ExternalCommandHandler,
    LoggingHandler, MetricsHandler, ValidationHandler,
    TransformHandler, NotificationHandler,
};
pub use self::lifecycle::{HookLifecycle, HookPhase};
pub use self::manager::HookManager;
pub use self::platform::PlatformDirs;
pub use self::tools::{
    HookAddRequest, HookRemoveRequest, HookListRequest, HookEnableRequest,
    HookDisableRequest, HookUpdateRequest, HookInfoRequest, HookTestRequest,
    HookSystemStatusRequest, HookSystemEnableRequest, HookSystemDisableRequest,
    HookConfigReloadRequest, HookConfigSaveRequest,
    handle_hook_add, handle_hook_remove, handle_hook_list, handle_hook_enable,
    handle_hook_disable, handle_hook_update, handle_hook_info, handle_hook_test,
    handle_hook_system_status, handle_hook_system_enable, handle_hook_system_disable,
    handle_hook_config_reload, handle_hook_config_save,
};
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