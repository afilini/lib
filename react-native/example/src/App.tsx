import { View, StyleSheet, Button } from 'react-native';
import { PortalApp, Keypair, parseAuthInitUrl, initLogger, AuthChallengeEvent, type AuthChallengeListener } from 'portal-app-lib';

let appListener: Promise<void> | null = null;

async function main() {
  initLogger();

  const keypair = new Keypair("nsec1w86jfju9yfpfxtcr6mhqmqrstzdvckkyrthdccdmqhk3xakvt3sqy5ud2k", undefined);
  console.log(keypair.publicKey());
  const url = parseAuthInitUrl("portal://npub1tzas2qztuv0hu86y9d6n04zkt32uadjqkdtgheudecqf7rl9n3escvl445?relays=wss%3A%2F%2Frelay.damus.io,wss%3A%2F%2Frelay.nostr.net&token=fJUx7w4eVaqLVJHyNAfn");
  console.log(url);
  try {
    if (appListener) {
      // TODO: Cancel the previous listener
      appListener = null;
    }

    const app = await PortalApp.create(keypair, ["wss://relay.nostr.net"]);
    appListener = app.listen(); // No await here because we want to listen in the background

    class AuthChallengeListenerImpl implements AuthChallengeListener {
      async onAuthChallenge(event: AuthChallengeEvent): Promise<boolean> {
        console.log("Auth challenge received", event);
        return true;
      }
    }
    app.listenForAuthChallenge(new AuthChallengeListenerImpl());

    await app.sendAuthInit(url);
    console.log("Auth init sent");
  } catch (error) {
    console.log(error);
  }
}

export default function App() {
  return (
    <View style={styles.container}>
      <Button title="Send Auth Init" onPress={() => {
        main()
      }} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
