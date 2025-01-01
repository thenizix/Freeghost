use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: Uuid,
    pub template: BiometricTemplate,
    pub metadata: IdentityMetadata,
    pub behavior_profile: BehaviorProfile,
    pub verification_status: VerificationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricTemplate {
    pub features: Vec<f32>,
    pub quality_score: f32,
    pub created_at: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityMetadata {
    pub created_at: u64,
    pub last_verified: Option<u64>,
    pub verification_count: u32,
    pub risk_score: f32,
    pub device_info: Option<DeviceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    pub patterns: Vec<BehaviorPattern>,
    pub trust_score: f32,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    pub pattern_type: PatternType,
    pub confidence: f32,
    pub occurrences: u32,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_type: String,
    pub os_info: String,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    TimeOfDay,
    Location,
    DeviceUsage,
    NetworkPattern,
    InteractionStyle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    Unverified,
    Pending,
    Verified,
    Suspended,
    Revoked,
}

impl Identity {
    pub fn new(template: BiometricTemplate) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: Uuid::new_v4(),
            template,
            metadata: IdentityMetadata {
                created_at: now,
                last_verified: None,
                verification_count: 0,
                risk_score: 0.0,
                device_info: None,
            },
            behavior_profile: BehaviorProfile {
                patterns: Vec::new(),
                trust_score: 0.0,
                last_updated: now,
            },
            verification_status: VerificationStatus::Unverified,
        }
    }

    pub fn update_verification(&mut self, verified: bool) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.metadata.last_verified = Some(now);
        self.metadata.verification_count += 1;

        if verified {
            self.verification_status = VerificationStatus::Verified;
        }
    }

    pub fn update_behavior(&mut self, pattern: BehaviorPattern) {
        if let Some(existing) = self.behavior_profile
            .patterns
            .iter_mut()
            .find(|p| p.pattern_type == pattern.pattern_type)
        {
            existing.confidence = (existing.confidence * 0.7 + pattern.confidence * 0.3).min(1.0);
            existing.occurrences += 1;
            existing.last_seen = pattern.last_seen;
        } else {
            self.behavior_profile.patterns.push(pattern);
        }

        // Update trust score based on patterns
        self.recalculate_trust_score();
    }

    fn recalculate_trust_score(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for pattern in &self.behavior_profile.patterns {
            let age_weight = {
                let age = now - pattern.last_seen;
                (1.0 / (1.0 + (age as f32 / 86400.0))).max(0.1) // Decay over days
            };

            let confidence_weight = pattern.confidence * 
                (pattern.occurrences as f32).min(10.0) / 10.0;

            total_score += confidence_weight * age_weight;
            total_weight += age_weight;
        }

        self.behavior_profile.trust_score = if total_weight > 0.0 {
            (total_score / total_weight).min(1.0)
        } else {
            0.0
        };

        self.behavior_profile.last_updated = now;
    }
}

impl BiometricTemplate {
    pub fn new(features: Vec<f32>, quality_score: f32, hash: String) -> Self {
        Self {
            features,
            quality_score,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hash,
        }
    }
}
