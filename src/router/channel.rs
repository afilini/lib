use nostr::message::SubscriptionId;
use nostr_relay_pool::{RelayPool, RelayPoolNotification, SubscribeOptions};

/// A trait for an abstract channel
///
/// This is modeled around Nostr relays, in which we can subscribe to events matching a filter.
pub trait Channel: Send + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn subscribe(
        &self,
        id: String,
        filter: nostr::Filter,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn unsubscribe(
        &self,
        id: String,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;

    fn broadcast(
        &self,
        event: nostr::Event,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn receive(
        &self,
    ) -> impl std::future::Future<Output = Result<RelayPoolNotification, Self::Error>> + Send;
}

impl Channel for RelayPool {
    type Error = nostr_relay_pool::pool::Error;

    async fn subscribe(&self, id: String, filter: nostr::Filter) -> Result<(), Self::Error> {
        self.subscribe_with_id(SubscriptionId::new(id), filter, SubscribeOptions::default())
            .await?;

        Ok(())
    }

    async fn unsubscribe(&self, id: String) -> Result<(), Self::Error> {
        self.unsubscribe(&SubscriptionId::new(id)).await;
        Ok(())
    }

    async fn broadcast(&self, event: nostr::Event) -> Result<(), Self::Error> {
        self.send_event(&event).await?;
        Ok(())
    }

    async fn receive(&self) -> Result<RelayPoolNotification, Self::Error> {
        self.notifications()
            .recv()
            .await
            .map_err(|_| nostr_relay_pool::pool::Error::Shutdown)
    }
}
