import express from 'express';
import { WebSocketServer, WebSocket } from 'ws';
import path from 'path';
import { v4 as uuidv4 } from 'uuid';
import { Currency, PaymentStatusContent, PortalSDK, Profile, RecurringPaymentStatusContent, Timestamp } from 'portal-sdk';

interface Session {
  id: string;
  publicKey: string;
  displayName: string;
  authToken: string;
}

interface LoginStatus {
  type: 'waiting' | 'sending_challenge' | 'approved' | 'timeout';
  displayName?: string;
  publicKey?: string;
  authToken?: string;
}

interface PaymentRequest {
  amount: number;
  description: string;
  payment_type: 'single' | 'recurring';
  frequency: 'minutely' | 'hourly' | 'daily' | 'weekly' | 'monthly' | 'quarterly' | 'semiannually' | 'yearly';
}

const app = express();
const port = process.env.PORT || 8000;
const sessions = new Map<string, Session>();
const loginTokens = new Map<string, LoginStatus>();

const portalClient = new PortalSDK({
  serverUrl: 'ws://127.0.0.1:3000/ws',
  connectTimeout: 5000
});
portalClient.connect()
    .then(() => {
        console.log('Connection established')
        mainFunction();
    })
    .catch(error => {
        console.error('Error connecting to server:', error);
        process.exit(1);
    });

function mainFunction() {
  const authToken = process.env.AUTH_TOKEN || 'your-auth-token'; // Replace with your actual token
  portalClient.authenticate(authToken)
      .catch(error => {
          console.error('Error authenticating:', error);
          process.exit(1);
      });
  
  // Serve static files from the public directory
  app.use(express.static(path.join(__dirname, '../public')));
  
  app.get('/logout', (req, res) => {
    const sessionId = req.cookies?.session_id;
    if (sessionId) {
      sessions.delete(sessionId);
      res.clearCookie('session_id');
    }
    res.redirect('/');
  });
  
  // Create HTTP server
  const server = app.listen(port, () => {
    console.log(`Server running at http://localhost:${port}`);
  });
  
  // Initialize WebSocket server
  const wss = new WebSocketServer({ server });
  
  wss.on('connection', async (ws: WebSocket, req) => {
    console.log('New WebSocket connection');
  
    const mainKey = req.headers.cookie?.split(';')
      .find(c => c.trim().startsWith('main_key='))
      ?.split('=')[1];
    const sessionId = req.headers.cookie?.split(';')
        .find(c => c.trim().startsWith('session_id='))
        ?.split('=')[1];
  
    const isDashboard = req.url?.includes('dashboard');
    if (isDashboard) {
      // Dashboard connection
      if (!sessionId || !sessions.has(sessionId)) {
          console.log('No session found');
          ws.send(`<span id="redirect" data-url="/"></span>`);
      } else {
        const session = sessions.get(sessionId)!;
        
        // Send user info
        ws.send(`<span id="user-name">${session.displayName}</span>`);
  
        // Handle payment requests
        ws.on('message', async (message: Buffer) => {
          try {
            const data = JSON.parse(message.toString());
            console.log('Received payment request:', data);

            const payment = data as PaymentRequest;
            payment.amount = parseInt(data.amount);

            if (payment.payment_type === 'single') {
                const payReq = {
                    amount: payment.amount * 1000,
                    description: payment.description,
                    currency: Currency.Millisats,
                    auth_token: session.authToken,
                    subscription_id: undefined,
                };
                const paymentResult = await portalClient.requestSinglePayment(
                    session.publicKey,
                    [],
                    payReq,
                    (status) => {
                        console.log('Payment status update:', status);
                        ws.send(makePaymentStatus(status));
                    }
                );

                console.log('Single payment result:', paymentResult);
                ws.send(makePaymentStatus(paymentResult));
            } else {
                const payReq = {
                    amount: payment.amount * 1000,
                    currency: Currency.Millisats,
                    auth_token: session.authToken,
                    recurrence: {
                        calendar: payment.frequency,
                        first_payment_due: Timestamp.fromNow(0),
                    },
                    expires_at: Timestamp.fromNow(3600),
                    current_exchange_rate: undefined,
                };

                const paymentResult = await portalClient.requestRecurringPayment(
                    session.publicKey,
                    [],
                    payReq,
                );
                console.log('Recurring payment result:', paymentResult);
                ws.send(makeRecurringPaymentStatus(paymentResult));
            }
          } catch (err) {
            console.error('Error processing message:', err);
          }
        });
      }
    } else if (mainKey) {
      console.log(`User has main key: ${mainKey}`);

      if (sessionId && sessions.has(sessionId)) {
          console.log(`Resuming session ${sessionId}`);
          ws.send(`
              <div id="status" class="status approved" x-session-id="${sessionId}" x-main-key="${mainKey}">
                Login approved! Redirecting...
              </div>
          `);
      } else {
          console.log('Resuming session for user with main key');

          ws.send(`
                <div id="qr-overlay" class="show">Loading...</div>
                <div id="login-button-section">
                  <a href="#" class="login-button disabled" id="portal-login">Login with Portal</a>
                </div>
            `);

          const profile = await portalClient.fetchProfile(mainKey);

          ws.on('message', (message: Buffer) => {
            const data = JSON.parse(message.toString());
            if (data.HEADERS['HX-Trigger'] === 'portal-login') {
                ws.send(`
                    <div id="login-button-section">
                      <a href="#" class="login-button disabled" id="portal-login">Login with Portal</a>
                    </div>
                    <div class="local-login" id="forget-user-section">
                    </div>
                    `);
                authenticateKey(portalClient, ws, mainKey, profile, mainKey);
            }
          });
  
          ws.send(`
              <div id="qr-section">
                <h3>Hello ${profile?.name || mainKey}!</h3>
              </div>
              <div class="local-login" id="login-button-section">
                <a href="#" class="login-button" id="portal-login" ws-send='{"cmd": "login"}'>Login with Portal</a>
              </div>
              <div class="local-login" id="forget-user-section">
                <a href="#" class="login-button forget-user-button" id="forget-user" onclick="window.resetMainKey()">Not you?</a>
              </div>
              <div id="status" class="status sending">
                Welcome back, ${profile?.name || mainKey}!
              </div>
          `);
      }
    } else {
      const loginUrl = await portalClient.newAuthInitUrl((mainKey) => {
          console.log('Auth Init received for key:', mainKey);
          let profile: Profile | null = null;
  
          portalClient.fetchProfile(mainKey)
              .then(p => {
                  profile = p;
                  console.log('Profile:', profile);
  
                  const status = loginTokens.get(loginUrl);
                  if (status && status.type === 'waiting') {
                    loginTokens.set(loginUrl, {
                      type: 'sending_challenge',
                      displayName: profile?.name || mainKey,
                    });
                  }
                
                  ws.send(`
                    <div id="status" class="status sending">
                      Welcome back, ${profile?.name || mainKey}!
                    </div>
                    <div id="qr-overlay" class="show">Loading...</div>
                    <div id="login-button-section">
                      <a href="#" class="login-button disabled" id="portal-login">Login with Portal</a>
                    </div>
                  `);
  
                  return authenticateKey(portalClient, ws, loginUrl, profile, mainKey);
              })
             .catch(error => {
                  console.error('Error authenticating:', error);
              });
      });
      loginTokens.set(loginUrl, { type: 'waiting' });
      
      // Send HTML updates
      ws.send(`
        <div id="login-button-section">
          <a href="${loginUrl}" class="login-button" id="portal-login">Login with Portal</a>
        </div>
        <canvas id="qrcode" data-url="${loginUrl}"></canvas>
      `);
    }
  
    ws.on('close', () => {
      console.log('Client disconnected');
    });
  }); 
}

async function authenticateKey(portalClient: PortalSDK, ws: WebSocket, loginUrl: string, profile: Profile | null, mainKey: string) {
    const authResponse = await portalClient.authenticateKey(mainKey);

    const sessionId = uuidv4();
    
    loginTokens.set(loginUrl, {
      type: 'approved',
      displayName: profile?.name || mainKey,
      publicKey: mainKey,
      authToken: authResponse.session_token,
    });
    
    // Create session
    sessions.set(sessionId, {
      id: sessionId,
      publicKey: mainKey,
      displayName: profile?.name || mainKey,
      authToken: authResponse.session_token,
    });
    
    ws.send(`
        <div id="status" class="status approved" x-session-id="${sessionId}" x-main-key="${mainKey}">
          Login approved! Redirecting...
        </div>
      `);
}

function makePaymentStatus(paymentResult: PaymentStatusContent) {
    return `
            <div id="payment-status">
              <h3>Payment Status: ${paymentResult.status}</h3>
            </div>
          `;
}

function makeRecurringPaymentStatus(paymentResult: RecurringPaymentStatusContent) {
    return `
            <div id="payment-status">
              <h3>Subscription Status</h3>
              <pre>${JSON.stringify(paymentResult, null, 2)}</pre>
            </div>
          `;
}