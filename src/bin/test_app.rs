use std::{io::Write, str::FromStr, sync::Arc};

use nostr::{Keys, nips::nip19::ToBech32};
use nostr_relay_pool::{RelayOptions, RelayPool};
use portal::{
    app::auth::{
        AuthChallengeListenerConversation, AuthInitConversation, AuthResponseConversation,
    },
    protocol::{LocalKeypair, auth_init::AuthInitUrl},
    router::{MessageRouter, MultiKeyListenerAdapter, adapters::one_shot::OneShotSenderAdapter},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create a new keypair
    // let keys = Keys::generate();
    let keys = Keys::parse("nsec1w86jfju9yfpfxtcr6mhqmqrstzdvckkyrthdccdmqhk3xakvt3sqy5ud2k")?;
    println!(
        "Running with keys: {}",
        keys.public_key.to_bech32().unwrap()
    );
    let keypair = LocalKeypair::new(keys, None);

    // Create a relay pool with some test relays
    let relay_pool = RelayPool::new();
    // relay_pool.add_relay("wss://relay.damus.io", RelayOptions::default()).await?;
    relay_pool
        .add_relay("wss://relay.nostr.net", RelayOptions::default())
        .await?;
    relay_pool.connect().await;

    let router = Arc::new(MessageRouter::new(relay_pool, keypair));
    let _router = Arc::clone(&router);
    tokio::spawn(async move {
        _router.listen().await.unwrap();
    });

    print!("Enter the auth init URL: ");
    std::io::stdout().flush()?;

    let mut auth_init_url = String::new();
    std::io::stdin().read_line(&mut auth_init_url)?;
    let auth_init_url = AuthInitUrl::from_str(auth_init_url.trim())?;

    // send auth init
    let conv = AuthInitConversation {
        url: auth_init_url,
        relays: router
            .channel()
            .relays()
            .await
            .keys()
            .map(|r| r.to_string())
            .collect(),
    };
    router
        .add_conversation(Box::new(OneShotSenderAdapter::new_with_user(
            conv.url.send_to(),
            conv.url.subkey.map(|s| vec![s.into()]).unwrap_or_default(),
            conv,
        )))
        .await?;
    // auth init sent

    let inner = AuthChallengeListenerConversation::new(router.keypair().public_key());
    let mut rx = router
        .add_and_subscribe(MultiKeyListenerAdapter::new(
            inner,
            router.keypair().subkey_proof().cloned(),
        ))
        .await?;

    while let Ok(response) = rx.next().await.unwrap() {
        log::debug!("Received auth challenge: {:?}", response);

        // ask the user to approve or reject the auth challenge
        print!("Approve auth challenge? (y/n): ");
        std::io::stdout().flush()?;
        let mut approve = String::new();
        std::io::stdin().read_line(&mut approve)?;
        let approve = approve.trim().to_lowercase() == "y";

        // let result = evt.on_auth_challenge(response.clone()).await?;

        log::debug!("Auth challenge callback result: {:?}", approve);

        if approve {
            let approve = AuthResponseConversation::new(
                response.clone(),
                vec![],
                router.keypair().subkey_proof().cloned(),
            );
            router
                .add_conversation(Box::new(OneShotSenderAdapter::new_with_user(
                    response.recipient.into(),
                    vec![],
                    approve,
                )))
                .await?;
        } else {
            // TODO: send explicit rejection
        }
    }

    Ok(())
    //
    //     // Create the authenticator
    //     let service = Connector::new(keypair, relay_pool);
    //
    //     // Bootstrap the authenticator
    //     service.bootstrap().await.unwrap();
    //
    //     let _service = Arc::clone(&service);
    //     tokio::spawn(async move {
    //         _service.process_incoming_events().await.unwrap();
    //     });
    //
    //
    //     let _service = Arc::clone(&service);
    //     tokio::spawn(async move {
    //         _service.send_auth_init(auth_init_url).await.unwrap();
    //     });
    //     let _service = Arc::clone(&service);
    //     tokio::spawn(async move {
    //         let mut rx = _service.listen_for_auth_request().await.unwrap();
    //         let request = rx.await_reply().await.unwrap().unwrap();
    //         log::info!("Received auth request: {:?}", request);
    //
    //         _service.auth_response(request.content, true).await.unwrap();
    //     });
    //
    //     // Process events
    //     println!("Processing events... Press Ctrl+C to exit");
    //     service.process_outgoing_events().await.unwrap();
    //
    //     Ok(())
}
