use std::sync::Arc;

use portal::{
    app::handlers::{AuthChallengeEvent, AuthChallengeListenerConversation, AuthInitConversation, AuthResponseConversation},
    nostr::nips::nip19::ToBech32,
    nostr_relay_pool::{RelayOptions, RelayPool},
    protocol::{auth_init::AuthInitUrl, model::auth::SubkeyProof},
    router::{DelayedReply, MessageRouter, MultiKeyProxy},
};

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn init_logger() {
    use log::LevelFilter;
    use android_logger::Config;

    android_logger::init_once(
        Config::default().with_max_level(LevelFilter::Trace),
    );

    log::info!("Logger initialized");
}

pub use portal::app::*;

#[derive(uniffi::Object)]
pub struct Keypair {
    inner: portal::protocol::LocalKeypair,
}

#[uniffi::export]
impl Keypair {
    #[uniffi::constructor]
    pub fn new(nsec: &str, subkey_proof: Option<SubkeyProof>) -> Result<Self, KeypairError> {
        let keys = portal::nostr::Keys::parse(nsec).map_err(|_| KeypairError::InvalidNsec)?;
        Ok(Self {
            inner: portal::protocol::LocalKeypair::new(keys, subkey_proof),
        })
    }

    pub fn public_key(&self) -> portal::protocol::model::bindings::PublicKey {
        portal::protocol::model::bindings::PublicKey(self.inner.public_key())
    }

    pub fn subkey_proof(&self) -> Option<SubkeyProof> {
        self.inner.subkey_proof().map(|p| p.clone())
    }

    pub fn nsec(&self) -> Result<String, KeypairError> {
        let keys = self.inner.secret_key();
        let nsec = keys.to_bech32().map_err(|_| KeypairError::InvalidNsec)?;
        Ok(nsec)
    }
}

#[derive(Debug, PartialEq, thiserror::Error, uniffi::Error)]
pub enum KeypairError {
    #[error("Invalid nsec")]
    InvalidNsec,
}

#[derive(uniffi::Object)]
pub struct PortalApp {
    router: Arc<MessageRouter<RelayPool>>,
}


#[uniffi::export]
pub fn parse_auth_init_url(url: &str) -> Result<AuthInitUrl, ParseError> {
    use std::str::FromStr;
    Ok(AuthInitUrl::from_str(url)?)
}

#[derive(Debug, PartialEq, thiserror::Error, uniffi::Error)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    Inner(String),
}
impl From<portal::protocol::auth_init::ParseError> for ParseError {
    fn from(error: portal::protocol::auth_init::ParseError) -> Self {
        ParseError::Inner(error.to_string())
    }
}

#[derive(Debug, PartialEq, thiserror::Error, uniffi::Error)]
pub enum CallbackError {
    #[error("Callback error: {0}")]
    Error(String),
}

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait AuthChallengeListener: Send + Sync {
    async fn on_auth_challenge(&self, event: AuthChallengeEvent) -> Result<bool, CallbackError>;
}

#[uniffi::export]
impl PortalApp {
    #[uniffi::constructor]
    pub async fn new(keypair: Arc<Keypair>, relays: Vec<String>) -> Result<Arc<Self>, AppError> {
        let relay_pool = RelayPool::new();
        for relay in relays {
            relay_pool.add_relay(relay, RelayOptions::default()).await?;
        }
        relay_pool.connect().await;

        let keypair = &keypair.inner;
        let router = Arc::new(MessageRouter::new(relay_pool, keypair.clone()));

        Ok(Arc::new(Self { router }))
    }

    pub async fn listen(&self) -> Result<(), AppError> {
        self.router.listen().await.unwrap();
        Ok(())
    }

    pub async fn send_auth_init(&self, url: AuthInitUrl) -> Result<(), AppError> {
        let relays = self
            .router
            .channel()
            .relays()
            .await
            .keys()
            .map(|r| r.to_string())
            .collect();
        let _id = self
            .router
            .add_conversation(Box::new(AuthInitConversation { url, relays }))
            .await?;
        // let rx = self.router.subscribe_to_service_request(id).await?;
        // let response = rx.await_reply().await.map_err(AppError::ConversationError)?;

        Ok(())
    }

    pub async fn listen_for_auth_challenge(&self, evt: Arc<dyn AuthChallengeListener>) -> Result<(), AppError> {
        let listener = AuthChallengeListenerConversation::new(self.router.keypair().public_key(), self.router.keypair().subkey_proof().cloned());
        let id = self.router.add_conversation(Box::new(MultiKeyProxy::new(listener))).await?;

        let mut rx: DelayedReply<AuthChallengeEvent> = self.router.subscribe_to_service_request(id).await?;
        let response = rx.await_reply().await.ok_or(AppError::ListenerDisconnected)?.unwrap();
        log::debug!("Received auth challenge: {:?}", response);

        let result = evt.on_auth_challenge(response.clone()).await?;
        log::debug!("Auth challenge callback result: {:?}", result);

        if result {
            let approve = AuthResponseConversation::new(response, vec![], self.router.keypair().subkey_proof().cloned());
            let _ = self.router.add_conversation(Box::new(approve)).await?;
        } else {
            // TODO: send explicit rejection
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppError {
    #[error("Failed to connect to relay: {0}")]
    RelayError(String),

    #[error("Failed to send auth init: {0}")]
    ConversationError(String),

    #[error("Listener disconnected")]
    ListenerDisconnected,

    #[error("Callback error: {0}")]
    CallbackError(#[from] CallbackError),
}

impl From<portal::router::ConversationError> for AppError {
    fn from(error: portal::router::ConversationError) -> Self {
        AppError::ConversationError(error.to_string())
    }
}

impl From<portal::nostr_relay_pool::pool::Error> for AppError {
    fn from(error: portal::nostr_relay_pool::pool::Error) -> Self {
        AppError::RelayError(error.to_string())
    }
}