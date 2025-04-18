use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
};

use channel::Channel;
use futures::{Stream, StreamExt};
use nostr_relay_pool::RelayPoolNotification;
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{Mutex, mpsc};

use nostr::{
    event::{Event, EventBuilder, EventId, Kind, Tags},
    filter::Filter,
    key::PublicKey,
    message::{RelayMessage, SubscriptionId},
    nips::nip44,
};

use crate::{protocol::LocalKeypair, utils::random_string};

pub mod channel;
pub mod adapters;

pub use adapters::{MultiKeySender, MultiKeySenderAdapter};

const MAX_CLIENTS: usize = 8;

// TODO: update expiry at every message

pub struct MessageRouter<C: Channel> {
    channel: C,
    keypair: LocalKeypair,
    conversations: Mutex<HashMap<String, Box<dyn Conversation + Send>>>,
    subscribers: Mutex<HashMap<String, Vec<mpsc::Sender<serde_json::Value>>>>,
}

impl<C: Channel> MessageRouter<C> {
    pub fn new(channel: C, keypair: LocalKeypair) -> Self {
        Self {
            channel,
            keypair,
            conversations: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(HashMap::new()),
        }
    }

    async fn cleanup_conversation(&self, conversation: &str) -> Result<(), ConversationError> {
        // Remove conversation state
        self.conversations.lock().await.remove(conversation);
        self.subscribers.lock().await.remove(conversation);

        // Remove filters from relays
        self.channel
            .unsubscribe(conversation.to_string())
            .await
            .map_err(|e| ConversationError::Inner(Box::new(e)))?;

        Ok(())
    }

    pub async fn purge(&mut self) {
        self.conversations.lock().await.clear();
        self.subscribers.lock().await.clear();
    }

    pub async fn listen(&self) -> Result<(), ConversationError> {
        while let Ok(notification) = self.channel.receive().await {
            log::trace!("Notification = {:?}", notification);

            let (subscription_id, event): (SubscriptionId, Event) = match notification {
                RelayPoolNotification::Message {
                    message:
                        RelayMessage::Event {
                            subscription_id,
                            event,
                        },
                    ..
                } => (subscription_id.into_owned(), event.into_owned()),
                RelayPoolNotification::Event {
                    event,
                    subscription_id,
                    ..
                } => (subscription_id, *event),
                _ => continue,
            };

            if event.pubkey == self.keypair.public_key() {
                log::trace!("Ignoring event from self");
                continue;
            }

            if !event.verify_signature() {
                log::warn!("Invalid signature for event id: {:?}", event.id);
                continue;
            }

            log::trace!("Decrypting with key = {:?}", self.keypair.public_key());

            let message = if let Ok(content) =
                nip44::decrypt(&self.keypair.secret_key(), &event.pubkey, &event.content)
            {
                let cleartext = match CleartextEvent::new(&event, &content) {
                    Ok(cleartext) => cleartext,
                    Err(e) => {
                        log::warn!("Invalid JSON in event: {:?}", e);
                        continue;
                    }
                };

                log::trace!("Decrypted event: {:?}", cleartext);

                ConversationMessage::Cleartext(cleartext)
            } else {
                log::warn!("Failed to decrypt event: {:?}", event);
                ConversationMessage::Encrypted(event)
            };

            let conversation_id = subscription_id.as_str();
            let response = match self.conversations.lock().await.get_mut(conversation_id) {
                Some(conv) => match conv.on_message(message) {
                    Ok(response) => response,
                    Err(e) => {
                        log::warn!("Error in conversation id {:?}: {:?}", conversation_id, e);
                        Response::new().finish()
                    }
                },
                None => {
                    log::warn!("No conversation found for id: {:?}", conversation_id);
                    self.channel
                        .unsubscribe(conversation_id.to_string())
                        .await
                        .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                    continue;
                }
            };

            self.process_response(conversation_id, response).await?;
        }

        Ok(())
    }

    async fn process_response(
        &self,
        id: &str,
        response: Response,
    ) -> Result<(), ConversationError> {
        log::trace!("Processing response builder for {} = {:?}", id, response);

        if !response.filter.is_empty() {
            self.channel
                .subscribe(id.to_string(), response.filter)
                .await
                .map_err(|e| ConversationError::Inner(Box::new(e)))?;
        }

        for response in response.responses.iter() {
            log::trace!(
                "Sending event of kind {:?} to {:?}",
                response.kind,
                response.recepient_keys
            );

            for pubkey in response.recepient_keys.iter() {
                // TODO: we should allow non-encrypted messages

                let encrypted = nip44::encrypt(
                    &self.keypair.secret_key(),
                    &pubkey,
                    serde_json::to_string(&response.content)
                        .map_err(|e| ConversationError::Inner(Box::new(e)))?,
                    nip44::Version::V2,
                )
                .map_err(|e| ConversationError::Inner(Box::new(e)))?;
                let event = EventBuilder::new(response.kind, encrypted)
                    .tags(response.tags.clone())
                    .sign_with_keys(&self.keypair)
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                log::trace!("Encrypted event: {:?}", event);

                // TODO: should only send to selected relays
                self.channel
                    .broadcast(event)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
                // TODO: wait for confirmation from relays
            }
        }

        for notification in response.notifications.iter() {
            let mut lock = self.subscribers.lock().await;
            if let Some(senders) = lock.get_mut(id) {
                for sender in senders.iter_mut() {
                    let _ = sender.send(notification.clone()).await;
                }
            }
        }

        if response.finished {
            self.cleanup_conversation(id).await?;
        }

        Ok(())
    }

    pub async fn add_conversation(
        &self,
        mut conversation: Box<dyn Conversation + Send>,
    ) -> Result<String, ConversationError> {
        let conversation_id = random_string(32);

        let response = conversation.init()?;

        self.conversations
            .lock()
            .await
            .insert(conversation_id.clone(), conversation);

        self.process_response(&conversation_id, response).await?;

        Ok(conversation_id)
    }

    pub async fn subscribe_to_service_request<T: DeserializeOwned + Serialize>(
        &self,
        id: String,
    ) -> Result<DelayedReply<T>, ConversationError> {
        let (tx, rx) = mpsc::channel(8);
        self.subscribers
            .lock()
            .await
            .entry(id)
            .or_insert(Vec::new())
            .push(tx);

        let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
        let rx = rx.map(|content| serde_json::from_value(content));
        let rx = DelayedReply::new(rx);

        Ok(rx)
    }

    pub fn channel(&self) -> &C {
        &self.channel
    }

    pub fn keypair(&self) -> &LocalKeypair {
        &self.keypair
    }
}

#[derive(Debug)]
struct ResponseEntry {
    pub recepient_keys: Vec<PublicKey>,
    pub kind: Kind,
    pub tags: Tags,
    pub content: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct Response {
    filter: Filter,
    responses: Vec<ResponseEntry>,
    notifications: Vec<serde_json::Value>,
    finished: bool,
}

impl Response {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    pub fn reply_all<S: serde::Serialize>(mut self, kind: Kind, tags: Tags, content: S) -> Self {
        let content = serde_json::to_value(&content).unwrap();
        self.responses.push(ResponseEntry {
            recepient_keys: vec![],
            kind,
            tags,
            content,
        });
        self
    }

    pub fn reply_to<S: serde::Serialize>(
        mut self,
        pubkey: PublicKey,
        kind: Kind,
        tags: Tags,
        content: S,
    ) -> Self {
        let content = serde_json::to_value(&content).unwrap();
        self.responses.push(ResponseEntry {
            recepient_keys: vec![pubkey],
            kind,
            tags,
            content,
        });
        self
    }

    pub fn notify<S: serde::Serialize>(mut self, data: S) -> Self {
        let content = serde_json::to_value(&data).unwrap();
        self.notifications.push(content);
        self
    }

    pub fn finish(mut self) -> Self {
        self.finished = true;
        self
    }

    fn set_recepient_keys(&mut self, user: PublicKey, subkeys: &HashSet<PublicKey>) {
        for response in &mut self.responses {
            if response.recepient_keys.is_empty() {
                response.recepient_keys.push(user);
                response.recepient_keys.extend(subkeys.iter().cloned());
            }
        }
    }

    fn extend_responses(&mut self, response: Response) {
        self.responses.extend(response.responses);
    }
}

#[derive(Debug)]
pub enum ConversationMessage {
    Cleartext(CleartextEvent),
    Encrypted(Event),
}

#[derive(thiserror::Error, Debug)]
pub enum ConversationError {
    #[error("Encrypted messages not supported")]
    Encrypted,

    #[error("User not set")]
    UserNotSet,

    #[error("Inner error: {0}")]
    Inner(Box<dyn std::error::Error + Send + Sync>),
}

pub trait Conversation {
    fn on_message(&mut self, message: ConversationMessage) -> Result<Response, ConversationError>;
    fn is_expired(&self) -> bool;
    fn init(&mut self) -> Result<Response, ConversationError> {
        Ok(Response::default())
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


pub trait InnerDelayedReply<T: Serialize>:
    Stream<Item = Result<T, serde_json::Error>> + Send + Unpin + 'static
{
}
impl<S, T: Serialize> InnerDelayedReply<T> for S where
    S: Stream<Item = Result<T, serde_json::Error>> + Send + Unpin + 'static
{
}

pub struct DelayedReply<T: Serialize> {
    stream: Box<dyn InnerDelayedReply<T>>,
}

impl<T: Serialize> DelayedReply<T> {
    pub fn new(stream: impl InnerDelayedReply<T>) -> Self {
        Self {
            stream: Box::new(stream),
        }
    }

    pub async fn await_reply(&mut self) -> Option<Result<T, serde_json::Error>> {
        use futures::StreamExt;

        self.stream.next().await
    }
}

impl<T: Serialize> Deref for DelayedReply<T> {
    type Target = Box<dyn InnerDelayedReply<T>>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl<T: Serialize> DerefMut for DelayedReply<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}