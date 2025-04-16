use std::{collections::HashSet, ops::Deref};

use nostr::{event::Kind, filter::Filter, key::PublicKey, Tag};
use serde::{Deserialize, Serialize};

use crate::{
    protocol::{
        auth_init::AuthInitUrl,
        model::{
            auth::{AuthChallengeContent, AuthInitContent, AuthResponseContent, ClientInfo, SubkeyProof},
            bindings,
            event_kinds::{AUTH_CHALLENGE, AUTH_INIT, AUTH_RESPONSE},
        },
    },
    router::{Conversation, ConversationError, ConversationMessage, MultiKeyTrait, Response},
};

pub struct AuthInitConversation {
    pub url: AuthInitUrl,
    pub relays: Vec<String>,
}

impl Conversation for AuthInitConversation {
    fn init(&self) -> Result<Response, ConversationError> {
        let content = AuthInitContent {
            token: self.url.token.clone(),
            client_info: ClientInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                name: "Portal".to_string(),
            },
            preferred_relays: self.relays.clone(),
        };

        let tags = self
            .url
            .all_keys()
            .iter()
            .map(|k| Tag::public_key(*k.deref()))
            .collect();
        let response = Response::new()
            .reply_to(self.url.send_to(), Kind::from(AUTH_INIT), tags, content)
            .finish();

        Ok(response)
    }

    fn on_message(&mut self, _message: ConversationMessage) -> Result<Response, ConversationError> {
        Ok(Response::default())
    }

    fn is_expired(&self) -> bool {
        false
    }
}

pub struct AuthChallengeListenerConversation {
    subkey_proof: Option<SubkeyProof>,
}

impl AuthChallengeListenerConversation {
    pub fn new(subkey_proof: Option<SubkeyProof>) -> Self {
        Self { subkey_proof }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(uniffi::Record))]
pub struct AuthChallengeEvent {
    pub service_key: bindings::PublicKey,
    pub recipient: bindings::PublicKey,
    pub challenge: String,
    pub expires_at: u64,
    pub required_permissions: Vec<String>,
}

impl MultiKeyTrait for AuthChallengeListenerConversation {
    const VALIDITY_SECONDS: u64 = u64::MAX;

    type Error = ConversationError;
    type Message = AuthChallengeContent;

    fn init(_state: &crate::router::MultiKeyProxy<Self>) -> Result<Response, Self::Error> {
        Ok(Response::new().filter(Filter::new().kinds(vec![Kind::from(AUTH_CHALLENGE)])))
    }

    fn on_message(
        _state: &mut crate::router::MultiKeyProxy<Self>,
        event: &crate::router::CleartextEvent,
        content: &Self::Message,
    ) -> Result<Response, Self::Error> {
        log::debug!(
            "Received auth challenge from {}: {:?}",
            event.pubkey,
            content
        );

        if content.expires_at.as_u64() < nostr::Timestamp::now().as_u64() {
            log::warn!("Ignoring expired auth challenge");
            return Ok(Response::default());
        }

        let service_key= if let Some(subkey_proof) = &content.subkey_proof {
            if let Err(e) = subkey_proof.verify(&event.pubkey) {
                log::warn!("Ignoring request with invalid subkey proof: {}", e);
                return Ok(Response::default());
            }

            subkey_proof.main_key
        } else {
            event.pubkey.into()
        };

        let response = Response::new().notify(AuthChallengeEvent {
            service_key,
            recipient: event.pubkey.into(),
            challenge: content.challenge.clone(),
            expires_at: content.expires_at.as_u64(),
            required_permissions: content.required_permissions.clone(),
        });

        Ok(response)
    }
}

pub struct AuthResponseConversation {
    event: AuthChallengeEvent,
    granted_permissions: Vec<String>,
    subkey_proof: Option<SubkeyProof>,
}

impl AuthResponseConversation {
    pub fn new(event: AuthChallengeEvent, granted_permissions: Vec<String>, subkey_proof: Option<SubkeyProof>) -> Self {
        Self { event, granted_permissions, subkey_proof }
    }
}

impl Conversation for AuthResponseConversation {
    fn init(&self) -> Result<Response, ConversationError> {
        let content = AuthResponseContent {
            granted_permissions: self.granted_permissions.clone(),
            session_token: "randomlygeneratedtoken".to_string(),
            subkey_proof: self.subkey_proof.clone(),
        };

        let mut keys = HashSet::new();
        keys.insert(self.event.service_key);
        keys.insert(self.event.recipient);

        let tags = keys.iter().map(|k| Tag::public_key(*k.deref())).collect();
        let response = Response::new()
            .reply_to(self.event.recipient.into(), Kind::from(AUTH_RESPONSE), tags, content)
            .finish();

        Ok(response)
    }

    fn on_message(&mut self, _message: ConversationMessage) -> Result<Response, ConversationError> {
        Ok(Response::default())
    }

    fn is_expired(&self) -> bool {
        false
    }
}

