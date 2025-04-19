import { View, StyleSheet, Button, TextInput } from 'react-native';
import { PortalApp, Keypair, parseAuthInitUrl, initLogger, AuthChallengeEvent, type AuthChallengeListener, Mnemonic, generateMnemonic } from 'portal-app-lib';
import { useState } from 'react';

async function main(authInitUrl: string) {
  initLogger();

  try {
    // Generate a new random mnemonic
    const mnemonic = generateMnemonic();
    // Or load a mnemonic from a string
    // const mnemonic = new Mnemonic("ocean citizen innocent sheriff kit much involve addict machine fine hill zone");

    // Get a nostr keypair object (right now we only support master keys)
    const keypair = mnemonic.getKeypair();
    console.log(keypair.publicKey());

    // Construct the app
    const app = await PortalApp.create(keypair, ["wss://relay.nostr.net"]);
    app.listen(); // No await here because we want to listen in the background

    // Setup the listener for auth challenges (login requests)
    class AuthChallengeListenerImpl implements AuthChallengeListener {
      async onAuthChallenge(event: AuthChallengeEvent): Promise<boolean> {
        console.log("Auth challenge received", event);
        return true;
      }
    }
    app.listenForAuthChallenge(new AuthChallengeListenerImpl());

    // Parse the auth init url
    const url = parseAuthInitUrl(authInitUrl);
    console.log(url);

    // Send the auth init request to the service
    await app.sendAuthInit(url);
    console.log("Auth init sent");
  } catch (error) {
    console.log(error);
  }
}

export default function App() {
  const [authInitUrl, setAuthInitUrl] = useState("");

  return (
    <View style={styles.container}>
      <TextInput placeholder="Enter the auth init url" value={authInitUrl} onChangeText={setAuthInitUrl} />
      <Button title="Send Auth Init" onPress={() => {
        main(authInitUrl);
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
