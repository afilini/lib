use nostr::{event::Kind, filter::Filter, key::PublicKey};
use serde::{Deserialize, Serialize};

use crate::{
    protocol::model::{
        Timestamp,
        auth::SubkeyProof,
        bindings,
        event_kinds::{PAYMENT_REQUEST, RECURRING_PAYMENT_REQUEST},
        payment::{RecurringPaymentRequestContent, SinglePaymentRequestContent},
    },
    router::{
        ConversationError, MultiKeyListener, MultiKeyListenerAdapter, Response,
        adapters::ConversationWithNotification,
    },
};

pub struct PaymentRequestListenerConversation {
    local_key: PublicKey,
}

impl PaymentRequestListenerConversation {
    pub fn new(local_key: PublicKey) -> Self {
        Self { local_key }
    }
}

impl MultiKeyListener for PaymentRequestListenerConversation {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Error = ConversationError;
    type Message = PaymentRequestContent;

    fn init(state: &crate::router::MultiKeyListenerAdapter<Self>) -> Result<Response, Self::Error> {
        let mut filter = Filter::new()
            .kinds(vec![
                Kind::Custom(PAYMENT_REQUEST),
                Kind::Custom(RECURRING_PAYMENT_REQUEST),
            ])
            .pubkey(state.local_key);

        if let Some(subkey_proof) = &state.subkey_proof {
            filter = filter.pubkey(subkey_proof.main_key.into());
        }

        Ok(Response::new().filter(filter))
    }

    fn on_message(
        _state: &mut crate::router::MultiKeyListenerAdapter<Self>,
        event: &crate::router::CleartextEvent,
        content: &Self::Message,
    ) -> Result<Response, Self::Error> {
        log::debug!(
            "Received payment request from {}: {:?}",
            event.pubkey,
            content
        );

        if content.expires_at().as_u64() < nostr::Timestamp::now().as_u64() {
            log::warn!("Ignoring expired auth challenge");
            return Ok(Response::default());
        }

        let service_key = if let Some(subkey_proof) = content.subkey_proof() {
            if let Err(e) = subkey_proof.verify(&event.pubkey) {
                log::warn!("Ignoring request with invalid subkey proof: {}", e);
                return Ok(Response::default());
            }

            subkey_proof.main_key
        } else {
            event.pubkey.into()
        };

        let response = Response::new().notify(PaymentRequestEvent {
            service_key,
            recipient: event.pubkey.into(),
            expires_at: content.expires_at(),
            content: content.clone(),
        });

        Ok(response)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(uniffi::Record))]
pub struct PaymentRequestEvent {
    pub service_key: bindings::PublicKey,
    pub recipient: bindings::PublicKey,
    pub expires_at: Timestamp,
    pub content: PaymentRequestContent,
}

impl ConversationWithNotification for MultiKeyListenerAdapter<PaymentRequestListenerConversation> {
    type Notification = PaymentRequestEvent;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "payment_type")]
#[cfg_attr(feature = "bindings", derive(uniffi::Enum))]
pub enum PaymentRequestContent {
    Single(SinglePaymentRequestContent),
    Recurring(RecurringPaymentRequestContent),
}

impl PaymentRequestContent {
    pub fn expires_at(&self) -> Timestamp {
        match self {
            Self::Single(content) => content.expires_at,
            Self::Recurring(content) => content.expires_at,
        }
    }

    pub fn subkey_proof(&self) -> Option<&SubkeyProof> {
        match self {
            Self::Single(content) => content.subkey_proof.as_ref(),
            Self::Recurring(content) => content.subkey_proof.as_ref(),
        }
    }
}
