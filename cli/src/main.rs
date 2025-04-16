use std::{io::Write, str::FromStr, sync::Arc};

use app::{handlers::AuthChallengeEvent, AuthChallengeListener, CallbackError, Keypair, PortalApp};
use portal::protocol::auth_init::AuthInitUrl;

struct ApproveLogin;

#[async_trait::async_trait]
impl AuthChallengeListener for ApproveLogin {
    async fn on_auth_challenge(&self, event: AuthChallengeEvent) -> Result<bool, CallbackError> {
        log::info!("Received auth challenge: {:?}", event);
        Ok(true)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let keypair = Keypair::new("nsec1w86jfju9yfpfxtcr6mhqmqrstzdvckkyrthdccdmqhk3xakvt3sqy5ud2k", None)?;

    let app = PortalApp::new(Arc::new(keypair), vec!["wss://relay.nostr.net".to_string()]).await?;
    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        _app.listen().await.unwrap();
    });

    print!("Enter the auth init URL: ");
    std::io::stdout().flush()?;

    let mut auth_init_url = String::new();
    std::io::stdin().read_line(&mut auth_init_url)?;
    let url = AuthInitUrl::from_str(auth_init_url.trim())?;
    app.send_auth_init(url).await?;

    app.listen_for_auth_challenge(Arc::new(ApproveLogin)).await?;

    Ok(())
}
