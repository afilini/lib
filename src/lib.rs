pub mod app;
pub mod profile;
pub mod protocol;
pub mod router;
pub mod sdk;
pub mod utils;
pub mod close_subscription;

pub use nostr;
pub use nostr_relay_pool;

#[cfg(feature = "bindings")]
uniffi::setup_scaffolding!();

#[cfg(test)]
mod test_framework;
