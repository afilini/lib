use std::{
    collections::HashSet,
    ops::Deref,
    time::{Duration, SystemTime},
};

use serde::de::DeserializeOwned;

use nostr::{
    event::Kind,
    key::PublicKey,
};

use crate::protocol::model::{auth::SubkeyProof, event_kinds::SUBKEY_PROOF};

use super::{CleartextEvent, Conversation, ConversationError, ConversationMessage, Response};

pub trait MultiKeySender: Sized + Send + 'static {
    const VALIDITY_SECONDS: u64;

    type Error: std::error::Error + Send + Sync + 'static;
    type Message: DeserializeOwned;

    fn init(_state: &MultiKeySenderAdapter<Self>) -> Result<Response, Self::Error>;

    fn build_initial_message(
        _state: &mut MultiKeySenderAdapter<Self>,
        _new_key: Option<PublicKey>,
    ) -> Result<Response, Self::Error> {
        Ok(Response::default())
    }

    fn on_message(
        _state: &mut MultiKeySenderAdapter<Self>,
        _event: &CleartextEvent,
        _message: &Self::Message,
    ) -> Result<Response, Self::Error>;
}

/// A conversation wrapper that handles key switching
/// 
/// This is specifically used for senders which follow this pattern:
///   1. Send out a message to a key (or if already known also to all subkeys)
///   2. Receive SUBKEY_PROOF messages asking to switch to a new key
///   3. Send out again the same message to the new key
///   4. Wait for the response
pub struct MultiKeySenderAdapter<Inner> {
    pub user: PublicKey,
    pub subkeys: HashSet<PublicKey>,
    pub expires_at: SystemTime,
    pub inner: Inner,
}

impl<T: MultiKeySender> Conversation for MultiKeySenderAdapter<T> {
    fn init(&mut self) -> Result<Response, ConversationError> {
        // Call init first, this normally sets up the filters
        let mut response = <T as MultiKeySender>::init(self).map_err(|e| ConversationError::Inner(Box::new(e)))?;

        // Then build the initial message, this will be sent to the user
        let initial_message = <T as MultiKeySender>::build_initial_message(self, None).map_err(|e| ConversationError::Inner(Box::new(e)))?;
        response.extend_responses(initial_message);

        response.set_recepient_keys(self.user, &self.subkeys);

        Ok(response)
    }

    fn on_message(&mut self, message: ConversationMessage) -> Result<Response, ConversationError> {
        match message {
            ConversationMessage::Cleartext(event) => {
                if let Ok(content) = serde_json::from_value(event.content.clone()) {
                    let mut response = <T as MultiKeySender>::on_message(self, &event, &content)
                        .map_err(|e| ConversationError::Inner(Box::new(e)))?;
                    response.set_recepient_keys(self.user, &self.subkeys);
                    Ok(response)
                } else if event.kind == Kind::from(SUBKEY_PROOF) {
                    let proof = match serde_json::from_value::<SubkeyProof>(event.content.clone()) {
                        Ok(content) => content,
                        Err(_) => {
                            return Ok(Response::default());
                        }
                    };

                    if let Err(e) = proof.verify(&event.pubkey) {
                        log::warn!("Invalid proof: {:?}", e);
                        return Ok(Response::default());
                    }

                    let response_result = if event.pubkey == self.user {
                        // We only knew about a subkey and we thought it was the main key. Switching it now
                        log::debug!("Switching {:?} to new main key: {:?}", event.pubkey, proof.main_key);

                        self.subkeys.insert(event.pubkey);
                        self.user = proof.main_key.into();

                        <T as MultiKeySender>::build_initial_message(self, Some(self.user))
                    } else {
                        // We already knew about the main key, but we got a proof for a new subkey
                        log::debug!("Learned about a new subkey for {:?}: {:?}", self.user, event.pubkey);

                        assert!(self.user == proof.main_key.into());

                        self.subkeys.insert(event.pubkey);

                        <T as MultiKeySender>::build_initial_message(self, Some(event.pubkey))
                    };

                    response_result.map_err(|e| ConversationError::Inner(Box::new(e)))
                } else {
                    Ok(Response::default())
                }
            }
            ConversationMessage::Encrypted(_event) => {
                Ok(Response::default())
            }
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_at < SystemTime::now()
    }
}

impl<Inner: MultiKeySender> MultiKeySenderAdapter<Inner> {
    pub fn new_with_user(user: PublicKey, subkeys: Vec<PublicKey>, inner: Inner) -> Self {
        Self {
            user,
            subkeys: subkeys.into_iter().collect(),
            expires_at: SystemTime::now() + Duration::from_secs(Inner::VALIDITY_SECONDS),
            inner,
        }
    }
}

impl<Inner: MultiKeySender> Deref for MultiKeySenderAdapter<Inner> {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
