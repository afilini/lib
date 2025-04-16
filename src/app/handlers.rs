use std::ops::Deref;

use nostr::{event::Kind, Tag};

use crate::{
    protocol::{
        auth_init::AuthInitUrl,
        model::{auth::{AuthInitContent, ClientInfo}, event_kinds::AUTH_INIT},
    },
    router::{Conversation, ConversationMessage, Response},
};

pub struct AuthInitConversation {
    pub url: AuthInitUrl,
    pub relays: Vec<String>,
}

impl Conversation for AuthInitConversation {
    fn init(&self) -> Result<Response, crate::router::ConversationError> {
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

    fn on_message(
        &mut self,
        _message: ConversationMessage,
    ) -> Result<Response, crate::router::ConversationError> {
        Ok(Response::default())
    }

    fn is_expired(&self) -> bool {
        false
    }
}