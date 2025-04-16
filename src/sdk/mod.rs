use nostr::key::PublicKey;
use nostr_relay_pool::RelayPool;

use crate::{
    model::{
        Timestamp,
        auth::{AuthChallengeContent, AuthInitContent, AuthResponseContent},
    },
    protocol::auth_init::AuthInitUrl,
    router::MessageRouter,
    utils::random_string,
};

struct SDK {
    router: MessageRouter<RelayPool>,
}

impl SDK {
    // pub async fn init_session(&self) -> (AuthInitUrl, DelayedReply<AuthInitContent>) {
    //     let relays = self
    //         .router
    //         .channel()
    //         .relays()
    //         .await
    //         .keys()
    //         .map(|r| r.to_string())
    //         .collect::<Vec<_>>();

    //     let (main_key, subkey) = if let Some(subkey_proof) = self.router.keypair().subkey_proof() {
    //         (subkey_proof.main_key, Some(self.router.keypair().public_key()))
    //     } else {
    //         (self.router.keypair().public_key(), None)
    //     };

    //     // Generate a random token
    //     let token = random_string(20);

    //     let id = router
    //         .new_service_request::<crate::router::sdk::AuthPing>(
    //             self.keypair().public_key(),
    //             vec![],
    //             token.clone(),
    //         )
    //         .unwrap();
    //     let rx = router
    //         .subscribe_to_service_request::<AuthInitContent>(id.clone())
    //         .unwrap();

    //     (
    //         AuthInitUrl {
    //             main_key,
    //             relays,
    //             token,
    //             subkey,
    //         },
    //         rx,
    //     )
    // }

    // pub async fn request_login(
    //     &self,
    //     main_key: PublicKey,
    //     subkeys: Vec<PublicKey>,
    //     relays: Vec<String>,
    // ) -> Result<DelayedReply<AuthResponseContent>, crate::router::connector::Error> {
    //     let mut router = self.router().lock().await;

    //     let challenge = AuthChallengeContent {
    //         challenge: random_string(32),
    //         expires_at: Timestamp::now_plus_seconds(60),
    //         required_permissions: vec![],
    //         subkey_proof: self.keypair().subkey_proof().cloned(),
    //     };

    //     // TODO: connect to relays

    //     let id = router
    //         .new_service_request::<crate::router::sdk::AuthRequest>(main_key, subkeys, challenge)
    //         .unwrap();
    //     let rx = router
    //         .subscribe_to_service_request::<AuthResponseContent>(id.clone())
    //         .unwrap();

    //     Ok(rx)
    // }
}
