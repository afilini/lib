use std::{io::Write, str::FromStr, sync::Arc};

use app::{
    AuthChallengeListener, CallbackError, Mnemonic, PortalApp, handlers::AuthChallengeEvent,
};
use portal::{nostr::nips::nip19::ToBech32, protocol::auth_init::AuthInitUrl};

struct ApproveLogin;

#[async_trait::async_trait]
impl AuthChallengeListener for ApproveLogin {
    async fn on_auth_challenge(&self, event: AuthChallengeEvent) -> Result<bool, CallbackError> {
        log::info!("Received auth challenge: {:?}", event);
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        log::info!("Approving login");
        Ok(true)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mnemonic = Mnemonic::new(
        "mass derive myself benefit shed true girl orange family spawn device theme",
    )?;
    let keypair = mnemonic.get_keypair()?;

    log::info!(
        "Public key: {:?}",
        keypair.public_key().to_bech32().unwrap()
    );

    let app = PortalApp::new(
        Arc::new(keypair),
        vec![
            "wss://relay.nostr.net".to_string(),
            "wss://relay.damus.io".to_string(),
        ],
    )
    .await?;
    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        _app.listen().await.unwrap();
    });

    print!("Enter the auth init URL: ");
    std::io::stdout().flush()?;

    let handle = app.listen_for_auth_challenge(Arc::new(ApproveLogin));

    let mut auth_init_url = String::new();
    std::io::stdin().read_line(&mut auth_init_url)?;
    let url = AuthInitUrl::from_str(auth_init_url.trim())?;
    app.send_auth_init(url).await?;

    handle.await?;
    Ok(())
}
