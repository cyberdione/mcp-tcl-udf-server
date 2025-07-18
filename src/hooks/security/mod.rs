//! Security module for hooks system

pub mod context;
pub mod limits;
pub mod sandbox;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Require signed handlers
    pub require_signed_handlers: bool,
    
    /// Enable sandboxing
    pub sandbox_handlers: bool,
    
    /// Allowed namespaces
    pub allowed_namespaces: Vec<String>,
    
    /// Permission model
    pub permission_model: PermissionModel,
    
    /// Resource limits
    pub resource_limits: limits::ResourceLimits,
}

/// Permission model for hook handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionModel {
    /// Allow all operations
    AllowAll,
    
    /// Deny all operations
    DenyAll,
    
    /// Allow specific operations
    AllowList(Vec<String>),
    
    /// Deny specific operations
    DenyList(Vec<String>),
    
    /// Role-based access control
    RoleBased(HashMap<String, Vec<String>>),
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            require_signed_handlers: false,
            sandbox_handlers: true,
            allowed_namespaces: vec!["system".to_string(), "user".to_string()],
            permission_model: PermissionModel::AllowAll,
            resource_limits: limits::ResourceLimits::default(),
        }
    }
}