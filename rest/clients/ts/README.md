# NostrAuth REST Client

TypeScript client for NostrAuth REST API, allowing applications to interact with the NostrAuth server.

## Installation

```bash
npm install nostrauth-rest-client
```

## Building

```bash
npm run build
```

## Usage

### Basic usage

```typescript
import { NostrAuthClient, Currency } from 'nostrauth-rest-client';

const client = new NostrAuthClient({
  serverUrl: 'ws://localhost:3000/ws'
});

async function main() {
  // Connect to the server
  await client.connect();
  
  // Authenticate
  await client.authenticate('your-auth-token');
  
  // Get a new auth init URL
  const { url, streamId } = await client.newAuthInitUrl();
  console.log('Auth URL:', url);
  
  // Listen for auth init events
  const unsubscribe = client.onAuthInit(streamId, (mainKey) => {
    console.log('Auth initiated for key:', mainKey);
  });
  
  // Clean up when done
  unsubscribe();
  client.disconnect();
}

main().catch(console.error);
```

### Payment Handling

The client provides methods to request single and recurring payments, as well as monitor payment status:

```typescript
// Request a single payment
const { status, onStatusChange } = await client.requestSinglePayment(
  mainKey,
  {
    amount: 500,
    currency: Currency.Millisats,
    description: "Test payment"
  },
  subkeys
);

// Monitor payment status
const unsubscribePayment = onStatusChange((updatedStatus) => {
  console.log('Payment status update:', updatedStatus);
  
  if (updatedStatus.status === 'paid') {
    console.log('Payment completed successfully!');
  }
});

// Stop monitoring when done
unsubscribePayment();
```

## Examples

The repository includes example code to demonstrate client usage:

### Payment Example

Shows how to request payments and monitor their status:

```bash
npm run payment-example
```

## License

MIT 