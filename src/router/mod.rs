use std::{
    collections::{HashMap, HashSet},
    ops::Deref,
    time::{Duration, SystemTime},
};

use connector::DelayedReply;
use futures::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::mpsc;

use nostr::{
    event::{Event, EventId, Kind, Tags},
    filter::Filter,
    key::PublicKey,
    message::SubscriptionId,
};

use crate::utils::random_string;

pub mod app;
pub mod connector;
pub mod sdk;

const MAX_CLIENTS: usize = 8;

// TODO: remove expired conversations
// TODO: update expiry at every message
// TODO: remove subscribers when conversation is removed

pub struct MessageRouter {
    conversations: HashMap<String, Box<dyn ConversationTrait + Send>>,
    subscribers: HashMap<String, Vec<mpsc::Sender<WrappedContent<serde_json::Value>>>>,
    outgoing_queue: mpsc::UnboundedSender<RelayAction>,
}

impl MessageRouter {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<RelayAction>) {
        let (tx, rx) = mpsc::unbounded_channel();

        (
            Self {
                conversations: HashMap::new(),
                subscribers: HashMap::new(),
                outgoing_queue: tx,
            },
            rx,
        )
    }

    // fn get_relevant_conversations(
    //     &self,
    //     event_id: &EventId,
    //     kind: &Kind,
    //     pubkey: &PublicKey,
    //     tags: &Tags,
    // ) -> HashSet<String> {
    //     // TODO: remove cloned

    //     let mut conversations = HashSet::new();

    //     if let Some(evs) = self.triggers.get(&Trigger::Event(*event_id)) {
    //         conversations.extend(evs.iter().cloned());
    //     }
    //     if let Some(evs) = self.triggers.get(&Trigger::Kind(*kind)) {
    //         conversations.extend(evs.iter().cloned());
    //     }
    //     if let Some(evs) = self.triggers.get(&Trigger::FromPubkey(*pubkey)) {
    //         conversations.extend(evs.iter().cloned());
    //     }
    //     for pk in tags.public_keys() {
    //         if let Some(evs) = self.triggers.get(&Trigger::TagPubkey(*pk)) {
    //             conversations.extend(evs.iter().cloned());
    //         }
    //     }

    //     conversations
    // }

    pub async fn on_message(
        &mut self,
        event: &CleartextEvent,
        conversation: &SubscriptionId,
    ) -> Result<(), ConversationError> {
        log::trace!(
            "Dispatching message of kind {:?} for {:?}",
            event.kind,
            conversation
        );
        log::trace!("Current conversations = {:?}", self.conversations.keys());

        let conversation = conversation.as_str();
        let mut response = ResponseBuilder::new();

        if let Some(conv) = self.conversations.get_mut(conversation) {
            match conv.on_message(event, &mut response)? {
                ConversationState::Finished(content) => {
                    if let Some(subscribers) = self.subscribers.get(conversation) {
                        for sub in subscribers.iter() {
                            sub.send(content.clone()).await.unwrap();
                        }
                    }

                    self.conversations.remove(conversation);
                    self.subscribers.remove(conversation);
                    // TODO: send out message to remove filters on relays
                }
                ConversationState::Continue => {}
            }
        }

        self.process_response_builder(conversation, response);

        Ok(())
    }

    pub async fn on_encrypted_message(&mut self, _event: &Event) -> Result<(), ConversationError> {
        Ok(())
    }

    fn process_response_builder(&mut self, id: &str, response: ResponseBuilder) {
        log::trace!("Processing response builder for {} = {:?}", id, response);

        if !response.filter.is_empty() {
            self.outgoing_queue
                .send(RelayAction::ApplyFilter(id.to_string(), response.filter))
                .expect("Queue should always be available");
        }

        for (pubkey, (kind, tags, content)) in response.responses.iter() {
            let keys = match pubkey {
                Some(pubkey) => vec![pubkey.clone()],
                None => self
                    .conversations
                    .get(id)
                    .map(|c| c.get_involved_keys())
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            };

            log::trace!("Sending event of kind {:?} to {:?}", kind, keys);

            for pubkey in keys {
                self.outgoing_queue
                    .send(RelayAction::SendEvent(
                        pubkey,
                        OutgoingEvent {
                            kind: kind.clone(),
                            content: content.clone(),
                            encrypted: true, // TODO
                            tags: tags.clone(),
                        },
                    ))
                    .expect("Queue should always be available");
            }
        }
    }

    pub fn new_service_request<Inner: ServiceRequestInner>(
        &mut self,
        user: PublicKey,
        subkeys: Vec<PublicKey>,
        args: Inner::Args,
    ) -> Result<String, Inner::Error> {
        let id = random_string(16);
        let mut response = ResponseBuilder::new();

        let req = ServiceRequest::<Inner>::new(user, subkeys, args, &mut response)?;
        self.conversations.insert(id.clone(), Box::new(req));

        self.process_response_builder(&id, response);

        Ok(id)
    }

    pub fn subscribe_to_service_request<T: DeserializeOwned + Serialize>(
        &mut self,
        id: String,
    ) -> Result<DelayedReply<T>, ConversationError> {
        let (tx, rx) = mpsc::channel(8);
        self.subscribers.entry(id).or_insert(Vec::new()).push(tx);

        let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
        let rx = rx.map(|content| WrappedContent::map(content));
        let rx = DelayedReply::new(rx);

        Ok(rx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    Event(EventId),
    Kind(Kind),
    FromPubkey(PublicKey),
    TagPubkey(PublicKey),
}

impl From<Trigger> for Filter {
    fn from(trigger: Trigger) -> Self {
        match trigger {
            Trigger::Event(event_id) => Filter::new().event(event_id),
            Trigger::Kind(kind) => Filter::new().kinds([kind]),
            Trigger::FromPubkey(pubkey) => Filter::new().author(pubkey),
            Trigger::TagPubkey(pubkey) => Filter::new().pubkey(pubkey),
        }
    }
}

#[derive(Debug)]
pub enum RelayAction {
    ApplyFilter(String, Filter),

    SendEvent(PublicKey, OutgoingEvent),
}

#[derive(Debug)]
// TODO: we should select individual relays for each event
pub struct OutgoingEvent {
    pub kind: Kind,
    pub content: serde_json::Value,
    pub encrypted: bool,
    pub tags: Tags,
}

#[derive(Debug)]
pub struct ResponseBuilder {
    filter: Filter,
    responses: HashMap<Option<PublicKey>, (Kind, Tags, serde_json::Value)>,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        Self {
            filter: Filter::new(),
            responses: HashMap::new(),
        }
    }

    pub fn filter(&mut self, filter: Filter) -> &mut Self {
        self.filter = filter;
        self
    }

    pub fn reply_all<S: serde::Serialize>(
        &mut self,
        kind: Kind,
        tags: Tags,
        content: S,
    ) -> &mut Self {
        let content = serde_json::to_value(&content).unwrap();
        self.responses.insert(None, (kind, tags, content));
        self
    }

    pub fn reply_to<S: serde::Serialize>(
        &mut self,
        pubkey: PublicKey,
        kind: Kind,
        tags: Tags,
        content: S,
    ) -> &mut Self {
        let content = serde_json::to_value(&content).unwrap();
        self.responses.insert(Some(pubkey), (kind, tags, content));
        self
    }
}

pub trait ConversationTrait {
    fn on_encrypted_message(
        &mut self,
        event: &Event,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, ConversationError>;
    fn on_message(
        &mut self,
        event: &CleartextEvent,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, ConversationError>;
    fn is_expired(&self) -> bool;
    fn get_involved_keys(&self) -> HashSet<PublicKey>;
}

impl<T: ServiceRequestInner> ConversationTrait for ServiceRequest<T> {
    fn on_encrypted_message(
        &mut self,
        event: &Event,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, ConversationError> {
        self.on_encrypted_message(event, response)
            .map_err(|e| ConversationError::Inner(Box::new(e)))
    }

    fn on_message(
        &mut self,
        event: &CleartextEvent,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, ConversationError> {
        self.on_message(event, response)
            .map_err(|e| ConversationError::Inner(Box::new(e)))
    }

    fn is_expired(&self) -> bool {
        self.expires_at < SystemTime::now()
    }

    fn get_involved_keys(&self) -> HashSet<PublicKey> {
        self.subkeys
            .iter()
            .chain(std::iter::once(&self.user))
            .cloned()
            .collect()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConversationError {
    #[error("Inner error: {0}")]
    Inner(Box<dyn std::error::Error + Send + Sync>),
}

pub struct ServiceRequest<Inner> {
    pub user: PublicKey,
    pub subkeys: Vec<PublicKey>,
    pub expires_at: SystemTime,
    pub inner: Inner,
}

impl<Inner: ServiceRequestInner> Deref for ServiceRequest<Inner> {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Inner: ServiceRequestInner> ServiceRequest<Inner> {
    pub fn new(
        user: PublicKey,
        subkeys: Vec<PublicKey>,
        args: Inner::Args,
        response: &mut ResponseBuilder,
    ) -> Result<Self, Inner::Error> {
        let inner = Inner::new(args)?;
        let expires_at = SystemTime::now() + Duration::from_secs(Inner::VALIDITY_SECONDS);
        let mut s = Self {
            user,
            subkeys,
            expires_at,
            inner,
        };

        Inner::init(&mut s, response)?;

        Ok(s)
    }

    pub fn on_encrypted_message(
        &mut self,
        event: &Event,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Inner::Error> {
        Inner::on_encrypted_message(self, event, response)
    }

    pub fn on_message(
        &mut self,
        event: &CleartextEvent,
        response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Inner::Error> {
        Inner::on_message(self, event, response)
    }
}

#[derive(Debug, Clone)]
pub struct CleartextEvent {
    pub id: EventId,
    pub pubkey: PublicKey,
    pub created_at: nostr::types::Timestamp,
    pub kind: Kind,
    pub tags: Tags,
    pub content: serde_json::Value,
}

impl CleartextEvent {
    pub fn new(event: &Event, decrypted: &str) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: event.id,
            pubkey: event.pubkey,
            created_at: event.created_at,
            kind: event.kind,
            tags: event.tags.clone(),
            content: serde_json::from_str(decrypted)?,
        })
    }
}

pub trait ServiceRequestInner: Sized + Send + 'static {
    const VALIDITY_SECONDS: u64;

    type Error: std::error::Error + Send + Sync + 'static;
    type Args;

    fn new(args: Self::Args) -> Result<Self, Self::Error>;

    fn init(
        _state: &mut ServiceRequest<Self>,
        _response: &mut ResponseBuilder,
    ) -> Result<(), Self::Error> {
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
        _state: &mut ServiceRequest<Self>,
        _event: &CleartextEvent,
        _response: &mut ResponseBuilder,
    ) -> Result<ConversationState, Self::Error> {
        Ok(ConversationState::Continue)
    }
}

#[derive(Debug, Clone)]
pub enum ConversationState {
    Continue,
    Finished(WrappedContent<serde_json::Value>),
}

impl ConversationState {
    fn finish<T: Serialize>(pubkey: PublicKey, content: T) -> Self {
        ConversationState::Finished(WrappedContent::new(
            pubkey,
            serde_json::to_value(&content).unwrap(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedContent<T: Serialize> {
    pub pubkey: PublicKey,
    pub content: T,
}

impl WrappedContent<serde_json::Value> {
    pub fn new(pubkey: PublicKey, content: serde_json::Value) -> Self {
        Self { pubkey, content }
    }
}

impl<T: DeserializeOwned + Serialize> WrappedContent<T> {
    pub fn map(s: WrappedContent<serde_json::Value>) -> Result<Self, serde_json::Error> {
        let content = serde_json::from_value(s.content)?;
        Ok(Self {
            pubkey: s.pubkey,
            content,
        })
    }
}
