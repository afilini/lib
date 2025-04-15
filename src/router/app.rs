use nostr::event::{Event, Kind, Tag};

use crate::{
    app::AuthRequestData,
    model::{
        auth::{AuthChallengeContent, AuthInitContent, AuthResponseContent, SubkeyProof},
        event_kinds::*,
    },
};

use super::*;

pub struct SendAuthInit(AuthInitContent);

impl ServiceRequestInner for SendAuthInit {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Args = AuthInitContent;
    type Error = EmptyError;

    fn new(args: Self::Args) -> Result<Self, Self::Error> {
        Ok(Self(args))
    }

    fn init(
        state: &mut ServiceRequest<Self>,
        response: &mut ResponseBuilder,
    ) -> Result<(), Self::Error> {
        let tags: Tags = state
            .get_involved_keys()
            .iter()
            .map(|k| Tag::public_key(*k))
            .collect();
        response.reply_all(Kind::from(AUTH_INIT), tags, state.inner.0.clone());
        Ok(())
    }
}

pub struct AuthListener {
    subkey_proof: Option<SubkeyProof>,
}

impl ServiceRequestInner for AuthListener {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Args = Option<SubkeyProof>;
    type Error = EmptyError;

    fn new(args: Self::Args) -> Result<Self, Self::Error> {
        let s = Self { subkey_proof: args };

        Ok(s)
    }

    fn init(
        _state: &mut ServiceRequest<Self>,
        response: &mut ResponseBuilder,
    ) -> Result<(), Self::Error> {
        response.filter(Filter::new().kinds([Kind::from(AUTH_CHALLENGE)]).limit(0));
        Ok(())
    }

    fn on_message(
        _state: &mut ServiceRequest<Self>,
        event: &CleartextEvent,
        _response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        log::trace!("Entering on_message for AuthListener");

        match serde_json::from_value::<AuthChallengeContent>(event.content.clone()) {
            Ok(content) => {
                log::debug!(
                    "Received auth challenge from {}: {:?}",
                    event.pubkey,
                    content
                );

                // TODO: verify subkey proof

                Ok(ConversationState::finish(
                    event.pubkey,
                    AuthRequestData::from((content, event.id, event.pubkey)),
                ))
            }
            _ => Ok(ConversationState::Continue),
        }
    }

    fn on_encrypted_message(
        _state: &mut ServiceRequest<Self>,
        _event: &Event,
        _response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        // TODO: manage requests to other subkeys

        Ok(ConversationState::Continue)
    }
}

pub struct AuthChallengeResponse {
    status: bool,
    token: String,
    subkey_proof: Option<SubkeyProof>,
    event_id: EventId,
}

impl ServiceRequestInner for AuthChallengeResponse {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Args = (bool, String, Option<SubkeyProof>, EventId);
    type Error = EmptyError;

    fn new(args: Self::Args) -> Result<Self, Self::Error> {
        Ok(Self {
            status: args.0,
            token: args.1,
            subkey_proof: args.2,
            event_id: args.3,
        })
    }

    fn init(
        state: &mut ServiceRequest<Self>,
        response: &mut ResponseBuilder,
    ) -> Result<(), Self::Error> {
        let mut tags: Tags = state
            .get_involved_keys()
            .iter()
            .map(|k| Tag::public_key(*k))
            .collect();
        tags.push(Tag::event(state.inner.event_id));

        log::trace!("Tags: {:?}", tags);

        // TODO: implement negative status

        response.reply_all(
            Kind::from(AUTH_RESPONSE),
            tags,
            AuthResponseContent {
                granted_permissions: vec![],
                session_token: state.inner.token.clone(),
                subkey_proof: state.inner.subkey_proof.clone(),
            },
        );

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmptyError {}
