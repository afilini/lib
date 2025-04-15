use crate::{
    app::AppMethods, protocol::LocalKeypair, router::connector::Connector, sdk::SDKMethods,
};
use nostr::key::Keys;
use nostr_relay_pool::{RelayOptions, RelayPool};
use std::sync::Arc;

async fn create_app() -> Result<Arc<Connector>, Box<dyn std::error::Error>> {
    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec1w86jfju9yfpfxtcr6mhqmqrstzdvckkyrthdccdmqhk3xakvt3sqy5ud2k")?;
    log::info!("App running with keys: {:?}", keys.public_key);
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    // relay_pool.add_relay("wss://relay.damus.io", RelayOptions::default()).await?;
    relay_pool
        .add_relay("wss://relay.nostr.net", RelayOptions::default())
        .await?;

    // Create the authenticator
    let service = Connector::new(keypair, relay_pool);

    // Bootstrap the authenticator
    service.bootstrap().await.unwrap();

    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.process_incoming_events().await.unwrap();
    });
    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.process_outgoing_events().await.unwrap();
    });

    Ok(service)
}

async fn create_service() -> Result<Arc<Connector>, Box<dyn std::error::Error>> {
    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec12pmups7th4hhwt8e4h7v90s039yfuvywev33vuw9y4s9e8pnxgaq9gpdsf")?;
    log::info!("Service running with keys: {:?}", keys.public_key);
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    // relay_pool.add_relay("wss://relay.damus.io", RelayOptions::default()).await?;
    relay_pool
        .add_relay("wss://relay.nostr.net", RelayOptions::default())
        .await?;

    // Create the authenticator
    let service = Connector::new(keypair, relay_pool);

    // Bootstrap the authenticator
    service.bootstrap().await.unwrap();

    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.process_incoming_events().await.unwrap();
    });
    let _service = Arc::clone(&service);
    tokio::spawn(async move {
        _service.process_outgoing_events().await.unwrap();
    });

    Ok(service)
}

#[tokio::test]
async fn test_basic() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let app = create_app().await?;
    let service = create_service().await?;

    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        let mut rx = _app.listen_for_auth_request().await.unwrap();
        let request = rx.await_reply().await.unwrap().unwrap();
        log::info!("Auth request = {:?}", request);

        _app.auth_response(request.content, true).await.unwrap();
    });

    let (session, mut rx) = service.init_session().await;
    log::info!("Auth init URL: {}", session);
    app.send_auth_init(session).await?;

    let app_ping = rx.await_reply().await.unwrap()?;
    log::info!("User public key: {}", app_ping.pubkey);

    let mut response = service
        .request_login(app_ping.pubkey, vec![], app_ping.content.preferred_relays)
        .await?;
    log::info!("Login response = {:?}", response.await_reply().await);

    Ok(())
}
