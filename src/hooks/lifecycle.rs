//! Hook lifecycle management

use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Hook execution phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookPhase {
    /// Before handler execution
    PreExecution,
    /// During handler execution
    Executing,
    /// After successful execution
    PostExecution,
    /// After failed execution
    Failed,
    /// Execution was skipped
    Skipped,
}

/// Hook lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookLifecycleEvent {
    /// Handler name
    pub handler: String,
    /// Current phase
    pub phase: HookPhase,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional error message
    pub error: Option<String>,
    /// Execution duration (if completed)
    pub duration: Option<std::time::Duration>,
}

/// Hook lifecycle observer trait
pub trait LifecycleObserver: Send + Sync {
    /// Called when a lifecycle event occurs
    fn on_event(&self, event: &HookLifecycleEvent);
}

/// Hook lifecycle manager
pub struct HookLifecycle {
    observers: Arc<RwLock<Vec<Arc<dyn LifecycleObserver>>>>,
    active_executions: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl HookLifecycle {
    /// Create a new lifecycle manager
    pub fn new() -> Self {
        Self {
            observers: Arc::new(RwLock::new(Vec::new())),
            active_executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register an observer
    pub fn register_observer(&self, observer: Arc<dyn LifecycleObserver>) -> Result<(), String> {
        self.observers
            .write()
            .map_err(|_| "Failed to acquire write lock")?
            .push(observer);
        Ok(())
    }
    
    /// Notify pre-execution
    pub fn pre_execution(&self, handler: &str) {
        let event = HookLifecycleEvent {
            handler: handler.to_string(),
            phase: HookPhase::PreExecution,
            timestamp: Utc::now(),
            error: None,
            duration: None,
        };
        
        self.notify_observers(&event);
        
        if let Ok(mut active) = self.active_executions.write() {
            active.insert(handler.to_string(), event.timestamp);
        }
    }
    
    /// Notify execution started
    pub fn executing(&self, handler: &str) {
        let event = HookLifecycleEvent {
            handler: handler.to_string(),
            phase: HookPhase::Executing,
            timestamp: Utc::now(),
            error: None,
            duration: None,
        };
        
        self.notify_observers(&event);
    }
    
    /// Notify post-execution
    pub fn post_execution(&self, handler: &str) {
        let duration = self.calculate_duration(handler);
        
        let event = HookLifecycleEvent {
            handler: handler.to_string(),
            phase: HookPhase::PostExecution,
            timestamp: Utc::now(),
            error: None,
            duration,
        };
        
        self.notify_observers(&event);
        self.remove_active(handler);
    }
    
    /// Notify execution failed
    pub fn failed(&self, handler: &str, error: String) {
        let duration = self.calculate_duration(handler);
        
        let event = HookLifecycleEvent {
            handler: handler.to_string(),
            phase: HookPhase::Failed,
            timestamp: Utc::now(),
            error: Some(error),
            duration,
        };
        
        self.notify_observers(&event);
        self.remove_active(handler);
    }
    
    /// Notify execution skipped
    pub fn skipped(&self, handler: &str) {
        let event = HookLifecycleEvent {
            handler: handler.to_string(),
            phase: HookPhase::Skipped,
            timestamp: Utc::now(),
            error: None,
            duration: None,
        };
        
        self.notify_observers(&event);
    }
    
    /// Get active executions
    pub fn active_executions(&self) -> Vec<(String, DateTime<Utc>)> {
        self.active_executions
            .read()
            .map(|active| active.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default()
    }
    
    fn notify_observers(&self, event: &HookLifecycleEvent) {
        if let Ok(observers) = self.observers.read() {
            for observer in observers.iter() {
                observer.on_event(event);
            }
        }
    }
    
    fn calculate_duration(&self, handler: &str) -> Option<std::time::Duration> {
        if let Ok(active) = self.active_executions.read() {
            if let Some(start_time) = active.get(handler) {
                let duration = Utc::now().signed_duration_since(*start_time);
                return duration.to_std().ok();
            }
        }
        None
    }
    
    fn remove_active(&self, handler: &str) {
        if let Ok(mut active) = self.active_executions.write() {
            active.remove(handler);
        }
    }
}

impl Default for HookLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple logging observer
pub struct LoggingObserver {
    log_level: tracing::Level,
}

impl LoggingObserver {
    pub fn new(log_level: tracing::Level) -> Self {
        Self { log_level }
    }
}

impl LifecycleObserver for LoggingObserver {
    fn on_event(&self, event: &HookLifecycleEvent) {
        let message = match event.phase {
            HookPhase::PreExecution => format!("Hook handler '{}' starting", event.handler),
            HookPhase::Executing => format!("Hook handler '{}' executing", event.handler),
            HookPhase::PostExecution => {
                if let Some(duration) = event.duration {
                    format!("Hook handler '{}' completed in {:?}", event.handler, duration)
                } else {
                    format!("Hook handler '{}' completed", event.handler)
                }
            }
            HookPhase::Failed => {
                if let Some(error) = &event.error {
                    format!("Hook handler '{}' failed: {}", event.handler, error)
                } else {
                    format!("Hook handler '{}' failed", event.handler)
                }
            }
            HookPhase::Skipped => format!("Hook handler '{}' skipped", event.handler),
        };
        
        match self.log_level {
            tracing::Level::TRACE => tracing::trace!("{}", message),
            tracing::Level::DEBUG => tracing::debug!("{}", message),
            tracing::Level::INFO => tracing::info!("{}", message),
            tracing::Level::WARN => tracing::warn!("{}", message),
            tracing::Level::ERROR => tracing::error!("{}", message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    
    struct TestObserver {
        events: Arc<Mutex<Vec<HookLifecycleEvent>>>,
    }
    
    impl LifecycleObserver for TestObserver {
        fn on_event(&self, event: &HookLifecycleEvent) {
            if let Ok(mut events) = self.events.lock() {
                events.push(event.clone());
            }
        }
    }
    
    #[test]
    fn test_lifecycle_flow() {
        let lifecycle = HookLifecycle::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let observer = Arc::new(TestObserver {
            events: events.clone(),
        });
        
        lifecycle.register_observer(observer).unwrap();
        
        // Simulate successful execution
        lifecycle.pre_execution("test_handler");
        lifecycle.executing("test_handler");
        std::thread::sleep(std::time::Duration::from_millis(10));
        lifecycle.post_execution("test_handler");
        
        let collected_events = events.lock().unwrap();
        assert_eq!(collected_events.len(), 3);
        assert_eq!(collected_events[0].phase, HookPhase::PreExecution);
        assert_eq!(collected_events[1].phase, HookPhase::Executing);
        assert_eq!(collected_events[2].phase, HookPhase::PostExecution);
        assert!(collected_events[2].duration.is_some());
    }
    
    #[test]
    fn test_failed_execution() {
        let lifecycle = HookLifecycle::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let observer = Arc::new(TestObserver {
            events: events.clone(),
        });
        
        lifecycle.register_observer(observer).unwrap();
        
        // Simulate failed execution
        lifecycle.pre_execution("failing_handler");
        lifecycle.failed("failing_handler", "Test error".to_string());
        
        let collected_events = events.lock().unwrap();
        assert_eq!(collected_events.len(), 2);
        assert_eq!(collected_events[1].phase, HookPhase::Failed);
        assert_eq!(collected_events[1].error, Some("Test error".to_string()));
    }
    
    #[test]
    fn test_active_executions() {
        let lifecycle = HookLifecycle::new();
        
        lifecycle.pre_execution("handler1");
        lifecycle.pre_execution("handler2");
        
        let active = lifecycle.active_executions();
        assert_eq!(active.len(), 2);
        
        lifecycle.post_execution("handler1");
        
        let active = lifecycle.active_executions();
        assert_eq!(active.len(), 1);
    }
}