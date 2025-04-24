use std::{io::Write, str::FromStr, sync::Arc};

use app::{
    AuthChallengeListener, CallbackError, Mnemonic, PaymentRequestListener, PortalApp,
    RecurringPaymentRequest, SinglePaymentRequest, auth::AuthChallengeEvent, generate_mnemonic,
    payments::PaymentRequestEvent,
};
use portal::{
    nostr::nips::{nip19::ToBech32, nip47::PayInvoiceRequest},
    protocol::{
        auth_init::AuthInitUrl,
        model::payment::{PaymentStatusContent, RecurringPaymentStatusContent},
    },
};

struct ApproveLogin;

#[async_trait::async_trait]
impl AuthChallengeListener for ApproveLogin {
    async fn on_auth_challenge(&self, event: AuthChallengeEvent) -> Result<bool, CallbackError> {
        log::info!("Received auth challenge: {:?}", event);
        Ok(true)
    }
}

struct ApprovePayment(Arc<nwc::NWC>);

#[async_trait::async_trait]
impl PaymentRequestListener for ApprovePayment {
    async fn on_single_payment_request(
        &self,
        event: SinglePaymentRequest,
    ) -> Result<PaymentStatusContent, CallbackError> {
        log::info!("Received single payment request: {:?}", event);

        let nwc = self.0.clone();
        tokio::task::spawn(async move {
            let payment_result = nwc
                .pay_invoice(PayInvoiceRequest {
                    id: None,
                    invoice: event.content.invoice,
                    amount: None,
                })
                .await;
            log::info!("Payment result: {:?}", payment_result);
        });

        Ok(PaymentStatusContent::Pending)
    }

    async fn on_recurring_payment_request(
        &self,
        event: RecurringPaymentRequest,
    ) -> Result<RecurringPaymentStatusContent, CallbackError> {
        log::info!("Received recurring payment request: {:?}", event);
        Ok(RecurringPaymentStatusContent::Confirmed {
            subscription_id: "randomid".to_string(),
            authorized_amount: event.content.amount,
            authorized_currency: event.content.currency,
            authorized_recurrence: event.content.recurrence,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // let mnemonic = Mnemonic::new(
    //     "mass derive myself benefit shed true girl orange family spawn device theme",
    // )?;
    let mnemonic = generate_mnemonic()?;
    let keypair = mnemonic.get_keypair()?;

    let nwc_str = std::env::var("CLI_NWC_URL").expect("NWC_URL is not set");
    let nwc = nwc::NWC::new(nwc_str.parse()?);

    log::info!(
        "Public key: {:?}",
        keypair.public_key().to_bech32().unwrap()
    );

    let app = PortalApp::new(
        Arc::new(keypair),
        vec![
            "wss://relay.nostr.net".to_string(),
            // "wss://relay.damus.io".to_string(),
        ],
    )
    .await?;
    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        _app.listen().await.unwrap();
    });

    print!("Enter the auth init URL: ");
    std::io::stdout().flush()?;

    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        _app.listen_for_auth_challenge(Arc::new(ApproveLogin))
            .await
            .unwrap();
    });

    let _app = Arc::clone(&app);
    tokio::spawn(async move {
        _app.listen_for_payment_request(Arc::new(ApprovePayment(Arc::new(nwc))))
            .await
            .unwrap();
    });

    let mut auth_init_url = String::new();
    std::io::stdin().read_line(&mut auth_init_url)?;
    let url = AuthInitUrl::from_str(auth_init_url.trim())?;
    app.send_auth_init(url).await?;

    tokio::time::sleep(std::time::Duration::from_secs(600)).await;

    Ok(())
}
