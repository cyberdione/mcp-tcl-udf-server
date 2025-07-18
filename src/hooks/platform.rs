//! Platform-specific directory handling for hooks

use std::path::PathBuf;
use std::fs;
use std::io::Result;

/// Platform-specific directory resolver
pub struct PlatformDirs;

impl PlatformDirs {
    /// Get the hooks configuration directory
    pub fn config_dir() -> Result<PathBuf> {
        let base_dir = if cfg!(target_os = "linux") {
            // Linux: ~/.local/share/tcl-mcp-server/hooks/
            dirs::data_local_dir()
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find local data directory"
                ))?
                .join("tcl-mcp-server")
                .join("hooks")
        } else if cfg!(target_os = "macos") {
            // macOS: ~/Library/Application Support/tcl-mcp-server/hooks/
            dirs::data_dir()
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find application support directory"
                ))?
                .join("tcl-mcp-server")
                .join("hooks")
        } else if cfg!(target_os = "windows") {
            // Windows: %APPDATA%\tcl-mcp-server\hooks\
            dirs::data_dir()
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find app data directory"
                ))?
                .join("tcl-mcp-server")
                .join("hooks")
        } else {
            // Fallback to home directory
            dirs::home_dir()
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find home directory"
                ))?
                .join(".tcl-mcp-server")
                .join("hooks")
        };
        
        // Create directory if it doesn't exist
        fs::create_dir_all(&base_dir)?;
        Ok(base_dir)
    }
    
    /// Get the hooks configuration file path
    pub fn config_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("hooks.toml"))
    }
    
    /// Get the hooks scripts directory
    pub fn scripts_dir() -> Result<PathBuf> {
        let dir = Self::config_dir()?.join("scripts");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    
    /// Get the hooks logs directory
    pub fn logs_dir() -> Result<PathBuf> {
        let dir = Self::config_dir()?.join("logs");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    
    /// Get the hooks cache directory
    pub fn cache_dir() -> Result<PathBuf> {
        let dir = Self::config_dir()?.join("cache");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    
    /// Create all necessary directories
    pub fn ensure_directories() -> Result<()> {
        Self::config_dir()?;
        Self::scripts_dir()?;
        Self::logs_dir()?;
        Self::cache_dir()?;
        Ok(())
    }
    
    /// Get a backup file path for the configuration
    pub fn config_backup_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("hooks.toml.backup"))
    }
    
    /// Get a dated backup file path
    pub fn config_dated_backup_file() -> Result<PathBuf> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        Ok(Self::config_dir()?.join(format!("hooks.toml.backup.{}", timestamp)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_dirs() {
        // Just ensure methods don't panic
        let _ = PlatformDirs::config_dir();
        let _ = PlatformDirs::config_file();
        let _ = PlatformDirs::scripts_dir();
        let _ = PlatformDirs::logs_dir();
        let _ = PlatformDirs::cache_dir();
    }
}