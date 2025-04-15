use std::collections::HashSet;

use nostr::{
    event::{Event, Kind, Tag},
    key::PublicKey,
};

use crate::model::{
        auth::{AuthChallengeContent, AuthInitContent, SubkeyProof},
        event_kinds::*,
    };

use super::*;

pub struct AuthRequest {
    content: AuthChallengeContent,
    clients_online: HashSet<PublicKey>,
}

impl ServiceRequestInner for AuthRequest {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Args = AuthChallengeContent;
    type Error = AuthError;

    fn new(args: Self::Args) -> Result<Self, Self::Error> {
        let s = Self {
            content: args.clone(),
            clients_online: HashSet::new(),
        };

        Ok(s)
    }

    fn init(
        state: &mut ServiceRequest<Self>,
        response: &mut ResponseBuilder,
    ) -> Result<(), Self::Error> {
        response.filter(Filter::new().kinds([Kind::from(AUTH_RESPONSE), Kind::from(SUBKEY_PROOF)]).limit(0));

        response.reply_all(Kind::from(AUTH_CHALLENGE), state.get_involved_keys().iter().map(|k| Tag::public_key(*k)).collect(), state.content.clone());

        Ok(())
    }

    fn on_encrypted_message(
        _state: &mut ServiceRequest<Self>,
        _event: &Event,
        _response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        Ok(ConversationState::Continue)
    }

    fn on_message(
        state: &mut ServiceRequest<Self>,
        event: &CleartextEvent,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        if event.kind == Kind::from(SUBKEY_PROOF)
            && !state.clients_online.contains(&event.pubkey)
            && state.clients_online.len() < MAX_CLIENTS
        {
            // Verify the subkey proof
            let proof = serde_json::from_value::<SubkeyProof>(event.content.clone())
                .map_err(|_| "Invalid model")
                .and_then(|proof| {
                    if event.pubkey == state.user || proof.main_key == state.user {
                        Ok(proof)
                    } else {
                        Err("Invalid key")
                    }
                })
                .and_then(|proof| {
                    proof
                        .verify(&event.pubkey)
                        .map_err(|_| "Invalid proof")
                        .map(|_| proof)
                });

            let proof = match proof {
                Ok(proof) => proof,
                Err(_) => {
                    // TODO: return error
                    return Ok(ConversationState::Continue);
                }
            };

            // TODO: check subkey permissions

            if state.user != proof.main_key {
                // The key we though was the user is actually a subkey, so switch here
                state.subkeys.push(state.user);
                state.user = proof.main_key;
            } else if !state.subkeys.contains(&event.pubkey) {
                // We already knew the main key but we didn't know about this subkey specifically
                state.subkeys.push(event.pubkey);
            }

            // Mark this client as online and resend request
            state.inner.clients_online.insert(event.pubkey);
            response.reply_to(
                event.pubkey,
                Kind::from(AUTH_CHALLENGE),
                state.get_involved_keys().iter().map(|k| Tag::public_key(*k)).collect(),
                state.inner.content.clone(),
            );
        } else if event.kind == Kind::from(AUTH_RESPONSE) {
            log::debug!("Got auth response: {:?}", event.content);

            return Ok(ConversationState::finish(event.pubkey, event.content.clone()));
        }

        Ok(ConversationState::Continue)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid subkey proof")]
    InvalidSubkeyProof,
}

pub struct AuthPing {
    token: String,
}

impl ServiceRequestInner for AuthPing {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Args = String;
    type Error = AuthPingError;

    fn new(args: Self::Args) -> Result<Self, Self::Error> {
        log::debug!("Initializing AuthPing with token: {}", args);

        let s = Self {
            token: args.clone(),
        };

        Ok(s)
    }

    fn init(_state: &mut ServiceRequest<Self>, response: &mut ResponseBuilder) -> Result<(), Self::Error> {
        response.filter(Filter::new().kinds([Kind::from(AUTH_INIT)]));
        Ok(())
    }

    fn on_message(
        state: &mut ServiceRequest<Self>,
        event: &CleartextEvent,
        _response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        log::debug!("Got auth init: {:?}", event);

        let content = match serde_json::from_value::<AuthInitContent>(event.content.clone()) {
            Ok(content) if content.token == state.token => content,
            Ok(content) => {
                log::warn!("Token didn't match ({} != {}), continuing", content.token, state.token);
                return Ok(ConversationState::Continue);
            }
            Err(e) => {
                log::error!("Error parsing auth init content: {}", e);
                return Ok(ConversationState::Continue);
            }
        };

        Ok(ConversationState::finish(event.pubkey, content))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthPingError {}

