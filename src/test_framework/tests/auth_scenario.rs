use nostr::Keys;
use crate::{
    app::handlers::{AuthChallengeEvent, AuthChallengeListenerConversation, AuthInitConversation, AuthResponseConversation}, protocol::{
        auth_init::AuthInitUrl,
        LocalKeypair,
    }, router::{MultiKeyListenerAdapter, MultiKeySenderAdapter}, sdk::handlers::{AuthChallengeSenderConversation, AuthInitEvent, AuthInitReceiverConversation, AuthResponseEvent}, test_framework::ScenarioBuilder, utils::random_string
};

#[tokio::test]
async fn test_auth_flow() {
    env_logger::init();

    // Create keys for service and client
    let service_keys = Keys::generate();
    let client_keys = Keys::generate();

    // Create the auth init URL
    let token = random_string(32);
    let url = AuthInitUrl {
        main_key: service_keys.public_key().into(),
        relays: vec!["simulated".to_string()],
        token: token.clone(),
        subkey: None,
    };

    // Create the network with both nodes
    let network = ScenarioBuilder::new()
        .with_node("service".to_string(), LocalKeypair::new(service_keys.clone(), None)).await
        .with_node("client".to_string(), LocalKeypair::new(client_keys.clone(), None)).await
        .run()
        .await;

    // Get the routers
    let service_router = network.get_node("service").unwrap();
    let client_router = network.get_node("client").unwrap();

    // 1. Service sets up to receive auth init
    let mut service_notifications = service_router
        .add_and_subscribe(MultiKeyListenerAdapter::new(
            AuthInitReceiverConversation::new(service_keys.public_key(), token.clone()),
            None,
        ))
        .await
        .unwrap();

    // 2. Client sets up to listen for auth challenge
    let mut challenge_notifications = client_router
        .add_and_subscribe(MultiKeyListenerAdapter::new(
            AuthChallengeListenerConversation::new(client_keys.public_key()),
            None,
        ))
        .await
        .unwrap();

    // 3. Client initiates auth flow
    let auth_init = AuthInitConversation {
        url,
        relays: vec!["simulated".to_string()],
    };
    client_router
        .add_conversation(Box::new(auth_init))
        .await
        .unwrap();

    // 4. Service receives auth init
    let auth_init_event = service_notifications.await_reply().await.unwrap().unwrap();
    assert_eq!(auth_init_event.main_key, client_keys.public_key());

    // 5. Service sends auth challenge
    let mut auth_response_event = service_router.add_and_subscribe(MultiKeySenderAdapter::new_with_user(auth_init_event.main_key, vec![], AuthChallengeSenderConversation::new(service_keys.public_key(), None))).await.unwrap();

    // 6. Client receives auth challenge
    let auth_challenge_event = challenge_notifications.await_reply().await.unwrap().unwrap();
    assert_eq!(auth_challenge_event.service_key, service_keys.public_key().into());
    assert_eq!(auth_challenge_event.recipient, service_keys.public_key().into());

    // 7. Clients accepts requrest
    let approve = AuthResponseConversation::new(auth_challenge_event, vec![], None);
    client_router.add_conversation(Box::new(approve)).await.unwrap();

    // 8. Wait for auth response notification
    let auth_response_event = auth_response_event.await_reply().await.unwrap().unwrap();
    assert_eq!(auth_response_event.user_key, client_keys.public_key());
    assert_eq!(auth_response_event.recipient, client_keys.public_key().into());
} 