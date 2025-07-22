use std::{collections::HashSet, ops::Deref};

use nostr::{
    event::{Kind, Tag},
    filter::Filter,
    key::PublicKey,
};

use crate::{
    protocol::model::{
        auth::SubkeyProof,
        event_kinds::{CASHU_REQUEST, CASHU_RESPONSE},
        payment::{CashuRequestContent, CashuRequestContentWithKey, CashuResponseContent},
    },
    router::{
        ConversationError, MultiKeyListener, MultiKeyListenerAdapter, MultiKeySender,
        MultiKeySenderAdapter, Response,
        adapters::{ConversationWithNotification, one_shot::OneShotSender},
    },
};

/// Sender conversation to request a Cashu token.
///
/// Notifies the receiver with a [`CashuResponseContent`] event.
#[derive(derive_new::new)]
pub struct CashuRequestSenderConversation {
    local_key: PublicKey,
    subkey_proof: Option<SubkeyProof>,

    content: CashuRequestContent,
}

impl MultiKeySender for CashuRequestSenderConversation {
    const VALIDITY_SECONDS: Option<u64> = Some(60 * 5);

    type Error = ConversationError;
    type Message = CashuResponseContent;

    fn get_filter(
        state: &crate::router::MultiKeySenderAdapter<Self>,
    ) -> Result<Filter, Self::Error> {
        let mut filter = Filter::new()
            .kinds(vec![Kind::Custom(CASHU_RESPONSE)])
            .authors(state.subkeys.iter().chain([&state.user]).cloned())
            .pubkey(state.local_key);

        if let Some(subkey_proof) = &state.subkey_proof {
            filter = filter.pubkey(subkey_proof.main_key.into());
        }

        Ok(filter)
    }

    fn build_initial_message(
        state: &mut crate::router::MultiKeySenderAdapter<Self>,
        new_key: Option<PublicKey>,
    ) -> Result<Response, Self::Error> {
        let tags = state
            .subkeys
            .iter()
            .chain([&state.user])
            .map(|k| Tag::public_key(*k))
            .collect();

        if let Some(new_key) = new_key {
            Ok(Response::new().subscribe_to_subkey_proofs().reply_to(
                new_key,
                Kind::Custom(CASHU_REQUEST),
                tags,
                state.content.clone(),
            ))
        } else {
            Ok(Response::new().subscribe_to_subkey_proofs().reply_all(
                Kind::Custom(CASHU_REQUEST),
                tags,
                state.content.clone(),
            ))
        }
    }

    fn on_message(
        state: &mut crate::router::MultiKeySenderAdapter<Self>,
        _event: &crate::router::CleartextEvent,
        message: &Self::Message,
    ) -> Result<Response, Self::Error> {
        if message.request.inner.request_id == state.content.request_id {
            Ok(Response::new().notify(message.clone()).finish())
        } else {
            Ok(Response::default())
        }
    }
}

impl ConversationWithNotification for MultiKeySenderAdapter<CashuRequestSenderConversation> {
    type Notification = CashuResponseContent;
}

/// Receiver conversation to receive a [`CashuRequestContent`].
///
/// Notifies the sender with a [`CashuRequestContentWithKey`] event.
#[derive(derive_new::new)]
pub struct CashuRequestReceiverConversation {
    local_key: PublicKey,
}

impl MultiKeyListener for CashuRequestReceiverConversation {
    const VALIDITY_SECONDS: Option<u64> = Some(60 * 5);

    type Error = ConversationError;
    type Message = CashuRequestContent;

    fn init(state: &crate::router::MultiKeyListenerAdapter<Self>) -> Result<Response, Self::Error> {
        let mut filter = Filter::new()
            .kinds(vec![Kind::Custom(CASHU_REQUEST)])
            .pubkey(state.local_key);

        if let Some(subkey_proof) = &state.subkey_proof {
            filter = filter.pubkey(subkey_proof.main_key.into());
        }

        Ok(Response::new().filter(filter))
    }

    fn on_message(
        state: &mut crate::router::MultiKeyListenerAdapter<Self>,
        event: &crate::router::CleartextEvent,
        message: &Self::Message,
    ) -> Result<Response, Self::Error> {
        let sender_key = if let Some(subkey_proof) = state.subkey_proof.clone() {
            if let Err(e) = subkey_proof.verify(&event.pubkey) {
                return Ok(Response::default());
            }

            subkey_proof.main_key
        } else {
            event.pubkey.into()
        };

        let res = CashuRequestContentWithKey {
            inner: message.clone(),
            main_key: sender_key,
            recipient: event.pubkey.into(),
        };

        Ok(Response::new().notify(res))
    }
}

impl ConversationWithNotification for MultiKeyListenerAdapter<CashuRequestReceiverConversation> {
    type Notification = CashuRequestContentWithKey;
}

/// Sender conversation to send a Cashu token.
#[derive(derive_new::new)]
pub struct CashuResponseSenderConversation {
    content: CashuResponseContent,
}

impl OneShotSender for CashuResponseSenderConversation {
    type Error = ConversationError;

    fn send(
        state: &mut crate::router::adapters::one_shot::OneShotSenderAdapter<Self>,
    ) -> Result<Response, Self::Error> {
        let mut keys = HashSet::new();
        keys.insert(state.content.request.recipient);
        keys.insert(state.content.request.main_key);

        let tags = keys.iter().map(|k| Tag::public_key(*k.deref())).collect();
        let response = Response::new()
            .reply_to(
                state.content.request.recipient.into(),
                Kind::from(CASHU_RESPONSE),
                tags,
                state.content.clone(),
            )
            .finish();

        Ok(response)
    }
}
