# Hooks System API Reference

## Core Types

### HookType

Enumeration of all available hook points in the system.

```rust
pub enum HookType {
    // Server lifecycle
    ServerStartup,
    ServerShutdown,
    ServerInitialized,
    
    // Request processing
    RequestReceived,
    RequestProcessed,
    ResponseSent,
    
    // Tool execution
    ToolPreExecution,
    ToolPostExecution,
    ToolRegistered,
    ToolRemoved,
    
    // TCL execution
    TclPreExecution,
    TclPostExecution,
    TclError,
    
    // MCP server events
    McpServerConnected,
    McpServerDisconnected,
    McpServerError,
    
    // Security
    SecurityCheck,
    AccessDenied,
    
    // Custom hooks
    Custom(String),
}
```

### HookPriority

Controls the execution order of handlers. Lower values execute first.

```rust
pub struct HookPriority(pub u16);

impl HookPriority {
    pub const HIGHEST: Self = Self(0);
    pub const HIGH: Self = Self(100);
    pub const NORMAL: Self = Self(500);
    pub const LOW: Self = Self(900);
    pub const LOWEST: Self = Self(1000);
}
```

### HookPayload

Data passed to hook handlers.

```rust
pub struct HookPayload {
    pub hook_type: HookType,
    pub timestamp: DateTime<Utc>,
    pub execution_id: String,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl HookPayload {
    pub fn new(hook_type: HookType, data: serde_json::Value) -> Self;
    pub fn with_metadata(self, key: String, value: serde_json::Value) -> Self;
    pub fn get_data<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error>;
}
```

### HookContext

Shared context passed to all handlers in a chain.

```rust
pub struct HookContext {
    state: HashMap<String, serde_json::Value>,
    metadata: HashMap<String, serde_json::Value>,
}

impl HookContext {
    pub fn new() -> Self;
    pub fn builder() -> HookContextBuilder;
    pub fn get_state(&self, key: &str) -> Option<&serde_json::Value>;
    pub fn set_state(&mut self, key: String, value: serde_json::Value);
    pub fn with_state(mut self, key: String, value: serde_json::Value) -> Self;
    pub fn metadata(&self) -> &HashMap<String, serde_json::Value>;
}
```

### ExecutionResult

Controls the flow of execution through the handler chain.

```rust
pub enum ExecutionResult {
    /// Continue to the next handler
    Continue,
    
    /// Stop execution and return the provided data
    Stop(Option<serde_json::Value>),
    
    /// Replace the current data and continue
    Replace(serde_json::Value),
    
    /// Retry the operation
    Retry {
        delay: Option<Duration>,
        max_attempts: Option<u32>,
    },
    
    /// An error occurred
    Error {
        message: String,
        details: Option<serde_json::Value>,
    },
}
```

## Traits

### AsyncHookHandler

The main trait that all hook handlers must implement.

```rust
#[async_trait]
pub trait AsyncHookHandler: Send + Sync {
    /// Execute the hook handler
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult>;
    
    /// Get the handler name
    fn name(&self) -> &str;
    
    /// Check if the handler should run (optional)
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        true
    }
}
```

### HookHandler (Sync)

Synchronous version of the hook handler trait.

```rust
pub trait HookHandler: Send + Sync {
    fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult>;
    
    fn name(&self) -> &str;
    
    fn should_run(&self, context: &HookContext, payload: &HookPayload) -> bool {
        true
    }
}
```

## HookManager

The central manager for all hook operations.

### Construction

```rust
impl HookManager {
    /// Create a new hook manager
    pub fn new() -> Self;
    
    /// Create with custom global timeout
    pub fn with_timeout(timeout: Duration) -> Self;
}
```

### Registration

```rust
impl HookManager {
    /// Register an async hook handler
    pub fn register<H: AsyncHookHandler + 'static>(
        &self,
        name: impl Into<String>,
        hook_types: Vec<HookType>,
        handler: H,
        priority: HookPriority,
    ) -> HookResult<()>;
    
    /// Register a sync hook handler
    pub fn register_sync<H: HookHandler + 'static>(
        &self,
        name: impl Into<String>,
        hook_types: Vec<HookType>,
        handler: H,
        priority: HookPriority,
    ) -> HookResult<()>;
    
    /// Unregister a handler
    pub fn unregister(&self, name: &str) -> HookResult<()>;
}
```

### Execution

```rust
impl HookManager {
    /// Execute hooks for a given type
    pub async fn execute(
        &self,
        hook_type: HookType,
        context: &HookContext,
        data: serde_json::Value,
    ) -> HookResult<serde_json::Value>;
}
```

### Management

```rust
impl HookManager {
    /// Enable or disable the entire hook system
    pub fn set_enabled(&self, enabled: bool);
    
    /// Check if the hook system is enabled
    pub fn is_enabled(&self) -> bool;
    
    /// Enable or disable a specific handler
    pub fn set_handler_enabled(&self, name: &str, enabled: bool) -> HookResult<()>;
    
    /// Get handler information
    pub fn get_handler_info(&self, name: &str) -> Option<HandlerInfo>;
    
    /// List all registered handlers
    pub fn list_handlers(&self) -> Vec<HandlerInfo>;
    
    /// Get execution statistics
    pub fn get_stats(&self) -> Option<HashMap<String, HookStats>>;
    
    /// Clear execution history
    pub fn clear_history(&self);
}
```

## Built-in Handlers

### LoggingHandler

Logs hook events with configurable formats and outputs.

```rust
pub struct LoggingHandler {
    // ...
}

impl LoggingHandler {
    pub fn new(name: &str, config: BuiltInConfig) -> Self;
}

// Configuration options:
// - level: "trace" | "debug" | "info" | "warn" | "error"
// - format: "json" | "compact" | "detailed"
// - output: "stdout" | "stderr" | "file"
// - file_path: String (when output = "file")
// - include_context: bool
// - include_timing: bool
```

### MetricsHandler

Collects metrics about hook executions.

```rust
pub struct MetricsHandler {
    // ...
}

impl MetricsHandler {
    pub fn new(name: &str, config: BuiltInConfig) -> Self;
}

// Configuration options:
// - metric_type: "counter" | "gauge" | "histogram" | "summary"
// - metric_name: String
// - labels: HashMap<String, String> (supports templating)
// - buckets: Vec<f64> (for histogram)
// - quantiles: Vec<f64> (for summary)
```

### ValidationHandler

Validates data against configured rules.

```rust
pub struct ValidationHandler {
    // ...
}

impl ValidationHandler {
    pub fn new(name: &str, config: BuiltInConfig) -> Self;
}

// Configuration options:
// - required_fields: Vec<String>
// - forbidden_fields: Vec<String>
// - field_types: HashMap<String, String>
// - custom_validation: HashMap<String, ValidationRule>
// - fail_on_unknown: bool
// - fail_on_missing: bool
```

### TransformHandler

Transforms data by adding, removing, or modifying fields.

```rust
pub struct TransformHandler {
    // ...
}

impl TransformHandler {
    pub fn new(name: &str, config: BuiltInConfig) -> Self;
    
    // Convenience constructors
    pub fn new_add_field(field: &str, value: serde_json::Value) -> Self;
    pub fn new_remove_field(field: &str) -> Self;
    pub fn new_rename_field(from: &str, to: &str) -> Self;
}

// Configuration options:
// - add_fields: HashMap<String, Value>
// - remove_fields: Vec<String>
// - rename_fields: HashMap<String, String>
// - transform_fields: HashMap<String, TransformRule>
// - add_timestamp: bool
// - add_uuid: bool
```

### NotificationHandler

Sends notifications to external services.

```rust
pub struct NotificationHandler {
    // ...
}

impl NotificationHandler {
    pub fn new(name: &str, config: BuiltInConfig) -> Self;
}

// Configuration options:
// - notification_type: "webhook" | "email" | "slack"
// - webhook_url: String
// - email_to: Vec<String>
// - slack_channel: String
// - template: String
// - include_payload: bool
// - rate_limit_minutes: u32
```

## TCL Script Handler

Executes TCL scripts as hook handlers.

```rust
pub struct TclScriptHandler {
    // ...
}

impl TclScriptHandler {
    pub fn new(name: &str, config: TclScriptConfig) -> Result<Self>;
}

pub struct TclScriptConfig {
    pub script_path: PathBuf,
    pub timeout: Option<Duration>,
    pub environment: HashMap<String, String>,
}
```

### TCL Script Interface

TCL scripts must implement the `hook_execute` procedure:

```tcl
proc hook_execute {context payload} {
    # context - dict containing context state
    # payload - dict containing hook payload
    
    # Process the hook...
    
    # Return one of:
    # - [list continue {}]
    # - [list stop $data]
    # - [list replace $data]
    # - [list error $message $details]
}
```

## External Command Handler

Executes external programs as hook handlers.

```rust
pub struct ExternalCommandHandler {
    // ...
}

impl ExternalCommandHandler {
    pub fn new(name: &str, config: ExternalCommandConfig) -> Self;
}

pub struct ExternalCommandConfig {
    pub command: String,
    pub args: Vec<String>,
    pub timeout: Option<Duration>,
    pub environment: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
}
```

### External Command Interface

External commands receive JSON on stdin and must output JSON on stdout:

**Input format:**
```json
{
    "context": {
        "state": { ... },
        "metadata": { ... }
    },
    "payload": {
        "hook_type": "...",
        "timestamp": "...",
        "execution_id": "...",
        "data": { ... },
        "metadata": { ... }
    }
}
```

**Output format:**
```json
{
    "action": "continue" | "stop" | "replace" | "error",
    "data": { ... },
    "error_message": "...",
    "error_details": { ... }
}
```

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Handler '{0}' not found")]
    HandlerNotFound(String),
    
    #[error("Handler '{0}' already registered")]
    HandlerAlreadyRegistered(String),
    
    #[error("Hook execution failed for handler '{0}': {1}")]
    ExecutionFailed(String, String),
    
    #[error("Handler '{0}' timed out after {1:?}")]
    Timeout(String, Duration),
    
    #[error("Rate limit exceeded for handler '{0}': {1} calls in {2:?}")]
    RateLimitExceeded(String, u32, Duration),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type HookResult<T> = Result<T, HookError>;
```

## Lifecycle Methods

The HookManager provides lifecycle tracking through internal methods:

```rust
trait HookLifecycle {
    fn pre_execution(&self, handler: &str);
    fn executing(&self, handler: &str);
    fn post_execution(&self, handler: &str);
    fn skipped(&self, handler: &str);
    fn failed(&self, handler: &str, error: String);
}
```

These are called automatically during hook execution and can be monitored through statistics.

## Statistics

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub average_duration: Option<Duration>,
    pub max_duration: Option<Duration>,
    pub last_execution: Option<DateTime<Utc>>,
}
```

## Thread Safety

All types in the hooks system are thread-safe:
- `HookManager` uses internal synchronization
- Handlers must implement `Send + Sync`
- All operations are safe to call from multiple threads

## Performance Considerations

1. **Handler Execution**: Handlers run sequentially within a priority level
2. **Timeouts**: Default timeout is 5 seconds, configurable per manager
3. **Memory**: Execution history is limited to prevent unbounded growth
4. **Async Operations**: All handlers run in an async context for efficiency