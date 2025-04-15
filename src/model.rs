use std::time::{SystemTime, UNIX_EPOCH};

use hex;
use serde::{Deserialize, Serialize};

// Event kind ranges:
// Authentication: 27000-27999
// Payments: 28000-28999
// Identity: 29000-29999

pub mod event_kinds {
    // Authentication events (27000-27999)
    pub const AUTH_CHALLENGE: u16 = 27000;
    pub const AUTH_RESPONSE: u16 = 27001;
    pub const AUTH_SUCCESS: u16 = 27002;
    pub const AUTH_INIT: u16 = 27010;

    // Payment events (28000-28999)
    pub const PAYMENT_REQUEST: u16 = 28000;
    pub const PAYMENT_RESPONSE: u16 = 28001;
    pub const PAYMENT_CONFIRMATION: u16 = 28002;
    pub const PAYMENT_ERROR: u16 = 28003;
    pub const PAYMENT_RECEIPT: u16 = 28004;
    pub const RECURRING_PAYMENT_REQUEST: u16 = 28005;
    pub const RECURRING_PAYMENT_AUTH: u16 = 28006;
    pub const RECURRING_PAYMENT_CANCEL: u16 = 28007;

    // Identity events (29000-29999)
    pub const CERTIFICATE_REQUEST: u16 = 29000;
    pub const CERTIFICATE_RESPONSE: u16 = 29001;
    pub const CERTIFICATE_ERROR: u16 = 29002;
    pub const CERTIFICATE_REVOCATION: u16 = 29003;
    pub const CERTIFICATE_VERIFY_REQUEST: u16 = 29004;
    pub const CERTIFICATE_VERIFY_RESPONSE: u16 = 29005;

    // Control events (30000-30999)
    pub const SUBKEY_PROOF: u16 = 30000;
}

#[derive(Debug, Clone)]
pub struct Nonce([u8; 32]);

impl Nonce {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Serialize for Nonce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Convert bytes to hex string
        let hex = hex::encode(self.0);
        serializer.serialize_str(&hex)
    }
}

impl<'de> Deserialize<'de> for Nonce {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let hex_str = String::deserialize(deserializer)?;

        // Convert hex string back to bytes
        let bytes = hex::decode(&hex_str)
            .map_err(|e| Error::custom(format!("Invalid hex string: {}", e)))?;

        if bytes.len() != 32 {
            return Err(Error::custom(format!(
                "Invalid nonce length: expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Nonce(arr))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn new(timestamp: u64) -> Self {
        Self(timestamp)
    }

    pub fn now() -> Self {
        Self(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())
    }

    pub fn now_plus_seconds(seconds: u64) -> Self {
        let mut ts = Self::now();
        ts.0 += seconds;
        ts
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as string to avoid precision loss
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;

        s.parse::<u64>()
            .map(Timestamp::new)
            .map_err(|e| Error::custom(format!("Invalid timestamp: {}", e)))
    }
}

pub mod auth {
    use crate::protocol::subkey::{PublicSubkeyVerifier, SubkeyError, SubkeyMetadata};

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServiceInformation {
        pub service_pubkey: nostr::PublicKey,
        pub relays: Vec<String>,
        pub token: String,
        pub subkey: Option<nostr::PublicKey>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AuthInitContent {
        pub token: String,
        pub client_info: ClientInfo,
        pub preferred_relays: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClientInfo {
        pub name: String,
        pub version: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AuthChallengeContent {
        pub challenge: String,
        pub expires_at: Timestamp,
        pub required_permissions: Vec<String>,
        pub subkey_proof: Option<SubkeyProof>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SubkeyProof {
        pub main_key: nostr::PublicKey,
        pub signature: String,
        pub metadata: SubkeyMetadata,
    }

    impl SubkeyProof {
        pub fn verify(&self, subkey: &nostr::PublicKey) -> Result<(), SubkeyError> {
            self.main_key.verify_subkey(subkey, &self.metadata)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AuthResponseContent {
        pub granted_permissions: Vec<String>,
        pub session_token: String,
        pub subkey_proof: Option<SubkeyProof>,
    }
}

pub mod identity {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CertificateRequestContent {
        pub requested_types: Vec<String>,
        pub requested_fields: Vec<String>,
        pub purpose: String,
        pub require_status_proofs: Option<bool>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CertificateResponseContent {
        pub certificates: std::collections::HashMap<String, serde_json::Value>,
        pub status_proofs: Option<std::collections::HashMap<String, serde_json::Value>>,
    }
}

pub mod payment {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SinglePaymentRequestContent {
        pub payment_type: String,
        pub amount: u64,
        pub currency: String,
        pub invoice: String,
        pub auth_token: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PaymentResponseContent {
        pub status: String,
        pub payment_hash: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PaymentConfirmationContent {
        pub status: String,
        pub payment_hash: String,
        pub preimage: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RecurringPaymentRequestContent {
        pub payment_type: String,
        pub subscription_id: String,
        pub amount: u64,
        pub currency: String,
        pub exchange_rate: Option<ExchangeRate>,
        pub recurrence: RecurrenceInfo,
        pub request_expires_at: u64,
        pub auth_token: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExchangeRate {
        pub rate: f64,
        pub source: String,
        pub timestamp: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RecurrenceInfo {
        pub interval: String,
        pub interval_count: u32,
        pub start_date: u64,
        pub end_date: Option<u64>,
        pub max_payments: Option<u32>,
        pub trial_period_days: Option<u32>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RecurringPaymentAuthorizationContent {
        pub subscription_id: String,
        pub status: String,
        pub authorized_amount: u64,
        pub authorized_currency: String,
        pub exchange_rate_limit: Option<ExchangeRateLimit>,
        pub authorized_recurrence: RecurrenceInfo,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExchangeRateLimit {
        pub max_slippage_percent: f64,
        pub reference_rate: f64,
    }
}
