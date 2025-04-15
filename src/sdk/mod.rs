use nostr::key::PublicKey;

use crate::{
    model::{
        Timestamp,
        auth::{AuthChallengeContent, AuthInitContent, AuthResponseContent},
    },
    protocol::auth_init::AuthInitUrl,
    router::connector::{Connector, DelayedReply},
    utils::random_string,
};

pub trait SDKMethods {
    fn init_session(
        &self,
    ) -> impl std::future::Future<Output = (AuthInitUrl, DelayedReply<AuthInitContent>)> + Send;

    fn request_login(
        &self,
        main_key: PublicKey,
        subkeys: Vec<PublicKey>,
        relays: Vec<String>,
    ) -> impl std::future::Future<
        Output = Result<DelayedReply<AuthResponseContent>, crate::router::connector::Error>,
    > + Send;
}

impl SDKMethods for Connector {
    async fn init_session(&self) -> (AuthInitUrl, DelayedReply<AuthInitContent>) {
        let relays = self
            .relays()
            .relays()
            .await
            .keys()
            .map(|r| r.to_string())
            .collect::<Vec<_>>();

        let (main_key, subkey) = if let Some(subkey_proof) = self.keypair().subkey_proof() {
            (subkey_proof.main_key, Some(self.keypair().public_key()))
        } else {
            (self.keypair().public_key(), None)
        };

        // Generate a random token
        let token = random_string(20);

        let mut router = self.router().lock().await;

        let id = router
            .new_service_request::<crate::router::sdk::AuthPing>(
                self.keypair().public_key(),
                vec![],
                token.clone(),
            )
            .unwrap();
        let rx = router
            .subscribe_to_service_request::<AuthInitContent>(id.clone())
            .unwrap();

        (
            AuthInitUrl {
                main_key,
                relays,
                token,
                subkey,
            },
            rx,
        )
    }

    async fn request_login(
        &self,
        main_key: PublicKey,
        subkeys: Vec<PublicKey>,
        relays: Vec<String>,
    ) -> Result<DelayedReply<AuthResponseContent>, crate::router::connector::Error> {
        let mut router = self.router().lock().await;

        let challenge = AuthChallengeContent {
            challenge: random_string(32),
            expires_at: Timestamp::now_plus_seconds(60),
            required_permissions: vec![],
            subkey_proof: self.keypair().subkey_proof().cloned(),
        };

        // TODO: connect to relays

        let id = router
            .new_service_request::<crate::router::sdk::AuthRequest>(main_key, subkeys, challenge)
            .unwrap();
        let rx = router
            .subscribe_to_service_request::<AuthResponseContent>(id.clone())
            .unwrap();

        Ok(rx)
    }
}
