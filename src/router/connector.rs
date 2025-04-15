use std::{
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures::Stream;
use nostr::{
    event::{Event, EventBuilder},
    message::{RelayMessage, SubscriptionId},
    nips::nip44,
};
use nostr_relay_pool::{RelayPool, RelayPoolNotification, SubscribeOptions};
use serde::Serialize;
use tokio::sync::{Mutex, mpsc};

use crate::{
    protocol::LocalKeypair,
    router::{CleartextEvent, MessageRouter, RelayAction, WrappedContent},
};

pub struct Connector {
    keypair: LocalKeypair,
    relays: RelayPool,
    router: Mutex<MessageRouter>,
    outgoing_queue: Mutex<mpsc::UnboundedReceiver<RelayAction>>,

    bootstrapped: Arc<AtomicBool>,
}

impl Connector {
    pub fn new(keypair: LocalKeypair, relays: RelayPool) -> Arc<Self> {
        let (router, outgoing_queue) = MessageRouter::new();

        Arc::new(Self {
            keypair,
            relays,
            router: Mutex::new(router),
            outgoing_queue: Mutex::new(outgoing_queue),

            bootstrapped: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn bootstrap(self: &Arc<Self>) -> Result<(), Error> {
        // Check if already bootstrapped
        if self.bootstrapped.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Connect
        self.relays.connect().await;

        let service = Arc::clone(&self);
        let bootstrapped = Arc::clone(&self.bootstrapped);
        tokio::spawn(async move {
            while bootstrapped.load(Ordering::SeqCst) {
                if let Err(e) = service.process_incoming_events().await {
                    log::error!("Error processing incoming events: {:?}", e);
                }
            }
        });

        let service = Arc::clone(&self);
        let bootstrapped = Arc::clone(&self.bootstrapped);
        tokio::spawn(async move {
            while bootstrapped.load(Ordering::SeqCst) {
                if let Err(e) = service.process_outgoing_events().await {
                    log::error!("Error processing outgoing events: {:?}", e);
                }
            }
        });

        // Mark as bootstrapped
        self.bootstrapped.store(true, Ordering::SeqCst);

        Ok(())
    }

    pub async fn disconnect(self: &Arc<Self>) {
        self.relays.disconnect().await;

        // Purge queue
        let mut queue = self.outgoing_queue.lock().await;
        while let Some(_) = queue.recv().await {}

        self.router.lock().await.purge();

        self.bootstrapped.store(false, Ordering::SeqCst);
    }

    pub async fn process_incoming_events(&self) -> Result<(), Error> {
        log::debug!("Processing events...");

        while let Ok(notification) = self.relays.notifications().recv().await {
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

            if let Ok(content) =
                nip44::decrypt(&self.keypair.secret_key(), &event.pubkey, &event.content)
            {
                let cleartext = CleartextEvent::new(&event, &content)?;
                log::trace!("Decrypted event: {:?}", cleartext);

                self.router
                    .lock()
                    .await
                    .on_message(&cleartext, &subscription_id)
                    .await?;
            } else {
                log::warn!("Failed to decrypt event: {:?}", event);

                self.router
                    .lock()
                    .await
                    .on_encrypted_message(&event)
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn process_outgoing_events(&self) -> Result<(), Error> {
        while let Some(action) = self.outgoing_queue.lock().await.recv().await {
            match action {
                RelayAction::SendEvent(to, event) => {
                    log::trace!("Sending event: {:?} to {}", event, to);

                    let encrypted = nip44::encrypt(
                        &self.keypair.secret_key(),
                        &to,
                        serde_json::to_string(&event.content)?,
                        nip44::Version::V2,
                    )?;
                    let event = EventBuilder::new(event.kind, encrypted)
                        .tags(event.tags)
                        .sign_with_keys(&self.keypair)?;

                    log::trace!("Encrypted event: {:?}", event);

                    // TODO: should only send to selected relays
                    self.relays.send_event(&event).await?;

                    // TODO: wait for confirmation from relays
                }
                RelayAction::ApplyFilter(id, filter) => {
                    log::trace!("Conversation id {}, applying filter: {:?}", id, filter);
                    self.relays
                        .subscribe_with_id(
                            SubscriptionId::new(id),
                            filter,
                            SubscribeOptions::default(),
                        )
                        .await?;
                }
                RelayAction::RemoveFilter(id) => {
                    log::trace!("Conversation id {}, removing filter", id);
                    let sub_id = SubscriptionId::new(id);
                    self.relays.unsubscribe(&sub_id).await;
                }
            }
        }

        Ok(())
    }

    pub fn relays(&self) -> &RelayPool {
        &self.relays
    }

    pub fn router(&self) -> &Mutex<MessageRouter> {
        &self.router
    }

    pub fn outgoing_queue(&self) -> &Mutex<mpsc::UnboundedReceiver<RelayAction>> {
        &self.outgoing_queue
    }

    pub fn keypair(&self) -> &LocalKeypair {
        &self.keypair
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Connection error")]
    ConnectionError,

    #[error("Disconnection error")]
    DisconnectionError,

    #[error("Message error")]
    MessageError,

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Relay pool error: {0}")]
    RelayPool(#[from] nostr_relay_pool::pool::Error),

    #[error("Subkey error: {0}")]
    Subkey(#[from] crate::protocol::subkey::SubkeyError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Key error: {0}")]
    Key(#[from] nostr::key::Error),

    #[error("Bech32 error: {0}")]
    Bech32(#[from] nostr::nips::nip19::Error),

    #[error("Nostr event error: {0}")]
    NostrEvent(#[from] nostr::event::builder::Error),

    #[error("NIP44 error: {0}")]
    Nip44(#[from] nostr::nips::nip44::Error),

    #[error("Conversation error: {0}")]
    Conversation(#[from] crate::router::ConversationError),
}

pub trait InnerDelayedReply<T: Serialize>:
    Stream<Item = Result<WrappedContent<T>, serde_json::Error>> + Send + Unpin + 'static
{
}
impl<S, T: Serialize> InnerDelayedReply<T> for S where
    S: Stream<Item = Result<WrappedContent<T>, serde_json::Error>> + Send + Unpin + 'static
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

    pub async fn await_reply(&mut self) -> Option<Result<WrappedContent<T>, serde_json::Error>> {
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
