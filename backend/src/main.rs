#[macro_use]
extern crate rocket;

use portal::nostr::nips::nip19::ToBech32;
use portal::nostr::Keys;
use portal::protocol::calendar::{Calendar, CalendarWrapper};
use portal::protocol::model::bindings::PublicKey;
use portal::protocol::model::payment::{
    Currency, RecurrenceInfo, RecurringPaymentRequestContent, RecurringPaymentStatusContent,
    SinglePaymentRequestContent,
};
use portal::protocol::model::Timestamp;
use portal::protocol::LocalKeypair;
use portal::router::{MultiKeyListenerAdapter, MultiKeySenderAdapter, NotificationStream};
use portal::sdk::auth::{
    AuthChallengeSenderConversation, AuthInitEvent, AuthInitReceiverConversation, AuthResponseEvent,
};
use portal::sdk::payments::{
    RecurringPaymentRequestSenderConversation, SinglePaymentRequestSenderConversation,
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
    auth_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum LoginStatus {
    WaitingForInit,
    SendingChallenge(String),
    Approved(String, String),
    Timeout,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PaymentStatus {
    Pending,
    Approved { subscription_id: Option<String> },
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecurringPaymentInfo {
    pub subscription_id: String,
    pub user_id: portal::nostr::PublicKey,
    pub amount: u64,
    pub calendar: Calendar,
    pub auth_token: String,
    pub description: String,
}

type Sessions = Mutex<HashMap<String, Session>>;
type Users = Mutex<HashMap<String, User>>;
type LoginTokens = Arc<Mutex<HashMap<String, LoginStatus>>>;
type PaymentRequests = Arc<Mutex<HashMap<String, PaymentStatus>>>;
type RecurringPayments = Arc<Mutex<HashMap<String, (RecurringPaymentInfo, Timestamp)>>>;

#[derive(Debug, Deserialize, FromForm)]
struct PaymentRequestForm {
    pub amount: u64,
    pub description: String,
    pub payment_type: String,
    pub frequency: Option<String>,
}

#[get("/")]
async fn index(
    cookies: &CookieJar<'_>,
    sessions: &State<Sessions>,
    users: &State<Users>,
    login_tokens: &State<LoginTokens>,
    payment_requests: &State<PaymentRequests>,
    router: &State<Arc<MessageRouter<RelayPool>>>,
) -> Result<Template, Redirect> {
    if let Some(session_id) = cookies.get("session_id") {
        if let Some(session) = sessions.lock().unwrap().get(session_id.value()) {
            if session.expires_at > chrono::Utc::now() {
                if let Some(user) = users.lock().unwrap().get(&session.user_id) {
                    // Check if there's an ongoing payment request
                    let payment_status = if let Some(payment_id) = cookies.get("payment_id") {
                        if let Some(status) =
                            payment_requests.lock().unwrap().get(payment_id.value())
                        {
                            Some(status.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    return Ok(Template::render(
                        "dashboard",
                        context! {
                            name: &user.name,
                            payment_status: payment_status
                        },
                    ));
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
            let mut event = router
                .add_and_subscribe(MultiKeyListenerAdapter::new(
                    inner,
                    router.keypair().subkey_proof().cloned(),
                ))
                .await
                .unwrap();
            let event = event.next().await.unwrap().unwrap();

            log::info!("Got auth init event");

            _login_tokens.lock().unwrap().insert(
                _token.clone(),
                LoginStatus::SendingChallenge(event.main_key.to_bech32().unwrap()),
            );

            let conv = AuthChallengeSenderConversation::new(
                router.keypair().public_key(),
                router.keypair().subkey_proof().cloned(),
            );

            let mut event = router
                .add_and_subscribe(MultiKeySenderAdapter::new_with_user(
                    event.main_key,
                    vec![],
                    conv,
                ))
                .await
                .unwrap();
            let event = event.next().await.unwrap().unwrap();
            log::info!("Got auth response event");

            _login_tokens.lock().unwrap().insert(
                _token.clone(),
                LoginStatus::Approved(
                    event.user_key.to_bech32().unwrap(),
                    event.session_token.clone(),
                ),
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
    if let Some(LoginStatus::Approved(user_key, auth_token)) = &status {
        let session_id = Uuid::new_v4().to_string();

        let user = User {
            id: user_key.clone(),
            name: user_key.clone(),
        };

        let session = Session {
            user_id: user_key.clone(),
            auth_token: auth_token.clone(),
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
            payment_status: Option::<u64>::None,
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

#[post("/request-payment", data = "<form>")]
fn request_payment(
    cookies: &CookieJar<'_>,
    sessions: &State<Sessions>,
    payment_requests: &State<PaymentRequests>,
    recurring_payments: &State<RecurringPayments>,
    form: Form<PaymentRequestForm>,
    router: &State<Arc<MessageRouter<RelayPool>>>,
    nwc: &State<Arc<nwc::NWC>>,
) -> Result<Template, Redirect> {
    let session_id = match cookies.get("session_id") {
        Some(c) => c.value(),
        None => return Err(Redirect::to("/")),
    };

    let sessions_lock = sessions.lock().unwrap();
    let session = match sessions_lock.get(session_id) {
        Some(s) => s,
        None => {
            drop(sessions_lock);
            cookies.remove(Cookie::named("session_id"));
            return Err(Redirect::to("/"));
        }
    };

    if session.expires_at <= chrono::Utc::now() {
        drop(sessions_lock);
        cookies.remove(Cookie::named("session_id"));
        return Err(Redirect::to("/"));
    }

    cookies.remove(Cookie::named("payment_id"));

    // Generate a unique payment request ID
    let payment_id = Uuid::new_v4().to_string();

    // Store the payment request with pending status
    payment_requests
        .lock()
        .unwrap()
        .insert(payment_id.clone(), PaymentStatus::Pending);

    // Set payment request cookie
    cookies.add(Cookie::new("payment_id", payment_id.clone()));

    let auth_token = session.auth_token.clone();
    let user_id = session.user_id.clone();
    let router = Arc::clone(&router);
    let nwc = Arc::clone(&nwc);
    let payment_requests = Arc::clone(&payment_requests);
    let recurring_payments = Arc::clone(&recurring_payments);
    let _payment_id = payment_id.clone();
    let form_frequency = form.frequency.clone();
    let form_amount = form.amount;
    let form_payment_type = form.payment_type.clone();

    let user_id = user_id.parse().unwrap();

    tokio::spawn(async move {
        let status = match form_payment_type.as_str() {
            "recurring" => {
                let calendar = form_frequency
                    .unwrap()
                    .parse::<Calendar>()
                    .expect("Invalid frequency");

                let inner = RecurringPaymentRequestSenderConversation::new(
                    router.keypair().public_key(),
                    router.keypair().subkey_proof().cloned(),
                    RecurringPaymentRequestContent {
                        amount: form_amount * 1000,
                        currency: Currency::Millisats,
                        recurrence: RecurrenceInfo {
                            until: None,
                            calendar: CalendarWrapper::new(calendar.clone()),
                            max_payments: None,
                            first_payment_due: Timestamp::now(),
                        },
                        current_exchange_rate: None,
                        expires_at: Timestamp::now_plus_seconds(600),
                        auth_token: Some(auth_token.clone()),
                    },
                );
                let mut event = router
                    .add_and_subscribe(MultiKeySenderAdapter::new_with_user(user_id, vec![], inner))
                    .await
                    .unwrap();
                let event = event.next().await.unwrap().unwrap();
                log::info!("Got recurring payment status event: {:?}", event);

                match event {
                    RecurringPaymentStatusContent::Confirmed {
                        subscription_id, ..
                    } => {
                        // Store recurring payment info for periodic processing
                        recurring_payments.lock().unwrap().insert(
                            subscription_id.clone(),
                            (
                                RecurringPaymentInfo {
                                    subscription_id: subscription_id.clone(),
                                    user_id: user_id.clone(),
                                    amount: form_amount,
                                    calendar: calendar.clone(),
                                    auth_token: auth_token.clone(),
                                    description: form.description.clone(),
                                },
                                Timestamp::new(0),
                            ),
                        );

                        PaymentStatus::Approved {
                            subscription_id: Some(subscription_id),
                        }
                    }
                    RecurringPaymentStatusContent::Rejected { reason, .. } => {
                        log::warn!(
                            "Recurring payment rejected: {}",
                            reason.unwrap_or("Unknown reason".to_string())
                        );
                        PaymentStatus::Failed
                    }
                    RecurringPaymentStatusContent::Cancelled { reason, .. } => {
                        log::warn!(
                            "Recurring payment cancelled: {}",
                            reason.unwrap_or("Unknown reason".to_string())
                        );
                        PaymentStatus::Failed
                    }
                }
            }
            "single" => {
                let claim_result = claim_payment(
                    form_amount * 1000,
                    auth_token.clone(),
                    form.description.clone(),
                    None,
                    user_id.clone(),
                    router.clone(),
                    nwc.clone(),
                )
                .await;
                match claim_result {
                    Ok(_) => PaymentStatus::Approved {
                        subscription_id: None,
                    },
                    Err(e) => {
                        log::error!("Failed to claim payment: {}", e);
                        PaymentStatus::Failed
                    }
                }
            }
            _ => {
                log::error!("Invalid payment type: {}", form_payment_type);
                PaymentStatus::Failed
            }
        };

        payment_requests.lock().unwrap().insert(_payment_id, status);
    });

    return Err(Redirect::to(format!(
        "/payment-status?payment_id={}",
        payment_id
    )));
}

#[get("/payment-status?<payment_id>")]
async fn payment_status(
    cookies: &CookieJar<'_>,
    sessions: &State<Sessions>,
    payment_requests: &State<PaymentRequests>,
    payment_id: String,
) -> Result<Template, Redirect> {
    let session_id = match cookies.get("session_id") {
        Some(c) => c.value(),
        None => return Err(Redirect::to("/")),
    };

    let sessions_lock = sessions.lock().unwrap();
    let session = match sessions_lock.get(session_id) {
        Some(s) => s,
        None => {
            drop(sessions_lock);
            cookies.remove(Cookie::named("session_id"));
            return Err(Redirect::to("/"));
        }
    };

    if session.expires_at <= chrono::Utc::now() {
        drop(sessions_lock);
        cookies.remove(Cookie::named("session_id"));
        return Err(Redirect::to("/"));
    }

    let status = match payment_requests.lock().unwrap().get(&payment_id).cloned() {
        Some(status) => status,
        None => {
            cookies.remove(Cookie::named("payment_id"));
            return Err(Redirect::to("/"));
        }
    };

    Ok(Template::render(
        "dashboard",
        context! {
            name: &session.user_id,
            payment_status: status
        },
    ))
}

async fn claim_payment(
    amount_msat: u64,
    auth_token: String,
    description: String,
    subscription_id: Option<String>,
    user_id: portal::nostr::PublicKey,
    router: Arc<MessageRouter<RelayPool>>,
    nwc: Arc<nwc::NWC>,
) -> Result<(), nwc::Error> {
    let invoice = nwc
        .make_invoice(portal::nostr::nips::nip47::MakeInvoiceRequest {
            amount: amount_msat,
            description: Some(description),
            description_hash: None,
            expiry: None,
        })
        .await?;

    log::info!("Made invoice: {}", invoice.invoice);

    let inner = SinglePaymentRequestSenderConversation::new(
        router.keypair().public_key(),
        router.keypair().subkey_proof().cloned(),
        SinglePaymentRequestContent {
            amount: amount_msat,
            currency: Currency::Millisats,
            expires_at: Timestamp::now_plus_seconds(600),
            auth_token: Some(auth_token),
            current_exchange_rate: None,
            invoice: invoice.invoice.clone(),
            subscription_id,
        },
    );
    let _event = router
        .add_and_subscribe(MultiKeySenderAdapter::new_with_user(user_id, vec![], inner))
        .await
        .unwrap();
    // let event = event.next().await.unwrap().unwrap();

    for _ in 0..30 {
        let invoice = nwc
            .lookup_invoice(portal::nostr::nips::nip47::LookupInvoiceRequest {
                invoice: Some(invoice.invoice.clone()),
                payment_hash: None,
            })
            .await?;

        if invoice.settled_at.is_some() {
            return Ok(());
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    return Err(nwc::Error::Timeout);
}

// Process recurring payments that are due
async fn process_recurring_payments(
    recurring_payments: Arc<Mutex<HashMap<String, (RecurringPaymentInfo, Timestamp)>>>,
    router: Arc<MessageRouter<RelayPool>>,
    nwc: Arc<nwc::NWC>,
) {
    let mut payments_to_update = Vec::new();

    // Find payments that need processing
    {
        let lock = recurring_payments.lock().unwrap();
        for (_, (payment, last_payment)) in lock.iter() {
            match payment.calendar.next_occurrence(*last_payment) {
                Some(next_payment) if next_payment <= Timestamp::now() => {
                    payments_to_update.push(payment.clone());
                }
                _ => {}
            }
        }
    }

    // Process each due payment
    for payment in payments_to_update {
        log::info!("Processing recurring payment: {}", payment.subscription_id);

        let claim_result = claim_payment(
            payment.amount * 1000,
            payment.auth_token.clone(),
            payment.description.clone(),
            Some(payment.subscription_id.clone()),
            payment.user_id.clone(),
            router.clone(),
            nwc.clone(),
        )
        .await;
        match claim_result {
            Ok(_) => {
                let mut lock = recurring_payments.lock().unwrap();
                if let Some(payment_info) = lock.get_mut(&payment.subscription_id) {
                    payment_info.1 = Timestamp::now();
                }
                log::info!(
                    "Recurring payment processed successfully: {}",
                    payment.subscription_id
                );
            }
            Err(e) => {
                log::error!(
                    "Failed to process recurring payment: {}: {}",
                    payment.subscription_id,
                    e
                );
            }
        }
    }
}

#[get("/debug-login")]
fn debug_login(
    cookies: &CookieJar<'_>,
    sessions: &State<Sessions>,
    users: &State<Users>,
) -> Redirect {
    // Create a debug user and session
    let user_id = "debug-user-npub1abc123xyz".to_string();
    let user = User {
        id: user_id.clone(),
        name: "Debug User".to_string(),
    };

    let session_id = Uuid::new_v4().to_string();
    let session = Session {
        user_id: user_id.clone(),
        auth_token: "debug-auth-token".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
    };

    // Save user and session
    users.lock().unwrap().insert(user_id, user);
    sessions.lock().unwrap().insert(session_id.clone(), session);

    // Set session cookie
    cookies.add(Cookie::new("session_id", session_id));

    // Redirect to dashboard
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

    let nwc_str = std::env::var("NWC_URL").expect("NWC_URL is not set");
    let nwc = Arc::new(nwc::NWC::new(nwc_str.parse().unwrap()));

    // Create recurring payments map
    let recurring_payments = RecurringPayments::new(Mutex::new(HashMap::new()));
    let _recurring_payments = Arc::clone(&recurring_payments);
    let _router_for_payments = Arc::clone(&router);
    let _nwc = Arc::clone(&nwc);

    // Spawn task to process recurring payments every minute
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            process_recurring_payments(
                _recurring_payments.clone(),
                _router_for_payments.clone(),
                _nwc.clone(),
            )
            .await;
        }
    });

    rocket::build()
        .attach(Template::fairing())
        .attach(AdHoc::on_ignite("Mount Routes", |rocket| async {
            rocket
                .mount(
                    "/",
                    routes![index, logout, request_payment, debug_login, payment_status],
                )
                .manage(Sessions::new(HashMap::new()))
                .manage(Users::new(HashMap::new()))
                .manage(LoginTokens::new(Mutex::new(HashMap::new())))
                .manage(PaymentRequests::new(Mutex::new(HashMap::new())))
                .manage(recurring_payments)
                .manage(router)
                .manage(nwc)
        }))
}
