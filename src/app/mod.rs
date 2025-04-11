use nostr_relay_pool::RelayPool;

pub struct PortalService {
    relays: RelayPool,
}

impl PortalService {
    pub fn new(relays: RelayPool) -> Self {
        Self { relays }
    }

    pub async fn connect(&self) -> Result<(), Error> {
        self.relays.connect().await;

        // TODO: subscribe to events

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), Error> {
        self.relays.disconnect().await;
        Ok(())
    }
}

pub enum Error {
    ConnectionError,
    DisconnectionError,
    MessageError,
}
