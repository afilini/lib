use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use nostr::{
    event::{Event, EventBuilder, Kind},
    filter::Filter,
    message::{RelayMessage, SubscriptionId},
    nips::nip44,
};
use nostr_relay_pool::RelayPoolNotification;
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;

use crate::{
    protocol::{LocalKeypair, model::event_kinds::SUBKEY_PROOF},
    router::{
        CleartextEvent, Conversation, ConversationError, ConversationMessage, NotificationStream,
        PortalId, RelayNode, Response, adapters::ConversationWithNotification, channel::Channel,
    },
};

type ConversationBox = Box<dyn Conversation + Send + Sync>;

#[derive(thiserror::Error, Debug)]
pub enum MessageRouterActorError {
    #[error("Channel error: {0}")]
    Channel(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Conversation error: {0}")]
    Conversation(#[from] ConversationError),
    #[error("Receiver error: {0}")]
    Receiver(#[from] oneshot::error::RecvError),
}

pub enum MessageRouterActorMessage {
    AddRelay(String, oneshot::Sender<Result<(), ConversationError>>),
    RemoveRelay(String, oneshot::Sender<Result<(), ConversationError>>),
    Shutdown(oneshot::Sender<Result<(), ConversationError>>),
    Listen(oneshot::Sender<Result<(), ConversationError>>),
    AddConversation(
        ConversationBox,
        oneshot::Sender<Result<PortalId, ConversationError>>,
    ),
    AddConversationWithRelays(
        ConversationBox,
        Vec<String>,
        oneshot::Sender<Result<PortalId, ConversationError>>,
    ),
    SubscribeToServiceRequest(
        PortalId,
        oneshot::Sender<Result<NotificationStream<serde_json::Value>, ConversationError>>,
    ),
    AddAndSubscribe(
        ConversationBox,
        oneshot::Sender<Result<NotificationStream<serde_json::Value>, ConversationError>>,
    ),
}

pub struct MessageRouterActor {
    keypair: LocalKeypair,
    sender: mpsc::Sender<MessageRouterActorMessage>,
}

impl MessageRouterActor {
    pub fn new<C>(channel: C, keypair: LocalKeypair) -> Self
    where
        C: Channel + Send + Sync + 'static,
        C::Error: From<nostr::types::url::Error>,
    {
        let (tx, mut rx) = mpsc::channel(100);
        let keypair_clone = keypair.clone();
        tokio::spawn(async move {
            let mut state = MessageRouterActorState::new(channel, keypair_clone);
            while let Some(message) = rx.recv().await {
                match message {
                    MessageRouterActorMessage::AddRelay(url, response_tx) => {
                        let result = state.add_relay(url.clone()).await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send AddRelay({}) response: {:?}", url, e);
                        }
                    }
                    MessageRouterActorMessage::RemoveRelay(url, response_tx) => {
                        let result = state.remove_relay(url.clone()).await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send RemoveRelay({}) response: {:?}", url, e);
                        }
                    }
                    MessageRouterActorMessage::Shutdown(response_tx) => {
                        let result = state.shutdown().await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send Shutdown response: {:?}", e);
                        }
                        break;
                    }
                    MessageRouterActorMessage::Listen(response_tx) => {
                        let result = state.listen().await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send Listen response: {:?}", e);
                        }
                    }
                    MessageRouterActorMessage::AddConversation(conversation, response_tx) => {
                        let result = state.add_conversation(conversation).await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send AddConversation response: {:?}", e);
                        }
                    }
                    MessageRouterActorMessage::AddConversationWithRelays(
                        conversation,
                        relays,
                        response_tx,
                    ) => {
                        let result = state
                            .add_conversation_with_relays(conversation, relays)
                            .await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!(
                                "Failed to send AddConversationWithRelays response: {:?}",
                                e
                            );
                        }
                    }
                    MessageRouterActorMessage::SubscribeToServiceRequest(id, response_tx) => {
                        let result = state.subscribe_to_service_request(id);
                        if let Err(e) = response_tx.send(result) {
                            log::error!(
                                "Failed to send SubscribeToServiceRequest response: {:?}",
                                e
                            );
                        }
                    }
                    MessageRouterActorMessage::AddAndSubscribe(conversation, response_tx) => {
                        let result = state.add_and_subscribe(conversation).await;
                        if let Err(e) = response_tx.send(result) {
                            log::error!("Failed to send AddAndSubscribe response: {:?}", e);
                        }
                    }
                }
            }
        });
        Self {
            keypair,
            sender: tx,
        }
    }

    async fn send_message(
        &self,
        message: MessageRouterActorMessage,
    ) -> Result<(), MessageRouterActorError> {
        self.sender
            .send(message)
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        Ok(())
    }

    pub fn keypair(&self) -> &LocalKeypair {
        &self.keypair
    }

    pub async fn add_relay(&self, url: String) -> Result<(), MessageRouterActorError> {
        let (tx, _rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::AddRelay(url, tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        Ok(())
    }

    pub async fn remove_relay(&self, url: String) -> Result<(), MessageRouterActorError> {
        let (tx, _rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::RemoveRelay(url, tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), MessageRouterActorError> {
        let (tx, _rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::Shutdown(tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        Ok(())
    }

    pub async fn listen(&self) -> Result<(), MessageRouterActorError> {
        let (tx, _rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::Listen(tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        Ok(())
    }

    pub async fn add_conversation(
        &self,
        conversation: ConversationBox,
    ) -> Result<PortalId, MessageRouterActorError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::AddConversation(conversation, tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        match rx.await {
            Ok(result) => result.map_err(MessageRouterActorError::Conversation),
            Err(e) => Err(MessageRouterActorError::Receiver(e)),
        }
    }

    pub async fn add_conversation_with_relays(
        &self,
        conversation: ConversationBox,
        relays: Vec<String>,
    ) -> Result<PortalId, MessageRouterActorError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::AddConversationWithRelays(
                conversation,
                relays,
                tx,
            ))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        match rx.await {
            Ok(result) => result.map_err(MessageRouterActorError::Conversation),
            Err(e) => Err(MessageRouterActorError::Receiver(e)),
        }
    }

    /// Subscribes to notifications from a conversation with a specific type.
    ///
    /// # Type Parameters
    /// * `T` - The type of notifications to receive, must implement `DeserializeOwned` and `Serialize`
    ///
    /// # Arguments
    /// * `id` - The ID of the conversation to subscribe to
    ///
    /// # Returns
    /// * `Ok(NotificationStream<T>)` - A stream of notifications from the conversation
    /// * `Err(MessageRouterActorError)` if an error occurs during subscription
    pub async fn subscribe_to_service_request<T: DeserializeOwned + Serialize>(
        &self,
        id: PortalId,
    ) -> Result<NotificationStream<T>, MessageRouterActorError> {
        // For the actor pattern, we need to use the raw stream and convert it
        let raw_stream = self.subscribe_to_service_request_raw(id).await?;

        // Convert the stream from serde_json::Value to T
        let NotificationStream { stream } = raw_stream;
        let typed_stream =
            stream.map(|result| result.and_then(|value| serde_json::from_value(value)));

        Ok(NotificationStream::new(typed_stream))
    }

    /// Subscribes to notifications from a conversation with raw JSON values.
    ///
    /// This is the internal method used by the actor pattern.
    ///
    /// # Arguments
    /// * `id` - The ID of the conversation to subscribe to
    ///
    /// # Returns
    /// * `Ok(NotificationStream<serde_json::Value>)` - A stream of raw JSON notifications
    /// * `Err(MessageRouterActorError)` if an error occurs during subscription
    async fn subscribe_to_service_request_raw(
        &self,
        id: PortalId,
    ) -> Result<NotificationStream<serde_json::Value>, MessageRouterActorError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::SubscribeToServiceRequest(id, tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        match rx.await {
            Ok(result) => result.map_err(MessageRouterActorError::Conversation),
            Err(e) => Err(MessageRouterActorError::Receiver(e)),
        }
    }

    /// Adds a conversation and subscribes to its notifications in a single operation (typed).
    pub async fn add_and_subscribe<T: DeserializeOwned + Serialize>(
        &self,
        conversation: ConversationBox,
    ) -> Result<NotificationStream<T>, MessageRouterActorError> {
        let raw_stream = self.add_and_subscribe_raw(conversation).await?;
        let NotificationStream { stream } = raw_stream;
        let typed_stream =
            stream.map(|result| result.and_then(|value| serde_json::from_value(value)));
        Ok(NotificationStream::new(typed_stream))
    }

    /// Adds a conversation and subscribes to its notifications in a single operation (raw Value).
    async fn add_and_subscribe_raw(
        &self,
        conversation: ConversationBox,
    ) -> Result<NotificationStream<serde_json::Value>, MessageRouterActorError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(MessageRouterActorMessage::AddAndSubscribe(conversation, tx))
            .await
            .map_err(|e| MessageRouterActorError::Channel(Box::new(e)))?;
        match rx.await {
            Ok(result) => result.map_err(MessageRouterActorError::Conversation),
            Err(e) => Err(MessageRouterActorError::Receiver(e)),
        }
    }
}

pub struct MessageRouterActorState<C: Channel> {
    channel: C,
    keypair: LocalKeypair,
    conversations: HashMap<PortalId, ConversationBox>,
    aliases: HashMap<PortalId, Vec<u64>>,
    filters: HashMap<PortalId, Filter>,
    subscribers: HashMap<PortalId, Vec<mpsc::Sender<serde_json::Value>>>,
    end_of_stored_events: HashMap<PortalId, usize>,

    relay_nodes: HashMap<String, RelayNode>,
    global_relay_node: RelayNode,
}

impl<C: Channel + Send + Sync + 'static> MessageRouterActorState<C>
where
    C::Error: From<nostr::types::url::Error>,
{
    pub fn new(channel: C, keypair: LocalKeypair) -> Self {
        Self {
            channel,
            keypair,
            conversations: HashMap::new(),
            aliases: HashMap::new(),
            filters: HashMap::new(),
            subscribers: HashMap::new(),
            end_of_stored_events: HashMap::new(),
            relay_nodes: HashMap::new(),
            global_relay_node: RelayNode::new(),
        }
    }

    pub async fn add_relay(&mut self, url: String) -> Result<(), ConversationError> {
        self.channel
            .add_relay(url.clone())
            .await
            .map_err(|e| ConversationError::Inner(Box::new(e)))?;

        // Subscribe existing conversations to new relays
        {
            for conversation_id in self.global_relay_node.conversations.iter() {
                if let Some(filter) = self.filters.get(conversation_id) {
                    log::trace!("Subscribing {} to new relay = {:?}", conversation_id, &url);
                    self.channel
                        .subscribe_to(vec![url.clone()], conversation_id.clone(), filter.clone())
                        .await
                        .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                    self.end_of_stored_events
                        .get_mut(conversation_id)
                        .and_then(|v| Some(*v += 1));
                }

                if let Some(aliases) = self.aliases.get(conversation_id) {
                    for alias in aliases {
                        let alias_id =
                            PortalId::new_conversation_alias(conversation_id.id(), *alias);
                        if let Some(filter) = self.filters.get(&alias_id) {
                            self.channel
                                .subscribe_to(vec![url.clone()], alias_id, filter.clone())
                                .await
                                .map_err(|e| ConversationError::Inner(Box::new(e)))?;
                        }
                    }
                }
            }
        }

        {
            self.relay_nodes
                .entry(url.clone())
                .or_insert_with(|| RelayNode::new());
        }

        Ok(())
    }

    pub async fn remove_relay(&mut self, url: String) -> Result<(), ConversationError> {
        self.channel
            .remove_relay(url.clone())
            .await
            .map_err(|e| ConversationError::Inner(Box::new(e)))?;

        if let Some(node) = self.relay_nodes.remove(&url) {
            for conv in node.conversations.iter() {
                let relays_of_conversation = self.get_relays_by_conversation(conv)?;
                match relays_of_conversation {
                    Some(urls) => {
                        // If conversation it not present in other relays, clean it
                        if urls.is_empty() {
                            self.cleanup_conversation(conv).await?;
                        }
                    }
                    None => {
                        // If conversation is present also on the global node relay, should'nt happen, dont clean it
                    }
                }

                self.end_of_stored_events
                    .get_mut(conv)
                    .and_then(|v| Some(*v = v.saturating_sub(1)));
            }
        }
        Ok(())
    }

    fn get_relays_by_conversation(
        &self,
        conversation_id: &PortalId,
    ) -> Result<Option<HashSet<String>>, ConversationError> {
        if self
            .global_relay_node
            .conversations
            .contains(conversation_id)
        {
            return Ok(None);
        }

        let mut relays = HashSet::new();
        for (url, node) in self.relay_nodes.iter() {
            if node.conversations.contains(conversation_id) {
                relays.insert(url.clone());
            }
        }

        Ok(Some(relays))
    }

    async fn cleanup_conversation(
        &mut self,
        conversation: &PortalId,
    ) -> Result<(), ConversationError> {
        // Remove conversation state
        self.conversations.remove(conversation);
        self.subscribers.remove(conversation);
        self.filters.remove(conversation);
        self.end_of_stored_events.remove(conversation);
        let aliases = self.aliases.remove(conversation);

        // Remove from global relay node
        {
            self.global_relay_node.conversations.remove(conversation);
        }

        // Remove from specific relay node
        {
            for (_, relay_node) in self.relay_nodes.iter_mut() {
                relay_node.conversations.remove(conversation);
            }
        }

        // Remove filters from relays
        self.channel
            .unsubscribe(conversation.clone())
            .await
            .map_err(|e| ConversationError::Inner(Box::new(e)))?;

        if let Some(aliases) = aliases {
            for alias in aliases {
                let alias_id = PortalId::new_conversation_alias(conversation.id(), alias);
                self.channel
                    .unsubscribe(alias_id)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
            }
        }
        Ok(())
    }

    /// Shuts down the router and disconnects from all relays.
    pub async fn shutdown(&mut self) -> Result<(), ConversationError> {
        self.channel
            .shutdown()
            .await
            .map_err(|e| ConversationError::Inner(Box::new(e)))?;

        self.conversations.clear();
        self.subscribers.clear();
        self.aliases.clear();
        self.filters.clear();
        self.end_of_stored_events.clear();
        self.global_relay_node.conversations.clear();
        Ok(())
    }

    /// Starts listening for incoming messages and routes them to the appropriate conversations.
    ///
    /// This method should be spawned in a separate task as it runs indefinitely.
    ///
    /// # Returns
    /// * `Ok(())` if the listener exits normally
    /// * `Err(ConversationError)` if an error occurs while processing messages
    pub async fn listen(&mut self) -> Result<(), ConversationError> {
        enum LocalEvent {
            Message(Event),
            EndOfStoredEvents,
        }

        while let Ok(notification) = self.channel.receive().await {
            log::trace!("Notification = {:?}", notification);

            let (subscription_id, event): (SubscriptionId, LocalEvent) = match notification {
                RelayPoolNotification::Message {
                    message:
                        RelayMessage::Event {
                            subscription_id,
                            event,
                        },
                    ..
                } => (
                    subscription_id.into_owned(),
                    LocalEvent::Message(event.into_owned()),
                ),
                RelayPoolNotification::Event {
                    event,
                    subscription_id,
                    ..
                } => (subscription_id, LocalEvent::Message(*event)),
                RelayPoolNotification::Message {
                    message: RelayMessage::EndOfStoredEvents(subscription_id),
                    ..
                } => {
                    // Parse the subscription ID to get the PortalId
                    let portal_id = match PortalId::parse(subscription_id.as_str()) {
                        Some(id) => id,
                        None => {
                            log::warn!(
                                "Invalid subscription ID format for EOSE: {:?}",
                                subscription_id
                            );
                            continue;
                        }
                    };

                    let remaining = self.end_of_stored_events.get_mut(&portal_id).and_then(|v| {
                        *v -= 1;
                        Some(*v)
                    });

                    log::trace!("{:?} EOSE left for {}", remaining, portal_id);

                    if remaining == Some(0) {
                        self.end_of_stored_events.remove(&portal_id);
                        (subscription_id.into_owned(), LocalEvent::EndOfStoredEvents)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            let message = match &event {
                LocalEvent::Message(event) => {
                    if event.pubkey == self.keypair.public_key() && event.kind != Kind::Metadata {
                        log::trace!("Ignoring event from self");
                        continue;
                    }

                    if !event.verify_signature() {
                        log::warn!("Invalid signature for event id: {:?}", event.id);
                        continue;
                    }

                    log::trace!("Decrypting with key = {:?}", self.keypair.public_key());

                    if let Ok(content) =
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
                    } else if let Ok(cleartext) =
                        serde_json::from_str::<serde_json::Value>(&event.content)
                    {
                        log::trace!("Unencrypted event: {:?}", cleartext);
                        ConversationMessage::Cleartext(CleartextEvent::new_json(&event, cleartext))
                    } else {
                        log::warn!("Failed to decrypt event: {:?}", event);
                        ConversationMessage::Encrypted(event.clone())
                    }
                }
                LocalEvent::EndOfStoredEvents => ConversationMessage::EndOfStoredEvents,
            };

            self.dispatch_event(subscription_id.clone(), message.clone())
                .await?;

            let mut to_cleanup = vec![];
            let mut other_conversations = vec![];

            // Check if there are other potential conversations to dispatch to
            for (id, filter) in self.filters.iter() {
                if id.to_string() == subscription_id.as_str() {
                    continue;
                }

                match self.conversations.get(id) {
                    Some(conv) if conv.is_expired() => {
                        to_cleanup.push(id.clone());
                        continue;
                    }
                    _ => {}
                }

                if let LocalEvent::Message(event) = &event {
                    if filter.match_event(&event) {
                        other_conversations.push(id.clone());
                    }
                }
            }

            for id in to_cleanup {
                self.cleanup_conversation(&id).await?;
            }

            for id in other_conversations {
                self.dispatch_event(SubscriptionId::new(id.clone()), message.clone())
                    .await?;
            }
        }

        Ok(())
    }

    async fn dispatch_event(
        &mut self,
        subscription_id: SubscriptionId,
        message: ConversationMessage,
    ) -> Result<(), ConversationError> {
        let subscription_str = subscription_id.as_str();

        // Parse the subscription ID to get the PortalId
        let conversation_id = match PortalId::parse(subscription_str) {
            Some(id) => id,
            None => {
                log::warn!("Invalid subscription ID format: {:?}", subscription_str);
                return Ok(());
            }
        };

        let response = match self.conversations.get_mut(&conversation_id) {
            Some(conv) => match conv.on_message(message) {
                Ok(response) => response,
                Err(e) => {
                    log::warn!("Error in conversation id {}: {:?}", conversation_id, e);
                    Response::new().finish()
                }
            },
            None => {
                log::warn!("No conversation found for id: {}", conversation_id);
                self.channel
                    .unsubscribe(conversation_id)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                return Ok(());
            }
        };

        self.process_response(&conversation_id, response).await?;

        Ok(())
    }

    async fn process_response(
        &mut self,
        id: &PortalId,
        response: Response,
    ) -> Result<(), ConversationError> {
        log::trace!("Processing response builder for {} = {:?}", id, response);

        let selected_relays_optional = self.get_relays_by_conversation(id)?;

        if !response.filter.is_empty() {
            self.filters.insert(id.clone(), response.filter.clone());

            let num_relays = if let Some(selected_relays) = selected_relays_optional.clone() {
                let num_relays = selected_relays.len();

                log::trace!("Subscribing to relays = {:?}", selected_relays);
                self.channel
                    .subscribe_to(selected_relays, id.clone(), response.filter.clone())
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                num_relays
            } else {
                log::trace!("Subscribing to all relays");
                self.channel
                    .subscribe(id.clone(), response.filter.clone())
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                self.channel
                    .num_relays()
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?
            };

            self.end_of_stored_events.insert(id.clone(), num_relays);
        }

        let mut events_to_broadcast = vec![];
        for response_entry in response.responses.iter() {
            log::trace!(
                "Sending event of kind {:?} to {:?}",
                response_entry.kind,
                response_entry.recepient_keys
            );

            let build_event = |content: &str| {
                EventBuilder::new(response_entry.kind, content)
                    .tags(response_entry.tags.clone())
                    .sign_with_keys(&self.keypair)
                    .map_err(|e| ConversationError::Inner(Box::new(e)))
            };

            if !response_entry.encrypted {
                let content = serde_json::to_string(&response_entry.content)
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                let event = build_event(&content)?;
                log::trace!("Unencrypted event: {:?}", event);
                events_to_broadcast.push(event);
            } else {
                for pubkey in response_entry.recepient_keys.iter() {
                    let content = nip44::encrypt(
                        &self.keypair.secret_key(),
                        &pubkey,
                        serde_json::to_string(&response_entry.content)
                            .map_err(|e| ConversationError::Inner(Box::new(e)))?,
                        nip44::Version::V2,
                    )
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;

                    let event = build_event(&content)?;
                    log::trace!("Encrypted event: {:?}", event);
                    events_to_broadcast.push(event);
                }
            }
        }

        for notification in response.notifications.iter() {
            if let Some(senders) = self.subscribers.get_mut(id) {
                for sender in senders.iter_mut() {
                    let _ = sender.send(notification.clone()).await;
                }
            }
        }

        if response.subscribe_to_subkey_proofs {
            let alias_num = rand::random::<u64>();

            self.aliases.entry(id.clone()).or_default().push(alias_num);

            let filter = Filter::new()
                .kinds(vec![Kind::Custom(SUBKEY_PROOF)])
                .events(events_to_broadcast.iter().map(|e| e.id));

            let alias = PortalId::new_conversation_alias(id.id(), alias_num);
            self.filters.insert(alias.clone(), filter.clone());

            if let Some(selected_relays) = selected_relays_optional.clone() {
                log::trace!(
                    "Subscribing 'subkey proof' to relays = {:?}",
                    selected_relays
                );
                self.channel
                    .subscribe_to(selected_relays, alias, filter)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
            } else {
                log::trace!("Subscribing 'subkey proof' to all relays");
                // Subscribe to subkey proofs to all

                self.channel
                    .subscribe(alias, filter)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
            }
        }

        // check if Response has selected relays
        if let Some(selected_relays) = selected_relays_optional {
            for event in events_to_broadcast {
                // if selected relays, broadcast to selected relays
                self.channel
                    .broadcast_to(selected_relays.clone(), event)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
            }
        } else {
            for event in events_to_broadcast {
                // if not selected relays, broadcast to all relays
                self.channel
                    .broadcast(event)
                    .await
                    .map_err(|e| ConversationError::Inner(Box::new(e)))?;
            }

            // TODO: wait for confirmation from relays
        }

        if response.finished {
            self.cleanup_conversation(id).await?;
        }

        Ok(())
    }

    fn internal_add_with_id(
        &mut self,
        id: &PortalId,
        mut conversation: ConversationBox,
        relays: Option<Vec<String>>,
    ) -> Result<Response, ConversationError> {
        let response = conversation.init()?;

        if let Some(relays) = relays {
            // Update relays node
            // for each relay parameter
            for relay in relays {
                // get relay node associated

                match self.relay_nodes.get_mut(&relay) {
                    Some(found_node) => {
                        found_node.conversations.insert(id.clone());
                    }
                    None => {
                        return Err(ConversationError::RelayNotConnected(relay));
                    }
                }
            }
        } else {
            // Update Global Relay Node
            self.global_relay_node.conversations.insert(id.clone());
        }

        self.conversations.insert(id.clone(), conversation);

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
    /// * `Ok(PortalId)` - The ID of the added conversation
    /// * `Err(ConversationError)` if an error occurs during initialization
    pub async fn add_conversation(
        &mut self,
        conversation: ConversationBox,
    ) -> Result<PortalId, ConversationError> {
        let conversation_id = PortalId::new_conversation();

        let response = self.internal_add_with_id(&conversation_id, conversation, None)?;
        self.process_response(&conversation_id, response).await?;

        Ok(conversation_id)
    }

    pub async fn add_conversation_with_relays(
        &mut self,
        conversation: ConversationBox,
        relays: Vec<String>,
    ) -> Result<PortalId, ConversationError> {
        let conversation_id = PortalId::new_conversation();

        let response = self.internal_add_with_id(&conversation_id, conversation, Some(relays))?;
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
    pub fn subscribe_to_service_request<T: DeserializeOwned + Serialize>(
        &mut self,
        id: PortalId,
    ) -> Result<NotificationStream<T>, ConversationError> {
        let (tx, rx) = mpsc::channel(8);
        self.subscribers.entry(id).or_insert(Vec::new()).push(tx);

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
    pub async fn add_and_subscribe<T: DeserializeOwned + Serialize>(
        &mut self,
        conversation: ConversationBox,
    ) -> Result<NotificationStream<T>, ConversationError> {
        let conversation_id = PortalId::new_conversation();

        // Update Global Relay Node

        {
            self.global_relay_node
                .conversations
                .insert(conversation_id.clone());
        }

        let delayed_reply = self.subscribe_to_service_request::<T>(conversation_id.clone())?;
        let response = self.internal_add_with_id(&conversation_id, conversation, None)?;
        self.process_response(&conversation_id, response).await?;

        Ok(delayed_reply)
    }
}
