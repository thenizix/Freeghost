// src/core/identity/types.rs
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricData {
    pub raw_data: Vec<u8>,
    pub capture_timestamp: DateTime<Utc>,
    pub source_device: String,
    pub capture_quality: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricTemplate {
    pub id: Uuid,
    pub data: Vec<u8>,
    pub metadata: TemplateMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub algorithm_version: String,
    pub quality_score: f64,
}

impl BiometricTemplate {
    pub fn verify_quality(&self, minimum_quality: f64) -> bool {
        self.metadata.quality_score >= minimum_quality
    }

    pub fn is_fresh(&self, max_age: chrono::Duration) -> bool {
        let age = Utc::now() - self.metadata.created_at;
        age <= max_age
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_quality_verification() {
        let template = BiometricTemplate {
            id: Uuid::new_v4(),
            data: vec![1, 2, 3],
            metadata: TemplateMetadata {
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                algorithm_version: "1.0".to_string(),
                quality_score: 0.85,
            },
        };

        assert!(template.verify_quality(0.8));
        assert!(!template.verify_quality(0.9));
    }

    #[test]
    fn test_template_freshness() {
        let template = BiometricTemplate {
            id: Uuid::new_v4(),
            data: vec![1, 2, 3],
            metadata: TemplateMetadata {
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                algorithm_version: "1.0".to_string(),
                quality_score: 0.85,
            },
        };

        assert!(template.is_fresh(chrono::Duration::hours(1)));
        assert!(!template.is_fresh(chrono::Duration::seconds(0)));
    }
}