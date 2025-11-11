//! Circuit breaker for upstream service protection

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker for a single operation
#[derive(Debug, Clone)]
struct BreakerEntry {
    state: BreakerState,
    failure_count: usize,
    last_failure: Option<Instant>,
    opened_at: Option<Instant>,
}

impl BreakerEntry {
    fn new() -> Self {
        Self {
            state: BreakerState::Closed,
            failure_count: 0,
            last_failure: None,
            opened_at: None,
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub reset_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
        }
    }
}

/// Circuit breaker for protecting upstream services
pub struct CircuitBreaker {
    breakers: Arc<Mutex<HashMap<String, BreakerEntry>>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Check if the circuit is open for an operation
    pub fn is_open(&self, operation: &str) -> bool {
        let mut breakers = self.breakers.lock().unwrap();
        let entry = breakers.entry(operation.to_string()).or_insert_with(BreakerEntry::new);

        match entry.state {
            BreakerState::Closed => false,
            BreakerState::Open => {
                // Check if we should transition to half-open
                if let Some(opened_at) = entry.opened_at {
                    if opened_at.elapsed() >= self.config.reset_timeout {
                        entry.state = BreakerState::HalfOpen;
                        false
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
            BreakerState::HalfOpen => false,
        }
    }

    /// Mark a successful operation
    pub fn mark_success(&self, operation: &str) {
        let mut breakers = self.breakers.lock().unwrap();
        let entry = breakers.entry(operation.to_string()).or_insert_with(BreakerEntry::new);

        // Reset on success
        entry.state = BreakerState::Closed;
        entry.failure_count = 0;
        entry.last_failure = None;
        entry.opened_at = None;
    }

    /// Mark a failed operation
    pub fn mark_failure(&self, operation: &str) {
        let mut breakers = self.breakers.lock().unwrap();
        let entry = breakers.entry(operation.to_string()).or_insert_with(BreakerEntry::new);

        entry.failure_count += 1;
        entry.last_failure = Some(Instant::now());

        // Open circuit if threshold exceeded
        if entry.failure_count >= self.config.failure_threshold {
            entry.state = BreakerState::Open;
            entry.opened_at = Some(Instant::now());
        }
    }

    /// Get the current state for an operation
    pub fn state(&self, operation: &str) -> BreakerState {
        let breakers = self.breakers.lock().unwrap();
        breakers
            .get(operation)
            .map(|e| e.state)
            .unwrap_or(BreakerState::Closed)
    }

    /// Get statistics for an operation
    pub fn stats(&self, operation: &str) -> BreakerStats {
        let breakers = self.breakers.lock().unwrap();
        
        if let Some(entry) = breakers.get(operation) {
            BreakerStats {
                state: entry.state,
                failure_count: entry.failure_count,
                last_failure: entry.last_failure,
            }
        } else {
            BreakerStats {
                state: BreakerState::Closed,
                failure_count: 0,
                last_failure: None,
            }
        }
    }

    /// Reset a specific circuit breaker
    pub fn reset(&self, operation: &str) {
        let mut breakers = self.breakers.lock().unwrap();
        breakers.remove(operation);
    }

    /// Reset all circuit breakers
    pub fn reset_all(&self) {
        let mut breakers = self.breakers.lock().unwrap();
        breakers.clear();
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct BreakerStats {
    pub state: BreakerState,
    pub failure_count: usize,
    pub last_failure: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed_by_default() {
        let breaker = CircuitBreaker::default();
        assert!(!breaker.is_open("test_op"));
        assert_eq!(breaker.state("test_op"), BreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_secs(30),
        };
        let breaker = CircuitBreaker::new(config);

        // Mark failures
        breaker.mark_failure("test_op");
        assert!(!breaker.is_open("test_op"));
        
        breaker.mark_failure("test_op");
        assert!(!breaker.is_open("test_op"));
        
        breaker.mark_failure("test_op");
        assert!(breaker.is_open("test_op"));
        assert_eq!(breaker.state("test_op"), BreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_resets_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_secs(30),
        };
        let breaker = CircuitBreaker::new(config);

        // Mark failures
        breaker.mark_failure("test_op");
        breaker.mark_failure("test_op");
        
        // Success resets
        breaker.mark_success("test_op");
        
        let stats = breaker.stats("test_op");
        assert_eq!(stats.state, BreakerState::Closed);
        assert_eq!(stats.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            reset_timeout: Duration::from_millis(100),
        };
        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.mark_failure("test_op");
        breaker.mark_failure("test_op");
        assert!(breaker.is_open("test_op"));

        // Wait for reset timeout
        std::thread::sleep(Duration::from_millis(150));

        // Should transition to half-open
        assert!(!breaker.is_open("test_op"));
        assert_eq!(breaker.state("test_op"), BreakerState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_stats() {
        let breaker = CircuitBreaker::default();

        breaker.mark_failure("test_op");
        breaker.mark_failure("test_op");

        let stats = breaker.stats("test_op");
        assert_eq!(stats.failure_count, 2);
        assert!(stats.last_failure.is_some());
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let breaker = CircuitBreaker::default();

        breaker.mark_failure("test_op");
        breaker.mark_failure("test_op");
        
        breaker.reset("test_op");
        
        let stats = breaker.stats("test_op");
        assert_eq!(stats.state, BreakerState::Closed);
        assert_eq!(stats.failure_count, 0);
    }
}