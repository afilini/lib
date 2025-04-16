use std::sync::Arc;

use nostr::{Keys, event::Kind, filter::Filter};
use nostr_relay_pool::{RelayOptions, RelayPool};
use portal::{
    model::{auth::AuthInitContent, event_kinds::AUTH_INIT},
    protocol::{LocalKeypair, auth_init::AuthInitUrl},
    router::{Conversation, ConversationError, DelayedReply, MessageRouter, Response},
    utils::random_string,
};
// use portal::{protocol::LocalKeypair, router::connector::Connector, sdk::SDKMethods};

struct AuthInitReceiverConversation {
    token: String,
}

impl Conversation for AuthInitReceiverConversation {
    fn init(&self) -> Result<Response, ConversationError> {
        Ok(Response::new().filter(Filter::new().kinds(vec![Kind::from(AUTH_INIT)])))
    }

    fn on_message(
        &mut self,
        message: portal::router::ConversationMessage,
    ) -> Result<Response, ConversationError> {
        log::debug!("Received message: {:?}", message);

        match message {
            portal::router::ConversationMessage::Encrypted(_) => return Ok(Response::default()),
            portal::router::ConversationMessage::Cleartext(event) => {
                let content = serde_json::from_value::<AuthInitContent>(event.content).unwrap();
                if content.token == self.token {
                    let response = Response::new()
                        .notify(serde_json::json!({
                            "token": self.token,
                        }))
                        .finish();

                    return Ok(response);
                }
            }
        }

        Ok(Response::default())
    }

    fn is_expired(&self) -> bool {
        false
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec12pmups7th4hhwt8e4h7v90s039yfuvywev33vuw9y4s9e8pnxgaq9gpdsf")?;
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    relay_pool
        .add_relay("wss://relay.damus.io", RelayOptions::default())
        .await?;
    relay_pool
        .add_relay("wss://relay.nostr.net", RelayOptions::default())
        .await?;
    relay_pool.connect().await;

    let router = Arc::new(MessageRouter::new(relay_pool, keypair));
    let _router = Arc::clone(&router);
    tokio::spawn(async move {
        _router.listen().await;
    });

    let relays = router
        .channel()
        .relays()
        .await
        .keys()
        .map(|r| r.to_string())
        .collect::<Vec<_>>();

    let (main_key, subkey) = if let Some(subkey_proof) = router.keypair().subkey_proof() {
        (subkey_proof.main_key, Some(router.keypair().public_key()))
    } else {
        (router.keypair().public_key(), None)
    };
    // Generate a random token
    let token = random_string(20);

    let url = AuthInitUrl {
        main_key,
        relays,
        token: token.clone(),
        subkey,
    };

    log::info!("Auth init URL: {}", url);

    let conv = AuthInitReceiverConversation { token };
    let id = router.add_conversation(Box::new(conv)).await?;
    log::debug!("Added conversation with id: {}", id);
    let mut event: DelayedReply<serde_json::Value> =
        router.subscribe_to_service_request(id).await?;
    log::debug!("Waiting for notification...");
    log::debug!(
        "Received notification: {:?}",
        event.await_reply().await.unwrap()
    );

    // handle.await?;

    //
    //     // Create the authenticator
    //     let authenticator = Connector::new(keypair, relay_pool);
    //
    //     // Bootstrap the authenticator
    //     authenticator.bootstrap().await.unwrap();
    //
    //     // Initialize a new session
    //     let (session, mut rx) = authenticator.init_session().await;
    //     println!("Session token: {}", session.token);
    //     println!("Portal URL: {}", session.to_string());
    //
    //     let _authenticator = authenticator.clone();
    //     tokio::spawn(async move {
    //         while let Some(Ok(event)) = rx.next().await {
    //             println!("Event: {:?}", event);
    //             let mut rx = _authenticator
    //                 .request_login(event.pubkey, vec![], vec![])
    //                 .await
    //                 .unwrap();
    //             println!("Login response: {:?}", rx.await_reply().await.unwrap());
    //         }
    //     });
    //
    //     // Process events
    //     println!("Processing events... Press Ctrl+C to exit");
    //     let _authenticator = Arc::clone(&authenticator);
    //     tokio::spawn(async move {
    //         _authenticator.process_incoming_events().await.unwrap();
    //     });
    //
    //     authenticator.process_outgoing_events().await.unwrap();

    Ok(())
}
