//! Metrics collection hook handler

use crate::hooks::{
    AsyncHookHandler, HookContext, HookPayload, HookResult,
    ExecutionResult, BuiltInConfig,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::Duration;

/// Metrics storage
#[derive(Debug, Clone)]
struct Metrics {
    counters: HashMap<String, u64>,
    timers: HashMap<String, Vec<Duration>>,
    gauges: HashMap<String, f64>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            counters: HashMap::new(),
            timers: HashMap::new(),
            gauges: HashMap::new(),
        }
    }
}

/// Built-in metrics handler
pub struct MetricsHandler {
    name: String,
    config: BuiltInConfig,
    metrics: Arc<Mutex<Metrics>>,
}

impl MetricsHandler {
    /// Create a new metrics handler
    pub fn new(name: impl Into<String>, config: BuiltInConfig) -> Self {
        Self {
            name: name.into(),
            config,
            metrics: Arc::new(Mutex::new(Metrics::new())),
        }
    }
    
    /// Get metric key from config or generate default
    fn get_metric_key(&self, payload: &HookPayload) -> String {
        self.config.config
            .get("metric_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("hook.{}", payload.hook_type.to_string()))
    }
    
    /// Check if we should export metrics
    fn should_export(&self) -> bool {
        self.config.config
            .get("export")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

#[async_trait]
impl AsyncHookHandler for MetricsHandler {
    async fn execute(
        &self,
        context: &HookContext,
        payload: &HookPayload,
    ) -> HookResult<ExecutionResult> {
        let metric_key = self.get_metric_key(payload);
        let metric_type = self.config.config
            .get("metric_type")
            .and_then(|v| v.as_str())
            .unwrap_or("counter");
        
        let mut metrics = self.metrics.lock().await;
        
        match metric_type {
            "counter" => {
                // Increment counter
                let counter = metrics.counters.entry(metric_key.clone()).or_insert(0);
                *counter += 1;
                
                if self.should_export() {
                    return Ok(ExecutionResult::Replace(json!({
                        "data": payload.data,
                        "metrics": {
                            "type": "counter",
                            "key": metric_key,
                            "value": *counter,
                        }
                    })));
                }
            }
            "timer" => {
                // Timer metrics would need special handling since Value doesn't support Instant
                // For now, we can try to extract timestamp from context
                if let Some(start_time_value) = context.get_state("start_time_ms") {
                    if let Some(start_ms) = start_time_value.as_u64() {
                        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
                        let duration_ms = now_ms.saturating_sub(start_ms);
                        let duration = std::time::Duration::from_millis(duration_ms);
                        
                        metrics.timers.entry(metric_key.clone())
                            .or_insert_with(Vec::new)
                            .push(duration);
                        
                        if self.should_export() {
                            let timings = &metrics.timers[&metric_key];
                            let avg_ms = if !timings.is_empty() {
                                timings.iter().map(|d| d.as_millis()).sum::<u128>() / timings.len() as u128
                            } else {
                                0
                            };
                            
                            return Ok(ExecutionResult::Replace(json!({
                                "data": payload.data,
                                "metrics": {
                                    "type": "timer",
                                    "key": metric_key,
                                    "current_ms": duration.as_millis(),
                                    "average_ms": avg_ms,
                                    "count": timings.len(),
                                }
                            })));
                        }
                    }
                } else {
                    // Log that timer metrics need start_time_ms in context
                    tracing::debug!("Timer metrics require 'start_time_ms' in context");
                }
            }
            "gauge" => {
                // Extract value from payload or context
                let value = payload.data
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        self.config.config
                            .get("value_path")
                            .and_then(|path| path.as_str())
                            .and_then(|path| {
                                // Simple path extraction (could be enhanced)
                                payload.data.pointer(path).and_then(|v| v.as_f64())
                            })
                    })
                    .unwrap_or(0.0);
                
                metrics.gauges.insert(metric_key.clone(), value);
                
                if self.should_export() {
                    return Ok(ExecutionResult::Replace(json!({
                        "data": payload.data,
                        "metrics": {
                            "type": "gauge",
                            "key": metric_key,
                            "value": value,
                        }
                    })));
                }
            }
            _ => {
                // Unknown metric type, just continue
            }
        }
        
        Ok(ExecutionResult::Continue)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

impl MetricsHandler {
    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> Value {
        let metrics = self.metrics.lock().await;
        
        json!({
            "counters": metrics.counters,
            "timers": metrics.timers.iter().map(|(k, v)| {
                let avg_ms = if !v.is_empty() {
                    v.iter().map(|d| d.as_millis()).sum::<u128>() / v.len() as u128
                } else {
                    0
                };
                (k.clone(), json!({
                    "count": v.len(),
                    "average_ms": avg_ms,
                }))
            }).collect::<HashMap<_, _>>(),
            "gauges": metrics.gauges,
        })
    }
    
    /// Reset all metrics
    pub async fn reset(&self) {
        let mut metrics = self.metrics.lock().await;
        metrics.counters.clear();
        metrics.timers.clear();
        metrics.gauges.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{HookContext, HookPayload, HookType};
    use serde_json::json;
    use std::collections::HashMap;
    
    fn config_from_json(handler_name: &str, json_config: Value) -> BuiltInConfig {
        let mut config = HashMap::new();
        if let Value::Object(map) = json_config {
            for (k, v) in map {
                config.insert(k, v);
            }
        }
        BuiltInConfig {
            handler_name: handler_name.to_string(),
            config,
        }
    }
    
    #[tokio::test]
    async fn test_counter_metrics() {
        let config = config_from_json("metrics", json!({
            "metric_type": "counter",
            "metric_key": "test_counter",
        }));
        
        let handler = MetricsHandler::new("counter_test", config);
        let context = HookContext::new();
        
        // Execute multiple times
        for i in 0..3 {
            let payload = HookPayload::new(HookType::RequestReceived, json!({"count": i}));
            let result = handler.execute(&context, &payload).await.unwrap();
            assert!(matches!(result, ExecutionResult::Continue));
        }
        
        // Check metrics
        let metrics = handler.get_metrics().await;
        assert_eq!(metrics["counters"]["test_counter"], 3);
    }
    
    #[tokio::test]
    async fn test_gauge_metrics() {
        let config = config_from_json("metrics", json!({
            "metric_type": "gauge",
            "metric_key": "test_gauge",
        }));
        
        let handler = MetricsHandler::new("gauge_test", config);
        let context = HookContext::new();
        
        // Set different gauge values
        let values = vec![10.5, 20.3, 15.7];
        for value in values {
            let payload = HookPayload::new(
                HookType::RequestProcessed,
                json!({"value": value})
            );
            let result = handler.execute(&context, &payload).await.unwrap();
            assert!(matches!(result, ExecutionResult::Continue));
        }
        
        // Check metrics - should have last value
        let metrics = handler.get_metrics().await;
        assert_eq!(metrics["gauges"]["test_gauge"], 15.7);
    }
    
    #[tokio::test]
    async fn test_timer_metrics() {
        let config = config_from_json("metrics", json!({
            "metric_type": "timer",
            "metric_key": "test_timer",
        }));
        
        let handler = MetricsHandler::new("timer_test", config);
        
        // Add start time to context
        let start_ms = chrono::Utc::now().timestamp_millis() as u64;
        let context = HookContext::builder()
            .with_state("start_time_ms".to_string(), json!(start_ms))
            .build();
        
        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let payload = HookPayload::new(HookType::ToolPostExecution, json!({}));
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        // Check metrics
        let metrics = handler.get_metrics().await;
        let timers = metrics["timers"].as_object().unwrap();
        assert!(timers.contains_key("test_timer"));
        assert_eq!(timers["test_timer"]["count"], 1);
    }
    
    #[tokio::test]
    async fn test_export_metrics() {
        let config = config_from_json("metrics", json!({
            "metric_type": "counter",
            "metric_key": "export_test",
            "export": true,
        }));
        
        let handler = MetricsHandler::new("export_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(HookType::ServerInitialized, json!({}));
        
        let result = handler.execute(&context, &payload).await.unwrap();
        
        match result {
            ExecutionResult::Replace(data) => {
                assert!(data.get("metrics").is_some());
                let metrics = &data["metrics"];
                assert_eq!(metrics["type"], "counter");
                assert_eq!(metrics["key"], "export_test");
                assert_eq!(metrics["value"], 1);
            }
            _ => panic!("Expected Replace result with exported metrics"),
        }
    }
    
    #[tokio::test]
    async fn test_gauge_with_value_path() {
        let config = config_from_json("metrics", json!({
            "metric_type": "gauge",
            "metric_key": "nested_gauge",
            "value_path": "/nested/value",
        }));
        
        let handler = MetricsHandler::new("path_test", config);
        let context = HookContext::new();
        let payload = HookPayload::new(
            HookType::RequestReceived,
            json!({
                "nested": {
                    "value": 42.5
                }
            })
        );
        
        let result = handler.execute(&context, &payload).await.unwrap();
        assert!(matches!(result, ExecutionResult::Continue));
        
        let metrics = handler.get_metrics().await;
        assert_eq!(metrics["gauges"]["nested_gauge"], 42.5);
    }
    
    #[tokio::test]
    async fn test_reset_metrics() {
        let config = config_from_json("metrics", json!({
            "metric_type": "counter",
            "metric_key": "reset_test",
        }));
        
        let handler = MetricsHandler::new("reset_test", config);
        let context = HookContext::new();
        
        // Add some metrics
        for _ in 0..5 {
            let payload = HookPayload::new(HookType::TclPreExecution, json!({}));
            let _ = handler.execute(&context, &payload).await;
        }
        
        // Verify metrics exist
        let metrics = handler.get_metrics().await;
        assert_eq!(metrics["counters"]["reset_test"], 5);
        
        // Reset
        handler.reset().await;
        
        // Verify metrics cleared
        let metrics = handler.get_metrics().await;
        assert!(metrics["counters"].as_object().unwrap().is_empty());
    }
}