use nostr::{event::{Kind, Tag}, key::PublicKey, Filter};
use serde::{Deserialize, Serialize};

use crate::{
    protocol::model::{
        auth::{AuthChallengeContent, AuthInitContent, AuthResponseContent, SubkeyProof},
        event_kinds::*, Timestamp,
    },
    router::{ConversationError, MultiKeyTrait, Response},
    utils::random_string,
};

pub struct AuthInitReceiverConversation {
    local_key: PublicKey,
    token: String,
}

impl AuthInitReceiverConversation {
    pub fn new(local_key: PublicKey, token: String) -> Self {
        Self { local_key, token }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInitEvent {
    pub main_key: PublicKey,
}

impl MultiKeyTrait for AuthInitReceiverConversation {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Error = ConversationError;
    type Message = AuthInitContent;

    fn init(state: &crate::router::MultiKeyProxy<Self>) -> Result<Response, Self::Error> {
        // TODO: also listen for messages to the main key if we are a subkey
        Ok(Response::new().filter(
            Filter::new()
                .kinds(vec![Kind::from(AUTH_INIT)])
                .pubkey(state.local_key),
        ))
    }

    fn on_message(
        _state: &mut crate::router::MultiKeyProxy<Self>,
        event: &crate::router::CleartextEvent,
        message: &Self::Message,
    ) -> Result<Response, Self::Error> {
        if message.token == _state.token {
            return Ok(Response::new().notify(AuthInitEvent {
                main_key: event.pubkey,
            }));
        }

        Ok(Response::default())
    }
}

pub struct AuthChallengeSenderConversation {
    recipient: PublicKey,
    recipient_subkeys: Vec<PublicKey>,

    local_key: PublicKey,
    subkey_proof: Option<SubkeyProof>,

    challenge: String,
}

impl AuthChallengeSenderConversation {
    pub fn new(
        recipient: PublicKey,
        recipient_subkeys: Vec<PublicKey>,
        local_key: PublicKey,
        subkey_proof: Option<SubkeyProof>,
    ) -> Self {
        Self {
            recipient,
            recipient_subkeys,
            local_key,
            subkey_proof,
            challenge: random_string(32),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponseEvent {
    pub user_key: PublicKey,
    pub recipient: PublicKey,
    pub challenge: String,
    pub granted_permissions: Vec<String>,
    pub session_token: String,
}

impl MultiKeyTrait for AuthChallengeSenderConversation {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Error = ConversationError;
    type Message = AuthResponseContent;

    fn init(state: &crate::router::MultiKeyProxy<Self>) -> Result<Response, Self::Error> {
        let mut filter = Filter::new()
            .kinds(vec![Kind::from(AUTH_RESPONSE)])
            .authors(
                state
                    .recipient_subkeys
                    .iter()
                    .chain([&state.recipient])
                    .cloned(),
            )
            .pubkey(state.local_key);

        if let Some(subkey_proof) = &state.subkey_proof {
            filter = filter.pubkey(subkey_proof.main_key.into());
        }

        let content = AuthChallengeContent {
            challenge: state.challenge.clone(),
            expires_at: Timestamp::now_plus_seconds(60 * 5), // TODO: should take it from state.expires_at
            required_permissions: vec![],
            subkey_proof: state.subkey_proof.clone(),
        };

        let tags = state
            .recipient_subkeys
            .iter()
            .chain([&state.recipient])
            .map(|k| Tag::public_key(*k))
            .collect();

        let response = Response::new().filter(filter).reply_to(
            state.recipient,
            Kind::from(AUTH_CHALLENGE),
            tags,
            content,
        );

        Ok(response)
    }

    fn on_message(
        state: &mut crate::router::MultiKeyProxy<Self>,
        event: &crate::router::CleartextEvent,
        message: &Self::Message,
    ) -> Result<Response, Self::Error> {
        if message.challenge != state.challenge {
            log::warn!(
                "Ignoring response with invalid challenge: {}",
                message.challenge
            );
            return Ok(Response::default());
        }

        let user_key = if let Some(subkey_proof) = &message.subkey_proof {
            if let Err(e) = subkey_proof.verify(&state.local_key) {
                log::warn!("Ignoring response with invalid subkey proof: {}", e);
                return Ok(Response::default());
            }

            subkey_proof.main_key.into()
        } else {
            event.pubkey
        };

        Ok(Response::new().notify(AuthResponseEvent {
            user_key,
            recipient: event.pubkey.into(),
            challenge: message.challenge.clone(),
            granted_permissions: message.granted_permissions.clone(),
            session_token: message.session_token.clone(),
        }))
    }
}
