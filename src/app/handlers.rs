use std::{collections::HashSet, ops::Deref, time::{Duration, SystemTime}};

use nostr::{Tag, event::Kind, filter::Filter, key::PublicKey};
use serde::{Deserialize, Serialize};

use crate::{
    protocol::{
        auth_init::AuthInitUrl,
        model::{
            auth::{
                AuthChallengeContent, AuthInitContent, AuthResponseContent, ClientInfo, SubkeyProof,
            },
            bindings,
            event_kinds::{AUTH_CHALLENGE, AUTH_INIT, AUTH_RESPONSE},
        },
    },
    router::{Conversation, ConversationError, ConversationMessage, MultiKeySender, Response},
};

pub struct AuthInitConversation {
    pub url: AuthInitUrl,
    pub relays: Vec<String>,
}

impl Conversation for AuthInitConversation {
    fn init(&mut self) -> Result<Response, ConversationError> {
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
    local_key: PublicKey,
    subkey_proof: Option<SubkeyProof>,
    expires_at: SystemTime,
}

impl AuthChallengeListenerConversation {
    pub fn new(local_key: PublicKey, subkey_proof: Option<SubkeyProof>) -> Self {
        Self { local_key, subkey_proof, expires_at: SystemTime::now() + Duration::from_secs(60 * 5) }
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

impl Conversation for AuthChallengeListenerConversation {
    fn init(&mut self) -> Result<Response, ConversationError> {
        let mut filter = Filter::new()
            .kinds(vec![Kind::from(AUTH_CHALLENGE)])
            .pubkey(self.local_key);

        if let Some(subkey_proof) = &self.subkey_proof {
            filter = filter.pubkey(subkey_proof.main_key.into());
        }

        Ok(Response::new().filter(filter))
    }

    fn on_message(&mut self, message: ConversationMessage) -> Result<Response, ConversationError> {
        let event = match message {
            ConversationMessage::Cleartext(event) => event,
            ConversationMessage::Encrypted(_) => return Ok(Response::default()),
        };

        let content = match serde_json::from_value::<AuthChallengeContent>(event.content) {
            Ok(content) => content,
            Err(_) => return Ok(Response::default()),
        };

        log::debug!(
            "Received auth challenge from {}: {:?}",
            event.pubkey,
            content
        );

        if content.expires_at.as_u64() < nostr::Timestamp::now().as_u64() {
            log::warn!("Ignoring expired auth challenge");
            return Ok(Response::default());
        }

        let service_key = if let Some(subkey_proof) = &content.subkey_proof {
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

    fn is_expired(&self) -> bool {
        self.expires_at > SystemTime::now()
    }
}

pub struct AuthResponseConversation {
    event: AuthChallengeEvent,
    granted_permissions: Vec<String>,
    subkey_proof: Option<SubkeyProof>,
}

impl AuthResponseConversation {
    pub fn new(
        event: AuthChallengeEvent,
        granted_permissions: Vec<String>,
        subkey_proof: Option<SubkeyProof>,
    ) -> Self {
        Self {
            event,
            granted_permissions,
            subkey_proof,
        }
    }
}

impl Conversation for AuthResponseConversation {
    fn init(&mut self) -> Result<Response, ConversationError> {
        let content = AuthResponseContent {
            challenge: self.event.challenge.clone(),
            granted_permissions: self.granted_permissions.clone(),
            session_token: "randomlygeneratedtoken".to_string(),
            subkey_proof: self.subkey_proof.clone(),
        };

        let mut keys = HashSet::new();
        keys.insert(self.event.service_key);
        keys.insert(self.event.recipient);

        let tags = keys.iter().map(|k| Tag::public_key(*k.deref())).collect();
        let response = Response::new()
            .reply_to(
                self.event.recipient.into(),
                Kind::from(AUTH_RESPONSE),
                tags,
                content,
            )
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
