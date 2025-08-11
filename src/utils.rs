use lightning_invoice::Bolt11Invoice;
use nostr::{key::PublicKey, nips::nip05::Nip05Address};
use rand::Rng;
use secp256k1::PublicKey as PubKey;
use std::{str::FromStr, time::UNIX_EPOCH};

use crate::protocol::model::Timestamp;

pub fn random_string(lenght: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(lenght)
        .map(char::from)
        .collect()
}

pub async fn verify_nip05(nip05: &str, main_key: &PublicKey) -> bool {
    let address = match Nip05Address::parse(nip05) {
        Ok(address) => address,
        Err(_) => return false,
    };

    let url = address.url();
    let req = match reqwest::get(url.to_string()).await {
        Ok(req) => req,
        Err(_) => return false,
    };
    let nip05: serde_json::Value = match req.json().await {
        Ok(nip05) => nip05,
        Err(_) => return false,
    };

    nostr::nips::nip05::verify_from_json(&main_key, &address, &nip05)
}

#[derive(Debug, Clone)]
pub struct Bolt11InvoiceData {
    pub invoice_string: String,
    pub amount_msat: Option<u64>,
    pub description: Option<String>,
    pub description_hash: Option<String>,
    pub payment_hash: String,
    pub timestamp: Timestamp,
    pub expiry: Timestamp,
    pub payee_pubkey: PubKey,
    pub recovery_pubkey: PubKey,
    pub payment_secret: [u8; 32],
    pub features: Option<Vec<u8>>,
    pub route_hints: Vec<RouteHint>,
    pub fallbacks: Vec<FallbackAddress>,
    pub additional_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RouteHint {
    pub pubkey: String,
    pub short_channel_id: u64,
    pub fee_base_msat: u32,
    pub fee_proportional_millionths: u32,
    pub cltv_expiry_delta: u16,
}

#[derive(Debug, Clone)]
pub struct FallbackAddress {
    pub fallback_type: String,
    pub address: String,
}

pub fn parse_bolt11(invoice: &str) -> Result<Bolt11InvoiceData, Box<dyn std::error::Error>> {
    let bolt11_invoice = Bolt11Invoice::from_str(invoice)?;

    let amount_msat = bolt11_invoice.amount_milli_satoshis();
    let (description, description_hash) = match bolt11_invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            (Some(desc.to_string()), None)
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(hash) => {
            (None, Some(hex::encode(hash.0)))
        }
    };
    let payment_hash = hex::encode(bolt11_invoice.payment_hash());

    let timestamp_as_u64 = bolt11_invoice
        .timestamp()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let timestamp = Timestamp::new(timestamp_as_u64);
    let expiry = Timestamp::new(timestamp_as_u64 + bolt11_invoice.expiry_time().as_secs());

    let payee_pubkey = bolt11_invoice.get_payee_pub_key();
    let recovery_pubkey = bolt11_invoice.recover_payee_pub_key();

    let payment_secret = bolt11_invoice.payment_secret().0;

    let features = bolt11_invoice.features().map(|f| f.le_flags().to_vec());

    let route_hints = bolt11_invoice
        .route_hints()
        .iter()
        .flat_map(|hints| hints.0.iter())
        .map(|hint| RouteHint {
            pubkey: hint.src_node_id.to_string(),
            short_channel_id: hint.short_channel_id,
            fee_base_msat: hint.fees.base_msat,
            fee_proportional_millionths: hint.fees.proportional_millionths,
            cltv_expiry_delta: hint.cltv_expiry_delta,
        })
        .collect();

    let fallbacks = bolt11_invoice
        .fallbacks()
        .iter()
        .map(|fallback| match fallback {
            lightning_invoice::Fallback::SegWitProgram { version, program } => FallbackAddress {
                fallback_type: format!("SegWitProgram_v{}", version),
                address: hex::encode(program),
            },
            lightning_invoice::Fallback::PubKeyHash(pubkey_hash) => FallbackAddress {
                fallback_type: "PubKeyHash".to_string(),
                address: hex::encode(pubkey_hash),
            },
            lightning_invoice::Fallback::ScriptHash(script_hash) => FallbackAddress {
                fallback_type: "ScriptHash".to_string(),
                address: hex::encode(script_hash),
            },
        })
        .collect();

    let additional_tags = bolt11_invoice
        .tagged_fields()
        .map(|tagged_field| tagged_field.tag().to_string())
        .collect();

    Ok(Bolt11InvoiceData {
        invoice_string: invoice.to_string(),
        amount_msat,
        description,
        description_hash,
        payment_hash,
        timestamp,
        expiry,
        payee_pubkey,
        recovery_pubkey,
        payment_secret,
        features,
        route_hints,
        fallbacks,
        additional_tags,
    })
}
