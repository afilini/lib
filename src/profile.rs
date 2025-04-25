use nostr::{event::Kind, filter::Filter, key::PublicKey, nips::nip01::Metadata};

use crate::{
    protocol::model::Timestamp,
    router::{adapters::{one_shot::OneShotSender, ConversationWithNotification}, Conversation, ConversationError, ConversationMessage, Response},
};

pub struct FetchProfileInfoConversation {
    pubkey: PublicKey,
    expires_at: Timestamp,
}

impl FetchProfileInfoConversation {
    pub fn new(pubkey: PublicKey) -> Self {
        Self {
            pubkey,
            expires_at: Timestamp::now_plus_seconds(1000),
        }
    }
}

impl Conversation for FetchProfileInfoConversation {
    fn init(&mut self) -> Result<crate::router::Response, crate::router::ConversationError> {
        Ok(Response::new().filter(
            Filter::new()
                .author(self.pubkey)
                .kind(Kind::Metadata)
                .limit(1),
        ))
    }

    fn on_message(
        &mut self,
        message: crate::router::ConversationMessage,
    ) -> Result<crate::router::Response, crate::router::ConversationError> {
        if let ConversationMessage::Cleartext(event) = message {
            let metadata: Result<Metadata, _> = serde_json::from_value(event.content);
            if let Ok(metadata) = metadata {
                return Ok(Response::new().notify(metadata).finish());
            }
        }

        Ok(Response::new())
    }

    fn is_expired(&self) -> bool {
        self.expires_at > Timestamp::now()
    }
}

impl ConversationWithNotification for FetchProfileInfoConversation {
    type Notification = Metadata;
}

pub struct SetProfileConversation {
    profile: Metadata,
}

impl SetProfileConversation {
    pub fn new(profile: Metadata) -> Self {
        Self { profile }
    }
}

impl OneShotSender for SetProfileConversation {
    type Error = ConversationError;

    fn send(state: &mut crate::router::adapters::one_shot::OneShotSenderAdapter<Self>) -> Result<Response, Self::Error> {
        Ok(Response::new().broadcast_unencrypted(Kind::Metadata, Default::default(), state.profile.clone()).finish())
    }
}