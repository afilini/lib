use std::{io::Write, str::FromStr, sync::Arc};

use nostr::{nips::nip19::ToBech32, Keys};
use nostr_relay_pool::{RelayOptions, RelayPool};
use portal::{app::AppMethods, protocol::{auth_init::AuthInitUrl, LocalKeypair}, router::connector::Connector};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec1w86jfju9yfpfxtcr6mhqmqrstzdvckkyrthdccdmqhk3xakvt3sqy5ud2k")?;
    println!("Running with keys: {}", keys.public_key.to_bech32().unwrap());
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    // relay_pool.add_relay("wss://relay.damus.io", RelayOptions::default()).await?;
    relay_pool.add_relay("wss://relay.nostr.net", RelayOptions::default()).await?;

    // Create the authenticator
    let service = Connector::new(keypair, relay_pool);

    // Bootstrap the authenticator
    service.bootstrap().await.unwrap();

    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.process_incoming_events().await.unwrap();
    });

    print!("Enter the auth init URL: ");
    std::io::stdout().flush()?;

    let mut auth_init_url = String::new();
    std::io::stdin().read_line(&mut auth_init_url)?;
    let auth_init_url = AuthInitUrl::from_str(auth_init_url.trim())?;

    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.send_auth_init(auth_init_url).await.unwrap();
    });
    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        let mut rx =_service.listen_for_auth_request().await.unwrap();
        let request = rx.await_reply().await.unwrap().unwrap();
        log::info!("Received auth request: {:?}", request);

        _service.auth_response(request.content, true).await.unwrap();
    });

    // Process events
    println!("Processing events... Press Ctrl+C to exit");
    service.process_outgoing_events().await.unwrap();

    Ok(())
} 