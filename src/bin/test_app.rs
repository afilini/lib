use std::{io::Write, str::FromStr, sync::Arc};

use nostr::{
    Keys,
    event::{Kind, Tag},
    nips::nip19::ToBech32,
};
use nostr_relay_pool::{RelayOptions, RelayPool};
use portal::{
    protocol::{
        LocalKeypair,
        auth_init::AuthInitUrl,
        model::{
            auth::{AuthInitContent, ClientInfo},
            event_kinds::AUTH_INIT,
        },
    },
    router::{Conversation, MessageRouter, Response},
};

struct AuthInitConversation {
    url: AuthInitUrl,
    relays: Vec<String>,
}

impl Conversation for AuthInitConversation {
    fn init(&self) -> Result<portal::router::Response, portal::router::ConversationError> {
        let content = AuthInitContent {
            token: self.url.token.clone(),
            client_info: ClientInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                name: "Portal".to_string(),
            },
            preferred_relays: self.relays.clone(),
        };

        let tags = self
            .url
            .all_keys()
            .iter()
            .map(|k| Tag::public_key(*k))
            .collect();
        let response = Response::new()
            .reply_to(self.url.send_to(), Kind::from(AUTH_INIT), tags, content)
            .finish();

        Ok(response)
    }

    fn on_message(
        &mut self,
        _message: portal::router::ConversationMessage,
    ) -> Result<portal::router::Response, portal::router::ConversationError> {
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
    router.add_conversation(Box::new(conv)).await?;

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
