use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::{Duration, SystemTime},
};

use nostr::{event::{EventBuilder, EventId, Kind, Tag}, filter::Filter, nips::{nip19::ToBech32, nip44}};
use nostr_relay_pool::{RelayOptions, RelayPool, RelayPoolNotification, SubscribeOptions};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::{model::{auth::{AuthChallengeContent, AuthInitContent, ClientInfo, SubkeyProof}, event_kinds::*, Timestamp}, protocol::{auth_init::AuthInitUrl, LocalKeypair}, router::{connector::{Connector, DelayedReply}, CleartextEvent, MessageRouter, OutgoingEvent, RelayAction}};

pub trait AppMethods {
    fn send_auth_init(&self, auth_init_url: AuthInitUrl) -> impl std::future::Future<Output = Result<(), crate::router::connector::Error>> + Send;

    fn listen_for_auth_request(&self) -> impl std::future::Future<Output = Result<DelayedReply<AuthRequestData>, crate::router::connector::Error>> + Send;

    fn auth_response(&self, request: AuthRequestData, accept: bool) -> impl std::future::Future<Output = Result<(), crate::router::connector::Error>> + Send;
}

impl AppMethods for Connector {
    async fn send_auth_init(&self, auth_init_url: AuthInitUrl) -> Result<(), crate::router::connector::Error> {
        let content = AuthInitContent {
            token: auth_init_url.token.clone(),
            client_info: ClientInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                name: "Portal".to_string(),
            },
            preferred_relays: self.relays().relays().await.keys().map(|r| r.to_string()).collect(),
        };

        let mut router = self.router().lock().await;

        router
            .new_service_request::<crate::router::app::SendAuthInit>(
                auth_init_url.send_to(),
                auth_init_url.all_keys(),
                content,
            )
            .unwrap();

        // TODO: connect to relays

        Ok(())
    }

    async fn listen_for_auth_request(&self) -> Result<DelayedReply<AuthRequestData>, crate::router::connector::Error> {
        let mut router = self.router().lock().await;

        let id = router
            .new_service_request::<crate::router::app::AuthListener>(
                self.keypair().public_key(),
                vec![],
                self.keypair().subkey_proof().cloned(),
            )
            .unwrap();
        let rx = router
            .subscribe_to_service_request(id.clone())
            .unwrap();

        Ok(rx)
    }

    async fn auth_response(&self, request: AuthRequestData, accept: bool) -> Result<(), crate::router::connector::Error> {
        let mut router = self.router().lock().await;

        router.new_service_request::<crate::router::app::AuthChallengeResponse>(
            request.from,
            vec![], // TODO: implement subkeys
            (accept, request.challenge, request.subkey_proof, request.event_id),
        ).unwrap();

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequestData {
    pub from: nostr::PublicKey,
    pub expires_at: Timestamp,
    pub required_permissions: Vec<String>,

    event_id: EventId,
    subkey_proof: Option<SubkeyProof>,
    challenge: String,
}

impl From<(AuthChallengeContent, EventId, nostr::PublicKey)> for AuthRequestData {
    fn from((challenge, event_id, pubkey): (AuthChallengeContent, EventId, nostr::PublicKey)) -> Self {
        Self {
            from: pubkey,
            expires_at: challenge.expires_at.into(),
            required_permissions: challenge.required_permissions,
            event_id,
            subkey_proof: challenge.subkey_proof,
            challenge: challenge.challenge,
        }
    }
}