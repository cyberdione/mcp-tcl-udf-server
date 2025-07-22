//! Hook handler implementations
//!
//! This module provides concrete implementations of hook handlers for:
//! - TCL script execution
//! - External command execution
//! - Built-in handlers

pub mod tcl_handler;
pub mod external_handler;
pub mod builtin;

pub use self::tcl_handler::TclScriptHandler;
pub use self::external_handler::ExternalCommandHandler;
// Re-export all built-in handlers
pub use self::builtin::*;