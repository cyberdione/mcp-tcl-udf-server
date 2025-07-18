//! File system watcher for configuration changes

use notify::{Watcher, RecursiveMode, Result as NotifyResult, Event, EventKind};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// Configuration file watcher
pub struct ConfigWatcher {
    watcher: notify::RecommendedWatcher,
    receiver: Receiver<NotifyResult<Event>>,
}

impl ConfigWatcher {
    /// Create a new configuration watcher
    pub fn new() -> NotifyResult<Self> {
        let (sender, receiver) = channel();
        
        let watcher = notify::recommended_watcher(move |res| {
            let _ = sender.send(res);
        })?;
        
        Ok(Self { watcher, receiver })
    }
    
    /// Watch a configuration file
    pub fn watch_file(&mut self, path: impl AsRef<Path>) -> NotifyResult<()> {
        self.watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)
    }
    
    /// Watch a directory
    pub fn watch_directory(&mut self, path: impl AsRef<Path>) -> NotifyResult<()> {
        self.watcher.watch(path.as_ref(), RecursiveMode::Recursive)
    }
    
    /// Stop watching a path
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> NotifyResult<()> {
        self.watcher.unwatch(path.as_ref())
    }
    
    /// Check for file system events
    pub fn check_events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        
        while let Ok(Ok(event)) = self.receiver.try_recv() {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    events.push(event);
                }
                _ => {}
            }
        }
        
        events
    }
    
    /// Check if configuration file was modified
    pub fn config_modified(&self, config_path: &Path) -> bool {
        self.check_events().iter().any(|event| {
            event.paths.iter().any(|p| p == config_path) &&
            matches!(event.kind, EventKind::Modify(_))
        })
    }
}

/// Auto-reload configuration manager
pub struct AutoReloadConfig {
    watcher: ConfigWatcher,
    config_path: std::path::PathBuf,
    last_reload: std::time::Instant,
    min_reload_interval: Duration,
}

impl AutoReloadConfig {
    /// Create a new auto-reload configuration
    pub fn new(config_path: impl Into<std::path::PathBuf>) -> NotifyResult<Self> {
        let config_path = config_path.into();
        let mut watcher = ConfigWatcher::new()?;
        watcher.watch_file(&config_path)?;
        
        Ok(Self {
            watcher,
            config_path,
            last_reload: std::time::Instant::now(),
            min_reload_interval: Duration::from_secs(1),
        })
    }
    
    /// Check if configuration should be reloaded
    pub fn should_reload(&mut self) -> bool {
        // Check minimum interval to avoid rapid reloads
        if self.last_reload.elapsed() < self.min_reload_interval {
            return false;
        }
        
        if self.watcher.config_modified(&self.config_path) {
            self.last_reload = std::time::Instant::now();
            true
        } else {
            false
        }
    }
    
    /// Set minimum reload interval
    pub fn set_min_reload_interval(&mut self, interval: Duration) {
        self.min_reload_interval = interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_config_watcher() -> NotifyResult<()> {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("test.toml");
        
        // Create initial file
        fs::write(&config_file, "test = true").unwrap();
        
        let mut watcher = ConfigWatcher::new()?;
        watcher.watch_file(&config_file)?;
        
        // Give watcher time to register
        std::thread::sleep(Duration::from_millis(100));
        
        // Modify file
        fs::write(&config_file, "test = false").unwrap();
        
        // Give watcher time to detect change
        std::thread::sleep(Duration::from_millis(100));
        
        let events = watcher.check_events();
        assert!(!events.is_empty());
        
        Ok(())
    }
}