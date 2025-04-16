use std::sync::Arc;

use nostr_relay_pool::{RelayOptions, RelayPool};

use crate::{protocol::{auth_init::AuthInitUrl, LocalKeypair}, router::{DelayedReply, MessageRouter}};

use handlers::*;
pub mod handlers;
