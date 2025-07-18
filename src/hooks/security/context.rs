//! Security context for hook execution

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Security context for hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSecurityContext {
    /// Principal identity
    pub principal: Principal,
    
    /// Granted permissions
    pub permissions: Vec<String>,
    
    /// Security metadata
    pub metadata: HashMap<String, serde_json::Value>,
    
    /// Context creation time
    pub created_at: DateTime<Utc>,
    
    /// Context expiration time
    pub expires_at: Option<DateTime<Utc>>,
}

/// Principal identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Principal {
    /// System principal
    System,
    
    /// User principal
    User {
        id: String,
        name: String,
        roles: Vec<String>,
    },
    
    /// Service principal
    Service {
        id: String,
        name: String,
    },
}

impl HookSecurityContext {
    /// Create a new security context
    pub fn new(principal: Principal) -> Self {
        Self {
            principal,
            permissions: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            expires_at: None,
        }
    }
    
    /// Add a permission
    pub fn add_permission(&mut self, permission: impl Into<String>) {
        self.permissions.push(permission.into());
    }
    
    /// Check if a permission is granted
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
    
    /// Check if context is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
}