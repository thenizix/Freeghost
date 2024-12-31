// src/network/circuit_breaker/mod.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::VecDeque;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Copy)]
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Failing, not accepting requests
    HalfOpen,  // Testing if system has recovered
}

#[derive(Debug, Clone)]
pub struct CircuitStats {
    failures: u64,
    successes: u64,
    last_failure: Option<Instant>,
    last_success: Option<Instant>,
    state: CircuitState,
}

pub struct CircuitBreaker {
    name: String,
    state: Arc<RwLock<CircuitState>>,
    stats: Arc<RwLock<CircuitStats>>,
    failure_threshold: u64,
    recovery_timeout: Duration,
    half_open_timeout: Duration,
    health_window: Arc<RwLock<VecDeque<bool>>>,
    window_size: usize,
    success_threshold: u64,
}

impl CircuitBreaker {
    pub fn new(
        name: String,
        failure_threshold: u64,
        recovery_timeout: Duration,
        half_open_timeout: Duration,
        window_size: usize,
        success_threshold: u64,
    ) -> Self {
        Self {
            name,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            stats: Arc::new(RwLock::new(CircuitStats {
                failures: 0,
                successes: 0,
                last_failure: None,
                last_success: None,
                state: CircuitState::Closed,
            })),
            failure_threshold,
            recovery_timeout,
            half_open_timeout,
            health_window: Arc::new(RwLock::new(VecDeque::with_capacity(window_size))),
            window_size,
            success_threshold,
        }
    }

    pub async fn execute<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError>
    where
        F: Future<Output = Result<T, E>>,
        E: std::error::Error,
    {
        self.pre_execute().await?;

        let result = tokio::time::timeout(
            Duration::from_secs(30), // Configurable timeout
            operation
        ).await;

        match result {
            Ok(Ok(value)) => {
                self.record_success().await;
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(e.to_string()))
            }
            Err(_) => {
                self.record_failure().await;
                Err(CircuitBreakerError::Timeout)
            }
        }
    }

    async fn pre_execute(&self) -> Result<(), CircuitBreakerError> {
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Open => {
                let stats = self.stats.read().await;
                if let Some(last_failure) = stats.last_failure {
                    if last_failure.elapsed() > self.recovery_timeout {
                        // Transition to half-open state
                        *self.state.write().await = CircuitState::HalfOpen;
                        info!("Circuit breaker '{}' transitioning to half-open state", self.name);
                    } else {
                        return Err(CircuitBreakerError::CircuitOpen);
                    }
                }
            }
            CircuitState::HalfOpen => {
                // Only allow limited traffic in half-open state
                let window = self.health_window.read().await;
                if window.len() >= self.window_size {
                    return Err(CircuitBreakerError::CircuitHalfOpen);
                }
            }
            CircuitState::Closed => {} // Allow operation to proceed
        }
        
        Ok(())
    }

    async fn record_success(&self) {
        let mut stats = self.stats.write().await;
        let mut state = self.state.write().await;
        let mut window = self.health_window.write().await;

        stats.successes += 1;
        stats.last_success = Some(Instant::now());
        window.push_back(true);

        if window.len() > self.window_size {
            window.pop_front();
        }

        match *state {
            CircuitState::HalfOpen => {
                let success_count = window.iter().filter(|&&x| x).count() as u64;
                if success_count >= self.success_threshold {
                    *state = CircuitState::Closed;
                    info!("Circuit breaker '{}' closed after successful recovery", self.name);
                }
            }
            _ => {}
        }
    }

    async fn record_failure(&self) {
        let mut stats = self.stats.write().await;
        let mut state = self.state.write().await;
        let mut window = self.health_window.write().await;

        stats.failures += 1;
        stats.last_failure = Some(Instant::now());
        window.push_back(false);

        if window.len() > self.window_size {
            window.pop_front();
        }

        let failure_count = window.iter().filter(|&&x| !x).count() as u64;

        match *state {
            CircuitState::Closed if failure_count >= self.failure_threshold => {
                *state = CircuitState::Open;
                error!("Circuit breaker '{}' opened due to excessive failures", self.name);
            }
            CircuitState::HalfOpen => {
                *state = CircuitState::Open;
                warn!("Circuit breaker '{}' reopened due to failure in half-open state", self.name);
            }
            _ => {}
        }
    }

    pub async fn get_stats(&self) -> CircuitStats {
        self.stats.read().await.clone()
    }

    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        *self.stats.write().await = CircuitStats {
            failures: 0,
            successes: 0,
            last_failure: None,
            last_success: None,
            state: CircuitState::Closed,
        };
        self.health_window.write().await.clear();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    #[error("Circuit is open")]
    CircuitOpen,
    
    #[error("Circuit is half-open and at capacity")]
    CircuitHalfOpen,
    
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    
    #[error("Operation timed out")]
    Timeout,
}

// Factory for creating circuit breakers with different configurations
pub struct CircuitBreakerFactory {
    default_failure_threshold: u64,
    default_recovery_timeout: Duration,
    default_half_open_timeout: Duration,
    default_window_size: usize,
    default_success_threshold: u64,
}

impl CircuitBreakerFactory {
    pub fn new() -> Self {
        Self {
            default_failure_threshold: 5,
            default_recovery_timeout: Duration::from_secs(60),
            default_half_open_timeout: Duration::from_secs(30),
            default_window_size: 100,
            default_success_threshold: 3,
        }
    }

    pub fn create(&self, name: String) -> CircuitBreaker {
        CircuitBreaker::new(
            name,
            self.default_failure_threshold,
            self.default_recovery_timeout,
            self.default_half_open_timeout,
            self.default_window_size,
            self.default_success_threshold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_circuit_breaker_state_transitions() {
        let breaker = CircuitBreaker::new(
            "test".to_string(),
            3,  // failure_threshold
            Duration::from_millis(100),  // recovery_timeout
            Duration::from_millis(50),   // half_open_timeout
            10,  // window_size
            2,   // success_threshold
        );

        // Test successful operations
        for _ in 0..5 {
            breaker.record_success().await;
        }
        assert!(matches!(breaker.get_state().await, CircuitState::Closed));

        // Test failures leading to open state
        for _ in 0..4 {
            breaker.record_failure().await;
        }
        assert!(matches!(breaker.get_state().await, CircuitState::Open));

        // Test recovery
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(matches!(breaker.get_state().await, CircuitState::HalfOpen));
    }

    #[tokio::test]
    async fn test_circuit_breaker_execution() {
        let breaker = CircuitBreaker::new(
            "test-exec".to_string(),
            3,
            Duration::from_secs(1),
            Duration::from_millis(500),
            10,
            2,
        );

        // Test successful execution
        let result = breaker.execute(async {
            Ok::<_, std::io::Error>("success")
        }).await;
        assert!(result.is_ok());

        // Test failed execution
        let result = breaker.execute(async {
            Err::<&str, _>(std::io::Error::new(std::io::ErrorKind::Other, "error"))
        }).await;
        assert!(result.is_err());
    }
}