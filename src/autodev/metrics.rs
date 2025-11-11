//! Metrics for autonomous software development

use lazy_static::lazy_static;
use prometheus::{
    register_int_counter, register_int_counter_vec, register_histogram_vec,
    IntCounter, IntCounterVec, HistogramVec,
};

lazy_static! {
    pub static ref AUTODEV_METRICS: AutodevMetrics = AutodevMetrics::new().unwrap();
}

/// Metrics for autonomous development
pub struct AutodevMetrics {
    /// Total tasks submitted
    pub tasks_total: IntCounter,
    
    /// Successful tasks
    pub tasks_success: IntCounter,
    
    /// Failed tasks
    pub tasks_failed: IntCounter,
    
    /// Cancelled tasks
    pub tasks_cancelled: IntCounter,
    
    /// Steps executed by status
    pub steps_total: IntCounterVec,
    
    /// Step execution duration by tool
    pub step_duration: HistogramVec,
    
    /// Task execution duration
    pub task_duration: prometheus::Histogram,
    
    /// PRs opened
    pub prs_opened: IntCounter,
    
    /// PRs merged
    pub merges_total: IntCounter,
    
    /// PRs reverted
    pub reverts_total: IntCounter,
    
    /// Policy denials
    pub policy_denials: IntCounterVec,
}

impl AutodevMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        Ok(Self {
            tasks_total: register_int_counter!(
                "autodev_tasks_total",
                "Total number of autonomous development tasks submitted"
            )?,
            
            tasks_success: register_int_counter!(
                "autodev_tasks_success_total",
                "Total number of successful tasks"
            )?,
            
            tasks_failed: register_int_counter!(
                "autodev_tasks_failed_total",
                "Total number of failed tasks"
            )?,
            
            tasks_cancelled: register_int_counter!(
                "autodev_tasks_cancelled_total",
                "Total number of cancelled tasks"
            )?,
            
            steps_total: register_int_counter_vec!(
                "autodev_steps_total",
                "Total number of steps executed by status",
                &["status"]
            )?,
            
            step_duration: register_histogram_vec!(
                "autodev_step_duration_seconds",
                "Step execution duration in seconds",
                &["tool"]
            )?,
            
            task_duration: prometheus::register_histogram!(
                "autodev_task_duration_seconds",
                "Task execution duration in seconds"
            )?,
            
            prs_opened: register_int_counter!(
                "autodev_prs_opened_total",
                "Total number of PRs opened"
            )?,
            
            merges_total: register_int_counter!(
                "autodev_merges_total",
                "Total number of PRs merged"
            )?,
            
            reverts_total: register_int_counter!(
                "autodev_reverts_total",
                "Total number of PRs reverted"
            )?,
            
            policy_denials: register_int_counter_vec!(
                "autodev_policy_denials_total",
                "Total number of policy denials by reason",
                &["reason"]
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        let metrics = AutodevMetrics::new().unwrap();
        
        // Test that metrics can be incremented
        metrics.tasks_total.inc();
        assert!(metrics.tasks_total.get() > 0);
    }
}