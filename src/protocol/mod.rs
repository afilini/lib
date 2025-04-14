use nostr::prelude::*;
use serde::{Deserialize, Serialize};

use crate::model::Nonce;
pub mod identity;

pub use identity::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubkeyMetadata {
    pub name: String,
    pub nonce: Nonce,
    pub valid_from: u64,
    pub expires_at: u64,
    pub permissions: Vec<SubkeyPermission>,
    pub version: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubkeyPermission {
    Auth,
    Payment,
}

pub struct PublicSubkey {
    pub metadata: SubkeyMetadata,
}

impl PublicSubkey {
    pub fn new(metadata: SubkeyMetadata) -> Self {
        Self { metadata }
    }

    pub fn verify(&self, master: &PublicKey) -> Result<(), Error> {
        Ok(())
    }
}

pub struct PrivateSubkey {
    pub metadata: SubkeyMetadata,
    pub key: Keys,
}

impl PrivateSubkey {
    pub fn new(metadata: SubkeyMetadata, key: &Keys) -> Self {
        // TODO

        Self {
            metadata,
            key: Keys::generate(),
        }
    }
}
pub enum Error {
    InvalidMetadata,
}
