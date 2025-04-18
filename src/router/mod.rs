use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
};

use adapters::ConversationWithNotification;
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

pub use adapters::multi_key_sender::{MultiKeySender, MultiKeySenderAdapter};
pub use adapters::multi_key_listener::{MultiKeyListener, MultiKeyListenerAdapter};

// TODO: update expiry at every message

/// A router that manages conversations over a Nostr channel.
///
/// The `MessageRouter` is responsible for:
/// - Managing conversations and their lifecycle
/// - Routing incoming messages to the appropriate conversations
/// - Broadcasting outgoing messages to the network
/// - Managing subscriptions to conversation notifications
pub struct MessageRouter<C: Channel> {
    channel: C,
    keypair: LocalKeypair,
    conversations: Mutex<HashMap<String, Box<dyn Conversation + Send>>>,
    subscribers: Mutex<HashMap<String, Vec<mpsc::Sender<serde_json::Value>>>>,
}

impl<C: Channel> MessageRouter<C> {
    /// Creates a new `MessageRouter` with the given channel and keypair.
    ///
    /// The router will use the provided channel for all network communication and the keypair
    /// for message encryption/decryption.
    ///
    /// # Arguments
    /// * `channel` - The channel to use for network communication
    /// * `keypair` - The keypair to use for encryption/decryption
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

    /// Starts listening for incoming messages and routes them to the appropriate conversations.
    ///
    /// This method should be spawned in a separate task as it runs indefinitely.
    ///
    /// # Returns
    /// * `Ok(())` if the listener exits normally
    /// * `Err(ConversationError)` if an error occurs while processing messages
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

    async fn internal_add_with_id(&self, id: &str, mut conversation: Box<dyn Conversation + Send>) -> Result<Response, ConversationError> {
        let response = conversation.init()?;

        self.conversations
            .lock()
            .await
            .insert(id.to_string(), conversation);

        Ok(response)
    }

    /// Adds a new conversation to the router.
    ///
    /// The conversation will be initialized and its initial response will be processed.
    ///
    /// # Arguments
    /// * `conversation` - The conversation to add
    ///
    /// # Returns
    /// * `Ok(String)` - The ID of the added conversation
    /// * `Err(ConversationError)` if an error occurs during initialization
    pub async fn add_conversation(
        &self,
        conversation: Box<dyn Conversation + Send>,
    ) -> Result<String, ConversationError> {
        let conversation_id = random_string(32);

        let response = self.internal_add_with_id(&conversation_id, conversation).await?;
        self.process_response(&conversation_id, response).await?;

        Ok(conversation_id)
    }

    /// Subscribes to notifications from a conversation.
    ///
    /// # Type Parameters
    /// * `T` - The type of notifications to receive, must implement `DeserializeOwned` and `Serialize`
    ///
    /// # Arguments
    /// * `id` - The ID of the conversation to subscribe to
    ///
    /// # Returns
    /// * `Ok(NotificationStream<T>)` - A stream of notifications from the conversation
    /// * `Err(ConversationError)` if an error occurs during subscription
    pub async fn subscribe_to_service_request<T: DeserializeOwned + Serialize>(
        &self,
        id: String,
    ) -> Result<NotificationStream<T>, ConversationError> {
        let (tx, rx) = mpsc::channel(8);
        self.subscribers
            .lock()
            .await
            .entry(id)
            .or_insert(Vec::new())
            .push(tx);

        let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
        let rx = rx.map(|content| serde_json::from_value(content));
        let rx = NotificationStream::new(rx);

        Ok(rx)
    }

    /// Adds a conversation and subscribes to its notifications in a single operation.
    ///
    /// This is a convenience method that combines `add_conversation` and `subscribe_to_service_request`
    /// for conversations that implement `ConversationWithNotification`.
    /// 
    /// It also performs the subscription *before* adding the conversation to the router,
    /// so the subscriber will not miss any notifications.
    ///
    /// # Type Parameters
    /// * `Conv` - The conversation type, must implement `ConversationWithNotification`
    ///
    /// # Arguments
    /// * `conversation` - The conversation to add
    ///
    /// # Returns
    /// * `Ok(NotificationStream<Conv::Notification>)` - A stream of notifications from the conversation
    /// * `Err(ConversationError)` if an error occurs during initialization or subscription
    pub async fn add_and_subscribe<Conv: ConversationWithNotification + Send + 'static>(
        &self,
        conversation: Conv,
    ) -> Result<NotificationStream<Conv::Notification>, ConversationError> {
        let conversation_id = random_string(32);
        let delayed_reply = self.subscribe_to_service_request::<Conv::Notification>(conversation_id.clone()).await?;
        let response = self.internal_add_with_id(&conversation_id, Box::new(conversation)).await?;
        self.process_response(&conversation_id, response).await?;

        Ok(delayed_reply)
    }

    /// Gets a reference to the underlying channel.
    pub fn channel(&self) -> &C {
        &self.channel
    }

    /// Gets a reference to the router's keypair.
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

/// A response from a conversation.
///
/// Responses can include:
/// - Filters for subscribing to specific message types
/// - Replies to send to specific recipients or broadcast to all participants in the conversation
/// - Notifications to send to subscribers
/// - A flag indicating if the conversation is finished. If set, the conversation will be removed from the router.
///
/// # Example
/// ```rust,no_run
/// use portal::router::Response;
/// use nostr::{Filter, Kind, Tags};
///
/// let response = Response::new()
///     .filter(Filter::new().kinds(vec![Kind::from(27000)]))
///     .reply_to(pubkey, Kind::from(27001), Tags::new(), content)
///     .notify(notification)
///     .finish();
/// ```
#[derive(Debug, Default)]
pub struct Response {
    filter: Filter,
    responses: Vec<ResponseEntry>,
    notifications: Vec<serde_json::Value>,
    finished: bool,
}

impl Response {
    /// Creates a new empty response.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the filter for this response.
    ///
    /// The filter will be used to subscribe to specific message types with the relays.
    ///
    /// # Arguments
    /// * `filter` - The filter to set
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Adds a reply to be sent to all recipients.
    ///
    /// # Arguments
    /// * `kind` - The kind of message to send
    /// * `tags` - The tags to include in the message
    /// * `content` - The content to send, must be serializable
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

    /// Adds a reply to be sent to a specific recipient.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the recipient
    /// * `kind` - The kind of message to send
    /// * `tags` - The tags to include in the message
    /// * `content` - The content to send, must be serializable
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

    /// Adds a notification to be sent to subscribers.
    ///
    /// # Arguments
    /// * `data` - The notification data to send, must be serializable
    pub fn notify<S: serde::Serialize>(mut self, data: S) -> Self {
        let content = serde_json::to_value(&data).unwrap();
        self.notifications.push(content);
        self
    }

    /// Marks the conversation as finished.
    /// 
    /// When a conversation is finished, it will be removed from the router.
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


/// Convenience wrapper around a stream of notifications.
/// 
/// It's automatically implemented for any stream that implements `Stream<Item = Result<T, serde_json::Error>> + Send + Unpin + 'static`.
pub trait InnerNotificationStream<T: Serialize>:
    Stream<Item = Result<T, serde_json::Error>> + Send + Unpin + 'static
{
}
impl<S, T: Serialize> InnerNotificationStream<T> for S where
    S: Stream<Item = Result<T, serde_json::Error>> + Send + Unpin + 'static
{
}

pub struct NotificationStream<T: Serialize> {
    stream: Box<dyn InnerNotificationStream<T>>,
}

impl<T: Serialize> NotificationStream<T> {
    pub(crate) fn new(stream: impl InnerNotificationStream<T>) -> Self {
        Self {
            stream: Box::new(stream),
        }
    }

    /// Returns the next notification from the stream.
    pub async fn next(&mut self) -> Option<Result<T, serde_json::Error>> {
        use futures::StreamExt;

        self.stream.next().await
    }
}

impl<T: Serialize> Deref for NotificationStream<T> {
    type Target = Box<dyn InnerNotificationStream<T>>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl<T: Serialize> DerefMut for NotificationStream<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}