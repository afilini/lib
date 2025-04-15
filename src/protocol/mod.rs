use std::ops::Deref;

use crate::model::auth::SubkeyProof;

pub mod auth_init;
pub mod identity;
pub mod subkey;

pub struct LocalKeypair {
    keys: nostr::Keys,
    subkey_proof: Option<SubkeyProof>,
}

impl LocalKeypair {
    pub fn new(keys: nostr::Keys, subkey_proof: Option<SubkeyProof>) -> Self {
        Self { keys, subkey_proof }
    }

    pub fn subkey_proof(&self) -> Option<&SubkeyProof> {
        self.subkey_proof.as_ref()
    }
}

impl Deref for LocalKeypair {
    type Target = nostr::Keys;

    fn deref(&self) -> &Self::Target {
        &self.keys
    }
}