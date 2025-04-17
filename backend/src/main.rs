#[macro_use]
extern crate rocket;

use portal::nostr::nips::nip19::ToBech32;
use portal::nostr::Keys;
use portal::protocol::LocalKeypair;
use portal::router::{DelayedReply, MultiKeyProxy, WrappedContent};
use portal::sdk::handlers::{
    AuthChallengeSenderConversation, AuthInitEvent, AuthInitReceiverConversation, AuthResponseEvent,
};
use portal::{
    nostr_relay_pool::{RelayOptions, RelayPool},
    protocol::auth_init::AuthInitUrl,
    router::MessageRouter,
};
use rocket::{
    fairing::AdHoc,
    form::Form,
    http::{Cookie, CookieJar},
    response::Redirect,
    tokio, State,
};
use rocket_dyn_templates::{context, Template};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct User {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Session {
    user_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum LoginStatus {
    WaitingForInit,
    SendingChallenge,
    Approved(String),
    Timeout,
}

type Sessions = Mutex<HashMap<String, Session>>;
type Users = Mutex<HashMap<String, User>>;
type LoginTokens = Arc<Mutex<HashMap<String, LoginStatus>>>;

#[get("/")]
async fn index(
    cookies: &CookieJar<'_>,
    sessions: &State<Sessions>,
    users: &State<Users>,
    login_tokens: &State<LoginTokens>,
    router: &State<Arc<MessageRouter<RelayPool>>>,
) -> Result<Template, Redirect> {
    if let Some(session_id) = cookies.get("session_id") {
        if let Some(session) = sessions.lock().unwrap().get(session_id.value()) {
            if session.expires_at > chrono::Utc::now() {
                if let Some(user) = users.lock().unwrap().get(&session.user_id) {
                    return Ok(Template::render("dashboard", context! { name: &user.name }));
                }
            }
        }
    }

    // Check for existing login token in cookies
    let token = if let Some(token_cookie) = cookies.get("login_token") {
        let login_tokens = login_tokens.lock().unwrap();

        if !login_tokens.contains_key(token_cookie.value()) {
            cookies.remove(Cookie::named("login_token"));
            return Err(Redirect::to("/"));
        }

        token_cookie.value().to_string()
    } else {
        // Generate a new login token
        let new_token = Uuid::new_v4().to_string();
        cookies.add(Cookie::new("login_token", new_token.clone()));

        log::info!("Generating new login token");

        let router = Arc::clone(&router);
        let _login_tokens = Arc::clone(&login_tokens);
        let _token = new_token.clone();
        tokio::spawn(async move {
            let inner =
                AuthInitReceiverConversation::new(router.keypair().public_key(), _token.clone());
            let id = router
                .add_conversation(Box::new(MultiKeyProxy::new(inner)))
                .await
                .unwrap();
            let event: WrappedContent<AuthInitEvent> = router
                .subscribe_to_service_request(id)
                .await
                .unwrap()
                .await_reply()
                .await
                .unwrap()
                .unwrap();

            log::info!("Got auth init event");

            _login_tokens
                .lock()
                .unwrap()
                .insert(_token.clone(), LoginStatus::SendingChallenge);

            log::info!("Sending auth challenge");

            let conv = AuthChallengeSenderConversation::new(
                event.main_key,
                vec![],
                router.keypair().public_key(),
                router.keypair().subkey_proof().cloned(),
            );

            log::info!("Before adding conversation");
            let id = router
                .add_conversation(Box::new(MultiKeyProxy::new(conv)))
                .await
                .unwrap();

            log::info!("Subscribed to auth response event");
            let event: WrappedContent<AuthResponseEvent> = router
                .subscribe_to_service_request(id)
                .await
                .unwrap()
                .await_reply()
                .await
                .unwrap()
                .unwrap();

            log::info!("Got auth response event");

            _login_tokens.lock().unwrap().insert(
                _token.clone(),
                LoginStatus::Approved(event.user_key.to_bech32().unwrap()),
            );
        });

        login_tokens
            .lock()
            .unwrap()
            .insert(new_token.clone(), LoginStatus::WaitingForInit);

        new_token
    };

    // Check if we already have an approved status
    let status = login_tokens.lock().unwrap().get(&token).cloned();
    if let Some(LoginStatus::Approved(user_key)) = &status {
        let session_id = Uuid::new_v4().to_string();

        let user = User {
            id: user_key.clone(),
            name: user_key.clone(),
        };

        let session = Session {
            user_id: user_key.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
        };

        users.lock().unwrap().insert(user_key.clone(), user);
        sessions.lock().unwrap().insert(session_id.clone(), session);
        login_tokens.lock().unwrap().remove(&token);

        // Remove the login token cookie and set session cookie
        cookies.remove(Cookie::named("login_token"));
        cookies.add(Cookie::new("session_id", session_id));

        // Redirect to dashboard
        return Err(Redirect::to("/"));
    }

    let relays = router
        .channel()
        .relays()
        .await
        .keys()
        .map(|r| r.to_string())
        .collect::<Vec<_>>();

    let (main_key, subkey) = if let Some(subkey_proof) = router.keypair().subkey_proof() {
        (
            subkey_proof.main_key.into(),
            Some(router.keypair().public_key()),
        )
    } else {
        (router.keypair().public_key(), None)
    };

    let url = AuthInitUrl {
        main_key: main_key.into(),
        relays,
        token: token.clone(),
        subkey: subkey.map(|k| k.into()),
    };

    Ok(Template::render(
        "login",
        context! {
            login_url: url.to_string(),
            status: status,
        },
    ))
}

#[get("/logout")]
fn logout(cookies: &CookieJar<'_>, sessions: &State<Sessions>) -> Redirect {
    if let Some(session_id) = cookies.get("session_id") {
        sessions.lock().unwrap().remove(session_id.value());
        cookies.remove(Cookie::named("session_id"));
    }
    Redirect::to("/")
}

#[launch]
async fn rocket() -> _ {
    env_logger::init();

    let keys = Keys::generate();
    let keypair = LocalKeypair::new(keys, None);

    let relay_pool = RelayPool::new();
    // relay_pool
    //     .add_relay("wss://relay.damus.io", RelayOptions::default())
    //     .await
    //     .unwrap();
    relay_pool
        .add_relay("wss://relay.nostr.net", RelayOptions::default())
        .await
        .unwrap();
    relay_pool.connect().await;

    let router = Arc::new(MessageRouter::new(relay_pool, keypair));
    let _router = Arc::clone(&router);
    tokio::spawn(async move {
        _router.listen().await.unwrap();
    });

    rocket::build()
        .attach(Template::fairing())
        .attach(AdHoc::on_ignite("Mount Routes", |rocket| async {
            rocket
                .mount("/", routes![index, logout])
                .manage(Sessions::new(HashMap::new()))
                .manage(Users::new(HashMap::new()))
                .manage(LoginTokens::new(Mutex::new(HashMap::new())))
                .manage(router)
        }))
}
