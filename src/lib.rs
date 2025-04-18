pub mod app;
pub mod protocol;
pub mod router;
pub mod utils;
pub mod sdk;

pub use nostr;
pub use nostr_relay_pool;

#[cfg(feature = "bindings")]
uniffi::setup_scaffolding!();

#[cfg(test)]
mod test_framework;
