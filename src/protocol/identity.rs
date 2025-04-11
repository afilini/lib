use serde::{Deserialize, Serialize};
use crate::model::Timestamp;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    InPerson,
    VideoCall,
    DocumentUpload,
    RegistryCheck,
    ThirdPartyVerification,
    #[serde(rename = "custom")]
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub version: u32,
    pub subject: CertificateSubject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_class: Option<String>,
    pub data: CertificateData,
    pub metadata: CertificateMetadata,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CertificateData {
    #[serde(rename = "personal")]
    Person(PersonData),
    #[serde(rename = "business")]
    Business(BusinessData),
    #[serde(rename = "custom")]
    Custom {
        #[serde(flatten)]
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateSubject {
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateMetadata {
    pub issuer_pubkey: String,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub verification_level: VerificationLevel,
    pub verification_method: VerificationMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonData {
    pub full_name: String,
    pub date_of_birth: String,
    pub nationality: String,
    pub document_type: String,
    pub document_number: String,
    pub place_of_birth: Option<String>,
    pub gender: Option<String>,
    pub issue_date: Option<String>,
    pub expiry_date: Option<String>,
    pub address: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessData {
    pub legal_name: String,
    pub trading_name: Option<String>,
    pub registration_number: String,
    pub tax_id: Option<String>,
    pub jurisdiction: String,
    pub incorporation_date: String,
    pub business_type: String,
    pub address: Address,
    pub contact: ContactInfo,
    pub website: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealableField {
    pub value: serde_json::Value,
    pub salt: String,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct PreparedCertificate {
    pub version: u32,
    pub fields: HashMap<String, RevealableField>,
}

impl Certificate {
    /// Prepares the certificate for selective revealing by breaking down all fields
    /// into individually revealable components with their own salts and hashes.
    pub fn prepare_for_revealing(&self) -> Result<PreparedCertificate, RevealError> {
        // Only serialize the data part
        let value = serde_json::to_value(&self.data)?;
        
        let mut fields = HashMap::new();
        flatten_json("", &value, &mut fields)?;

        Ok(PreparedCertificate {
            version: self.version,
            fields,
        })
    }
}

impl RevealableField {
    pub fn new<T: Serialize>(value: &T) -> Result<Self, RevealError> {
        let value = serde_json::to_value(value)?;
        let salt = generate_salt();
        let hash = hash_field(&value, &salt)?;

        Ok(Self {
            value,
            salt,
            hash,
        })
    }
}

fn generate_salt() -> String {
    use rand::{thread_rng, RngCore};
    let mut bytes = [0u8; 32];
    thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

fn hash_field(value: &serde_json::Value, salt: &str) -> Result<String, RevealError> {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(value.to_string().as_bytes());
    hasher.update(salt.as_bytes());
    
    Ok(hex::encode(hasher.finalize()))
}

fn flatten_json(
    prefix: &str,
    value: &serde_json::Value,
    fields: &mut HashMap<String, RevealableField>
) -> Result<(), RevealError> {
    match value {
        serde_json::Value::Object(map) => {
            // Add the object itself as a field
            if !prefix.is_empty() {
                fields.insert(
                    prefix.to_string(),
                    RevealableField::new(value)?
                );
            }

            // Then add all its fields
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(&new_prefix, val, fields)?;
            }
        },
        serde_json::Value::Array(arr) => {
            // Add the array itself as a field
            if !prefix.is_empty() {
                fields.insert(
                    prefix.to_string(),
                    RevealableField::new(value)?
                );
            }

            // Then add all its elements
            for (i, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}.{}", prefix, i);
                flatten_json(&new_prefix, val, fields)?;
            }
        },
        _ => {
            if !prefix.is_empty() {
                fields.insert(
                    prefix.to_string(),
                    RevealableField::new(value)?
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum RevealError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Invalid field value")]
    InvalidField,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_person_certificate() -> Certificate {
        Certificate {
            version: 1,
            subject: CertificateSubject {
                pubkey: "test_pubkey".to_string(),
            },
            certificate_class: None,
            data: CertificateData::Person(PersonData {
                full_name: "John Doe".to_string(),
                date_of_birth: "1990-01-01".to_string(),
                nationality: "US".to_string(),
                document_type: "passport".to_string(),
                document_number: "123456789".to_string(),
                place_of_birth: Some("New York".to_string()),
                gender: Some("M".to_string()),
                issue_date: Some("2020-01-01".to_string()),
                expiry_date: Some("2030-01-01".to_string()),
                address: Some(Address {
                    street: "123 Main St".to_string(),
                    city: "New York".to_string(),
                    state: Some("NY".to_string()),
                    postal_code: "10001".to_string(),
                    country: "US".to_string(),
                }),
            }),
            metadata: CertificateMetadata {
                issuer_pubkey: "issuer_pubkey".to_string(),
                issued_at: Timestamp::new(1234567890),
                expires_at: Timestamp::new(9876543210),
                verification_level: VerificationLevel::High,
                verification_method: VerificationMethod::InPerson,
            },
            signature: "test_signature".to_string(),
        }
    }

    fn create_test_business_certificate() -> Certificate {
        Certificate {
            version: 1,
            subject: CertificateSubject {
                pubkey: "test_pubkey".to_string(),
            },
            certificate_class: None,
            data: CertificateData::Business(BusinessData {
                legal_name: "Acme Corp".to_string(),
                trading_name: Some("Acme".to_string()),
                registration_number: "REG123456".to_string(),
                tax_id: Some("TAX123456".to_string()),
                jurisdiction: "Delaware".to_string(),
                incorporation_date: "2000-01-01".to_string(),
                business_type: "Corporation".to_string(),
                address: Address {
                    street: "456 Business Ave".to_string(),
                    city: "Dover".to_string(),
                    state: Some("DE".to_string()),
                    postal_code: "19901".to_string(),
                    country: "US".to_string(),
                },
                contact: ContactInfo {
                    email: Some("contact@acme.com".to_string()),
                    phone: Some("+1234567890".to_string()),
                },
                website: Some("https://acme.com".to_string()),
            }),
            metadata: CertificateMetadata {
                issuer_pubkey: "issuer_pubkey".to_string(),
                issued_at: Timestamp::new(1234567890),
                expires_at: Timestamp::new(9876543210),
                verification_level: VerificationLevel::High,
                verification_method: VerificationMethod::DocumentUpload,
            },
            signature: "test_signature".to_string(),
        }
    }

    fn create_test_custom_certificate() -> Certificate {
        let custom_data = serde_json::json!({
            "custom_field": "custom_value",
            "nested": {
                "field1": "value1",
                "field2": 42
            },
            "array": ["item1", "item2", "item3"]
        });

        Certificate {
            version: 1,
            subject: CertificateSubject {
                pubkey: "test_pubkey".to_string(),
            },
            certificate_class: Some("test_class".to_string()),
            data: CertificateData::Custom {
                data: custom_data,
            },
            metadata: CertificateMetadata {
                issuer_pubkey: "issuer_pubkey".to_string(),
                issued_at: Timestamp::new(1234567890),
                expires_at: Timestamp::new(9876543210),
                verification_level: VerificationLevel::Medium,
                verification_method: VerificationMethod::Custom("test_method".to_string()),
            },
            signature: "test_signature".to_string(),
        }
    }

    #[test]
    fn test_prepare_person_certificate() {
        let cert = create_test_person_certificate();
        let prepared = cert.prepare_for_revealing().unwrap();

        // Check version
        assert_eq!(prepared.version, 1);

        // Check that essential fields exist
        let essential_fields = [
            "full_name",
            "date_of_birth",
            "nationality",
            "document_type",
            "document_number",
        ];

        for field in essential_fields {
            assert!(prepared.fields.contains_key(field), "Missing field: {}", field);
        }

        // Check optional fields
        let optional_fields = [
            "place_of_birth",
            "gender",
            "address.street",
            "address.city",
            "address.state",
        ];

        for field in optional_fields {
            assert!(prepared.fields.contains_key(field), "Missing optional field: {}", field);
        }

        // Verify that each field has the correct structure
        for (_, field) in prepared.fields {
            assert!(!field.salt.is_empty(), "Salt should not be empty");
            assert!(!field.hash.is_empty(), "Hash should not be empty");
            assert!(field.value.is_string() || field.value.is_number() || field.value.is_object(),
                "Value should be a valid JSON type");
        }
    }

    #[test]
    fn test_prepare_business_certificate() {
        let cert = create_test_business_certificate();
        let prepared = cert.prepare_for_revealing().unwrap();

        // Check that essential business fields exist
        let business_fields = [
            "legal_name",
            "registration_number",
            "jurisdiction",
            "incorporation_date",
            "business_type",
            "address.street",
            "address.city",
            "address.country",
        ];

        for field in business_fields {
            assert!(prepared.fields.contains_key(field), "Missing field: {}", field);
        }

        // Check optional business fields
        let optional_fields = [
            "trading_name",
            "tax_id",
            "website",
            "contact.email",
            "contact.phone",
        ];

        for field in optional_fields {
            assert!(prepared.fields.contains_key(field), "Missing optional field: {}", field);
        }
    }

    #[test]
    fn test_prepare_custom_certificate() {
        let cert = create_test_custom_certificate();
        println!("cert: {:?}", cert);
        println!("{}", serde_json::to_string_pretty(&cert).unwrap());

        let prepared = cert.prepare_for_revealing().unwrap();
        dbg!(&prepared);

        // Check custom fields
        let custom_fields = [
            "custom_field",
            "nested.field1",
            "nested.field2",
            "array.0",
            "array.1",
            "array.2",
        ];

        for field in custom_fields {
            assert!(prepared.fields.contains_key(field), "Missing field: {}", field);
        }

        // Check that array values are correct
        assert_eq!(
            prepared.fields.get("array.0").unwrap().value.as_str().unwrap(),
            "item1"
        );
        assert_eq!(
            prepared.fields.get("array.1").unwrap().value.as_str().unwrap(),
            "item2"
        );
        assert_eq!(
            prepared.fields.get("array.2").unwrap().value.as_str().unwrap(),
            "item3"
        );

        // Check that nested object values are correct
        assert_eq!(
            prepared.fields.get("nested.field1").unwrap().value.as_str().unwrap(),
            "value1"
        );
        assert_eq!(
            prepared.fields.get("nested.field2").unwrap().value.as_u64().unwrap(),
            42
        );
    }

    #[test]
    fn test_field_hashes_are_deterministic() {
        let cert = create_test_person_certificate();
        let prepared1 = cert.prepare_for_revealing().unwrap();
        let prepared2 = cert.prepare_for_revealing().unwrap();

        // The hashes should be different (because of random salt)
        // but the values should be the same
        for (key, field1) in &prepared1.fields {
            let field2 = &prepared2.fields[key];
            assert_eq!(field1.value, field2.value, "Values should be equal for {}", key);
            assert_ne!(field1.hash, field2.hash, "Hashes should be different for {}", key);
            assert_ne!(field1.salt, field2.salt, "Salts should be different for {}", key);
        }
    }

    #[test]
    fn test_hash_verification() {
        let cert = create_test_person_certificate();
        let prepared = cert.prepare_for_revealing().unwrap();

        // Verify that we can recompute the hash for each field
        for (key, field) in &prepared.fields {
            let computed_hash = hash_field(&field.value, &field.salt).unwrap();
            assert_eq!(computed_hash, field.hash, "Hash verification failed for {}", key);
        }
    }
}

