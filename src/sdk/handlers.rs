use nostr::{Filter, event::Kind};

use crate::{protocol::model::{auth::AuthInitContent, event_kinds::*}, router::{ConversationError, MultiKeyTrait, Response}};

pub struct AuthInitReceiverConversation {
    token: String,
}

impl AuthInitReceiverConversation {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

impl MultiKeyTrait for AuthInitReceiverConversation {
    const VALIDITY_SECONDS: u64 = 60 * 5;

    type Error = ConversationError;
    type Message = AuthInitContent;

    fn init(_state: &crate::router::MultiKeyProxy<Self>) -> Result<Response, Self::Error> {
        Ok(Response::new().filter(Filter::new().kinds(vec![Kind::from(AUTH_INIT)])))
    }

    fn on_message(
        _state: &mut crate::router::MultiKeyProxy<Self>,
        _event: &crate::router::CleartextEvent,
        message: &Self::Message,
    ) -> Result<Response, Self::Error> {
        if message.token == _state.token {
            return Ok(Response::new().notify(serde_json::json!({
                "token": _state.token,
            })));
        }

        Ok(Response::default())
    }
}