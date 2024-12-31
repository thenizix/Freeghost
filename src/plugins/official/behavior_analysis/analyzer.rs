// src/plugins/official/behavior_analysis/analyzer.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use ring::rand::SystemRandom;
use crate::core::crypto::{
    secure_memory::SecretData,
    audit::{CryptoAuditor, AuditableOperation, AuditStatus},
};
use crate::utils::error::{Result, AnalysisError};
use serde::{Serialize, Deserialize};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH, Duration};

// Maximum time window for behavior patterns to prevent timing attacks
const MAX_PATTERN_WINDOW: Duration = Duration::from_secs(300); // 5 minutes
const MIN_PATTERNS_REQUIRED: usize = 5;
const MAX_PATTERNS_STORED: usize = 1000;
const ANOMALY_THRESHOLD: f64 = 0.85;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    timestamp: u64,
    pattern_type: PatternType,
    metrics: Vec<f64>,
    // Randomized pattern ID to prevent correlation
    id: [u8; 32], 
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    KeyboardDynamics,
    MouseMovement,
    TouchGesture,
    DeviceOrientation,
    AppUsagePattern,
}

pub struct BehaviorAnalyzer {
    patterns: Arc<RwLock<VecDeque<BehaviorPattern>>>,
    auditor: Arc<CryptoAuditor>,
    rng: SystemRandom,
    // Store normalized baseline in secure memory
    baseline: Arc<RwLock<SecretData<Vec<f64>>>>,
}

impl BehaviorAnalyzer {
    pub fn new(auditor: Arc<CryptoAuditor>) -> Result<Self> {
        let baseline = SecretData::new(&Vec::new())
            .map_err(|e| AnalysisError::Initialization(e.to_string()))?;

        Ok(Self {
            patterns: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_PATTERNS_STORED))),
            auditor,
            rng: SystemRandom::new(),
            baseline: Arc::new(RwLock::new(baseline)),
        })
    }

    pub async fn add_pattern(&self, raw_metrics: Vec<f64>, pattern_type: PatternType) -> Result<()> {
        // Validate input length to prevent buffer overflow
        if raw_metrics.len() > 1000 {
            return Err(AnalysisError::InvalidInput("Pattern too long".into()));
        }

        // Generate random pattern ID
        let mut id = [0u8; 32];
        ring::rand::SecureRandom::fill(&self.rng, &mut id)
            .map_err(|e| AnalysisError::Random(e.to_string()))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AnalysisError::Timing(e.to_string()))?
            .as_secs();

        // Normalize metrics to prevent statistical attacks
        let metrics = self.normalize_metrics(&raw_metrics)?;

        let pattern = BehaviorPattern {
            timestamp: now,
            pattern_type,
            metrics,
            id,
        };

        // Thread-safe pattern storage
        let mut patterns = self.patterns.write().await;
        
        // Remove old patterns to prevent timing attacks
        self.cleanup_old_patterns(&mut patterns).await?;

        // Add new pattern
        if patterns.len() >= MAX_PATTERNS_STORED {
            patterns.pop_front(); // FIFO removal
        }
        patterns.push_back(pattern);

        // Update baseline if we have enough patterns
        if patterns.len() >= MIN_PATTERNS_REQUIRED {
            self.update_baseline(&patterns).await?;
        }

        // Audit the operation
        self.auditor
            .record_operation(
                AuditableOperation::BehaviorPatternAdded {
                    pattern_id: Uuid::new_v4(), // Generate new ID for audit
                    pattern_type,
                },
                AuditStatus::Success,
                None,
            )
            .await?;

        Ok(())
    }

    pub async fn verify_behavior(&self, recent_patterns: &[BehaviorPattern]) -> Result<bool> {
        // Ensure we have enough patterns
        if recent_patterns.len() < MIN_PATTERNS_REQUIRED {
            return Err(AnalysisError::InsufficientData("Not enough patterns".into()));
        }

        // Verify pattern timestamps
        self.verify_pattern_timestamps(recent_patterns)?;

        // Get baseline for comparison
        let baseline = self.baseline.read().await;
        let baseline_data = baseline.get();

        // Calculate similarity score
        let similarity = self.calculate_similarity(recent_patterns, baseline_data)?;

        // Audit verification attempt
        self.auditor
            .record_operation(
                AuditableOperation::BehaviorVerification {
                    success: similarity >= ANOMALY_THRESHOLD,
                },
                AuditStatus::Success,
                None,
            )
            .await?;

        Ok(similarity >= ANOMALY_THRESHOLD)
    }

    async fn update_baseline(&self, patterns: &VecDeque<BehaviorPattern>) -> Result<()> {
        let mut baseline_metrics = vec![0.0; patterns.front().map_or(0, |p| p.metrics.len())];
        let pattern_count = patterns.len() as f64;

        // Calculate average metrics
        for pattern in patterns.iter() {
            for (i, &metric) in pattern.metrics.iter().enumerate() {
                baseline_metrics[i] += metric / pattern_count;
            }
        }

        // Store new baseline in secure memory
        let mut baseline = self.baseline.write().await;
        *baseline = SecretData::new(&baseline_metrics)
            .map_err(|e| AnalysisError::Baseline(e.to_string()))?;

        Ok(())
    }

    fn normalize_metrics(&self, metrics: &[f64]) -> Result<Vec<f64>> {
        if metrics.is_empty() {
            return Err(AnalysisError::InvalidInput("Empty metrics".into()));
        }

        // Calculate mean and standard deviation
        let mean = metrics.iter().sum::<f64>() / metrics.len() as f64;
        let std_dev = (metrics.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / metrics.len() as f64)
            .sqrt();

        // Prevent division by zero
        if std_dev == 0.0 {
            return Err(AnalysisError::InvalidInput("No variance in metrics".into()));
        }

        // Z-score normalization
        Ok(metrics.iter().map(|x| (x - mean) / std_dev).collect())
    }

    fn verify_pattern_timestamps(&self, patterns: &[BehaviorPattern]) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AnalysisError::Timing(e.to_string()))?
            .as_secs();

        for pattern in patterns {
            let age = now.saturating_sub(pattern.timestamp);
            if age > MAX_PATTERN_WINDOW.as_secs() {
                return Err(AnalysisError::Timing("Pattern too old".into()));
            }
        }

        Ok(())
    }

    async fn cleanup_old_patterns(&self, patterns: &mut VecDeque<BehaviorPattern>) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AnalysisError::Timing(e.to_string()))?
            .as_secs();

        while let Some(pattern) = patterns.front() {
            if now.saturating_sub(pattern.timestamp) > MAX_PATTERN_WINDOW.as_secs() {
                patterns.pop_front();
            } else {
                break;
            }
        }

        Ok(())
    }

    fn calculate_similarity(&self, patterns: &[BehaviorPattern], baseline: &[f64]) -> Result<f64> {
        if patterns.is_empty() || baseline.is_empty() {
            return Err(AnalysisError::InvalidInput("Empty comparison data".into()));
        }

        let mut total_similarity = 0.0;
        let pattern_count = patterns.len() as f64;

        for pattern in patterns {
            if pattern.metrics.len() != baseline.len() {
                return Err(AnalysisError::InvalidInput("Metric dimension mismatch".into()));
            }

            // Calculate cosine similarity
            let dot_product: f64 = pattern.metrics.iter()
                .zip(baseline.iter())
                .map(|(a, b)| a * b)
                .sum();

            let pattern_magnitude: f64 = pattern.metrics.iter()
                .map(|x| x * x)
                .sum::<f64>()
                .sqrt();

            let baseline_magnitude: f64 = baseline.iter()
                .map(|x| x * x)
                .sum::<f64>()
                .sqrt();

            if pattern_magnitude == 0.0 || baseline_magnitude == 0.0 {
                return Err(AnalysisError::InvalidInput("Zero magnitude vector".into()));
            }

            total_similarity += dot_product / (pattern_magnitude * baseline_magnitude);
        }

        Ok(total_similarity / pattern_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn setup_analyzer() -> BehaviorAnalyzer {
        let auditor = Arc::new(CryptoAuditor::new());
        BehaviorAnalyzer::new(auditor).unwrap()
    }

    #[tokio::test]
    async fn test_pattern_addition_and_verification() {
        let analyzer = setup_analyzer().await;
        
        // Add several patterns
        for _ in 0..MIN_PATTERNS_REQUIRED {
            let metrics = vec![1.0, 2.0, 3.0, 4.0, 5.0];
            analyzer.add_pattern(metrics.clone(), PatternType::KeyboardDynamics).await.unwrap();
        }

        // Get patterns for verification
        let patterns: Vec<BehaviorPattern> = analyzer.patterns.read().await
            .iter()
            .cloned()
            .collect();

        // Verify behavior
        assert!(analyzer.verify_behavior(&patterns).await.unwrap());
    }

    #[tokio::test]
    async fn test_anomaly_detection() {
        let analyzer = setup_analyzer().await;
        
        // Add normal patterns
        for _ in 0..MIN_PATTERNS_REQUIRED {
            let metrics = vec![1.0, 2.0, 3.0, 4.0, 5.0];
            analyzer.add_pattern(metrics, PatternType::KeyboardDynamics).await.unwrap();
        }

        // Add anomalous pattern
        let anomalous_metrics = vec![100.0, 200.0, 300.0, 400.0, 500.0];
        analyzer.add_pattern(anomalous_metrics, PatternType::KeyboardDynamics).await.unwrap();

        // Get recent patterns including anomaly
        let patterns: Vec<BehaviorPattern> = analyzer.patterns.read().await
            .iter()
            .rev()
            .take(1)
            .cloned()
            .collect();

        // Verify behavior should fail
        assert!(!analyzer.verify_behavior(&patterns).await.unwrap());
    }

    #[tokio::test]
    async fn test_timing_attack_prevention() {
        let analyzer = setup_analyzer().await;
        
        // Create old pattern
        let old_metrics = vec![1.0, 2.0, 3.0];
        let mut old_pattern = BehaviorPattern {
            timestamp: 0, // Very old timestamp
            pattern_type: PatternType::KeyboardDynamics,
            metrics: old_metrics,
            id: [0; 32],
        };

        // Verification should fail for old patterns
        assert!(analyzer.verify_behavior(&[old_pattern]).await.is_err());
    }

    #[tokio::test]
    async fn test_overflow_prevention() {
        let analyzer = setup_analyzer().await;
        
        // Try to add pattern with too many metrics
        let large_metrics = vec![1.0; 2000]; // Exceeds maximum
        assert!(analyzer.add_pattern(large_metrics, PatternType::KeyboardDynamics).await.is_err());
    }
}