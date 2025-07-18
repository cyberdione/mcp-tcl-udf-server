//! Error handling for the hooks system

use std::fmt;
use std::error::Error;

/// Result type for hook operations
pub type HookResult<T> = Result<T, HookError>;

/// Hook system error types
#[derive(Debug)]
pub enum HookError {
    /// Hook handler not found
    HandlerNotFound(String),
    
    /// Hook execution failed
    ExecutionFailed {
        handler: String,
        source: Box<dyn Error + Send + Sync>,
    },
    
    /// Hook execution timeout
    Timeout {
        handler: String,
        duration: std::time::Duration,
    },
    
    /// Invalid configuration
    InvalidConfiguration(String),
    
    /// Security violation
    SecurityViolation(String),
    
    /// Rate limit exceeded
    RateLimitExceeded {
        handler: String,
        limit: u32,
        window: std::time::Duration,
    },
    
    /// Resource limit exceeded
    ResourceLimitExceeded(String),
    
    /// Serialization/deserialization error
    SerializationError(serde_json::Error),
    
    /// IO error
    IoError(std::io::Error),
    
    /// Handler registration failed
    RegistrationFailed(String),
    
    /// Hook system not initialized
    NotInitialized,
    
    /// Custom error
    Custom(String),
}

impl HookError {
    /// Create an execution failed error
    pub fn execution_failed(handler: impl Into<String>, source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self::ExecutionFailed {
            handler: handler.into(),
            source: source.into(),
        }
    }
    
    /// Create a timeout error
    pub fn timeout(handler: impl Into<String>, duration: std::time::Duration) -> Self {
        Self::Timeout {
            handler: handler.into(),
            duration,
        }
    }
    
    /// Create an invalid configuration error
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::InvalidConfiguration(message.into())
    }
    
    /// Create a security violation error
    pub fn security_violation(message: impl Into<String>) -> Self {
        Self::SecurityViolation(message.into())
    }
    
    /// Create a rate limit exceeded error
    pub fn rate_limit_exceeded(handler: impl Into<String>, limit: u32, window: std::time::Duration) -> Self {
        Self::RateLimitExceeded {
            handler: handler.into(),
            limit,
            window,
        }
    }
    
    /// Create a custom error
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom(message.into())
    }
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandlerNotFound(name) => write!(f, "Hook handler not found: {}", name),
            Self::ExecutionFailed { handler, source } => {
                write!(f, "Hook handler '{}' execution failed: {}", handler, source)
            }
            Self::Timeout { handler, duration } => {
                write!(f, "Hook handler '{}' timed out after {:?}", handler, duration)
            }
            Self::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            Self::SecurityViolation(msg) => write!(f, "Security violation: {}", msg),
            Self::RateLimitExceeded { handler, limit, window } => {
                write!(f, "Rate limit exceeded for handler '{}': {} calls per {:?}", handler, limit, window)
            }
            Self::ResourceLimitExceeded(msg) => write!(f, "Resource limit exceeded: {}", msg),
            Self::SerializationError(e) => write!(f, "Serialization error: {}", e),
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::RegistrationFailed(msg) => write!(f, "Handler registration failed: {}", msg),
            Self::NotInitialized => write!(f, "Hook system not initialized"),
            Self::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for HookError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ExecutionFailed { source, .. } => {
                // This is safe because we know the trait object is 'static
                Some(source.as_ref() as &(dyn Error + 'static))
            }
            Self::SerializationError(e) => Some(e),
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for HookError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerializationError(error)
    }
}

impl From<std::io::Error> for HookError {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_display() {
        let err = HookError::HandlerNotFound("test_handler".to_string());
        assert_eq!(err.to_string(), "Hook handler not found: test_handler");
        
        let err = HookError::timeout("slow_handler", std::time::Duration::from_secs(5));
        assert_eq!(err.to_string(), "Hook handler 'slow_handler' timed out after 5s");
    }
    
    #[test]
    fn test_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let hook_err: HookError = json_err.into();
        assert!(matches!(hook_err, HookError::SerializationError(_)));
    }
}