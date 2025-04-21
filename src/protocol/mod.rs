use std::ops::Deref;

use model::auth::SubkeyProof;

pub mod auth_init;
pub mod calendar;
pub mod identity;
pub mod model;
pub mod subkey;

#[cfg_attr(feature = "bindings", derive(uniffi::Object))]
#[derive(Clone)]
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
