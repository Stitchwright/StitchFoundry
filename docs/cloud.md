# StitchFoundry cloud service

> **Status:** 2026-10-18, first version (CLOUD-01): accounts, credits, Stripe Checkout and AI
> requests paid in credits. The app's sign-in and credits come next (CLOUD-02). Not yet hosted.

StitchFoundry's AI features (Design from Prompt, Redraw Image with Claude, Ask About Embroidery)
work with the user's own Anthropic API key. The cloud service is the other way: people make a
StitchFoundry account, buy credits, and the app sends their AI requests through the service,
which pays Anthropic with its own key and takes credits for what each request used. It is open
source like the app (`apps/stitchfoundry-cloud`, MIT OR Apache-2.0), and whoever runs it sets its
keys and prices; nothing secret is in the repository.

## What it does

- **Accounts:** sign up and sign in with an email and a password. Passwords are kept only as
  Argon2id hashes; a sign-in gives the app a session token (kept by the service only as its
  SHA-256), which lasts 90 days unused. Five wrong passwords in 15 minutes lock that email out
  for the rest of the 15 minutes.
- **Credits:** a new account starts with `SF_CLOUD_SIGNUP_CREDITS` (100). Before an AI request
  goes out, the most it could cost is held; when the answer is in, only what it used is taken,
  and a request that fails or is refused costs nothing. Every change is a line in `ledger.jsonl`.
- **Buying credits:** the app asks for a checkout page for a pack (`POST /v1/checkout`); the
  buyer pays on Stripe's own page; Stripe tells the service with a signed webhook, which adds the
  credits once (even if Stripe sends it twice). Card details never reach the service.
- **AI requests:** `POST /v1/messages` takes the same request the app sends Anthropic, checks it
  (the offered model, at most 16,000 tokens a reply, no streaming, at most three at once an
  account), and sends it with the service's key. The answer comes back as Anthropic gave it, with
  `x-stitchfoundry-credits` (what is left) and `x-stitchfoundry-spent`.

## Prices

A credit is `SF_CLOUD_CENTS_PER_CREDIT` cents (1). A request costs what Anthropic charges for its
tokens times `SF_CLOUD_MARKUP` (1.5), rounded up to whole credits. With Claude Opus 5.5 at $4 a
million input tokens and $20 a million output tokens, an Ask question with a short answer costs
about 2 to 6 credits; a Design from Prompt drawing about 10 to 40.

The packs on sale are `SF_CLOUD_PACKS`, `id:credits:cents` apart by commas. The defaults are
placeholders until the owner sets prices: `small:500:500` ($5 for 500), `medium:1200:1000`
($10 for 1,200), `large:3000:2000` ($20 for 3,000).

## Running it

```sh
cargo build --release -p stitchfoundry-cloud
SF_CLOUD_ADDR=127.0.0.1:8080 \
SF_CLOUD_DATA=/var/lib/stitchfoundry-cloud \
SF_CLOUD_PUBLIC_URL=https://cloud.example.com \
ANTHROPIC_API_KEY=sk-ant-… \
STRIPE_SECRET_KEY=sk_live_… \
STRIPE_WEBHOOK_SECRET=whsec_… \
target/release/stitchfoundry-cloud
```

| Setting | What it is |
|---|---|
| `SF_CLOUD_ADDR` | where it listens (`127.0.0.1:8080`) |
| `SF_CLOUD_DATA` | the folder for `state.json` and `ledger.jsonl`: back it up |
| `SF_CLOUD_PUBLIC_URL` | the address people reach it at, for Stripe's return pages |
| `ANTHROPIC_API_KEY` | the service's Anthropic key; without it AI requests answer "not open yet" |
| `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET` | Stripe's secret key and the webhook's signing secret; without them nothing can be bought |
| `SF_CLOUD_SIGNUP_CREDITS` | credits a new account starts with (100) |
| `SF_CLOUD_CENTS_PER_CREDIT`, `SF_CLOUD_MARKUP` | a credit's price in cents (1) and the markup on the AI's cost (1.5) |
| `SF_CLOUD_PACKS` | the packs on sale (see Prices) |

It speaks plain HTTP: put it behind a TLS proxy (Caddy, nginx, or the cloud host's load
balancer) so the app reaches it over HTTPS. One small virtual machine is enough to start; it
keeps everything in one process with a lock, and moves to a database when it outgrows that.

**Stripe:** in the Stripe Dashboard add a webhook endpoint at
`https://<your address>/v1/stripe/webhook` for the events `checkout.session.completed` and
`checkout.session.async_payment_succeeded`, and put its signing secret in
`STRIPE_WEBHOOK_SECRET`. Test first with Stripe's test keys (`sk_test_…`) and test cards.

## What the owner needs

- An Anthropic account for the service's API key (billed to the owner; credits pay it back).
- A Stripe account (business details, a bank account for payouts).
- A host: a small virtual machine or container service, a domain for it, and TLS.
- The prices: what a credit sells for, the markup, the packs, the starting credits.
- Terms of service and a privacy policy (what is kept: email, password hash, credit history;
  what goes to Anthropic: the request itself).

## Endpoints

| | |
|---|---|
| `GET /v1/health` | `{ok, ai, payments}`: whether AI requests and payments are set up |
| `POST /v1/signup {email, password}` | a new account → `{token, account: {email, credits}}` |
| `POST /v1/signin {email, password}` | → `{token, account}` |
| `POST /v1/signout` | ends the session |
| `GET /v1/account` | → `{email, credits, packs, payments, history}` |
| `POST /v1/checkout {pack}` | → `{url}`: Stripe's checkout page |
| `POST /v1/stripe/webhook` | Stripe's signed events |
| `POST /v1/messages` | an Anthropic Messages request, paid in credits |

Requests other than sign-up and sign-in carry `Authorization: Bearer <token>`. Errors come in the
Anthropic API's shape, `{"type": "error", "error": {"type", "message"}}`: 401 to sign in again,
402 for too few credits, 429 to wait, 503 for what isn't set up.
