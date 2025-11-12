use std::{sync::Arc, time::Duration as StdDuration};

use app::{CallbackError, CashuRequestListener, RelayStatus, RelayStatusListener, RelayUrl, nwc::{MakeInvoiceRequest, NWC}, rates::{self, MarketAPI}};
use cli::{CliError, create_app_instance, create_sdk_instance};
use portal::protocol::model::{
    Timestamp,
    payment::{CashuRequestContent, CashuRequestContentWithKey, CashuResponseStatus, Currency, ExchangeRate, SinglePaymentRequestContent},
};

struct LogRelayStatusChange;

#[async_trait::async_trait]
impl RelayStatusListener for LogRelayStatusChange {
    async fn on_relay_status_change(
        &self,
        relay_url: RelayUrl,
        status: RelayStatus,
    ) -> Result<(), CallbackError> {
        log::info!("Relay {:?} status changed: {:?}", relay_url.0, status);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), CliError> {
    env_logger::init();

    let relays = vec!["wss://relay.getportal.cc".to_string()];

    let sender_sdk = create_sdk_instance(
        "draft sunny old taxi chimney ski tilt suffer subway bundle once story",
        relays.clone(),
    )
    .await?;

    let market_api = MarketAPI::new()?;
    let market_data = market_api.fetch_market_data("USD").await?;

    let amount = 5.0;
    let currency = "EUR".to_string();

    let nwc = NWC::new("nostr+walletconnect://d2983a182308a757ba8d4285c8c4dad2366069fa74ad22ac9c6ee02e208bf44f?relay=wss://relay.getalby.com/v1&secret=309f565b40da31a32d50362021a54d4c887c06e536ea14675ff43d9f5b46b1e5".to_string(), Arc::new(LogRelayStatusChange))?;
    // dbg!(nwc.get_info().await?);

    let msat_amount = market_data.calculate_millisats(amount);
    dbg!(msat_amount);

    let inv_req = MakeInvoiceRequest {
        amount: msat_amount as u64,
        description: Some("Test payment".to_string()),
        description_hash: None,
        expiry: None,
    };
    let invoice = nwc.make_invoice(inv_req).await?;

    let payment_request = SinglePaymentRequestContent {
        amount: (amount * 100.0) as u64,
        currency: Currency::Fiat(currency),
        description: Some("Test payment".to_string()),
        expires_at: Timestamp::now_plus_seconds(300),
        request_id: invoice.payment_hash.unwrap(),
        current_exchange_rate: Some(ExchangeRate {
            rate: market_data.rate,
            source: "coinbase".to_string(),
            time: Timestamp::now(),
        }),
        invoice: invoice.invoice,
        auth_token: None,
        subscription_id: None,
    };
    let mut event = sender_sdk.request_single_payment("npub1re2rdxc56f4085ed5y56lyyvcjmhfmctaleurz8jv7dugrxult4qxwemkk".parse().unwrap(), vec![], payment_request).await?;
    while let Some(payment_response) = event.next().await {
        dbg!(payment_response);
    }

    Ok(())
}
