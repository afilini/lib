use std::sync::Arc;

use futures::StreamExt;
use nostr::Keys;
use nostr_relay_pool::{RelayOptions, RelayPool};
use portal::{model::auth, protocol::LocalKeypair, router::connector::Connector, sdk::SDKMethods};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec12pmups7th4hhwt8e4h7v90s039yfuvywev33vuw9y4s9e8pnxgaq9gpdsf")?;
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    relay_pool.add_relay("wss://relay.damus.io", RelayOptions::default()).await?;
    relay_pool.add_relay("wss://relay.nostr.net", RelayOptions::default()).await?;

    // Create the authenticator
    let authenticator = Connector::new(keypair, relay_pool);

    // Bootstrap the authenticator
    authenticator.bootstrap().await.unwrap();

    // Initialize a new session
    let (session, mut rx) = authenticator.init_session().await;
    println!("Session token: {}", session.token);
    println!("Portal URL: {}", session.to_string());

    let _authenticator = authenticator.clone();
    tokio::spawn(async move {
        while let Some(Ok(event)) = rx.next().await {
            println!("Event: {:?}", event);
            let mut rx = _authenticator.request_login(event.pubkey, vec![], vec![]).await.unwrap();
        }
    });

    // Process events
    println!("Processing events... Press Ctrl+C to exit");
    let _authenticator = Arc::clone(&authenticator);
    tokio::spawn(async move {
        _authenticator.process_incoming_events().await.unwrap();
    });

    authenticator.process_outgoing_events().await.unwrap();

    Ok(())
} 