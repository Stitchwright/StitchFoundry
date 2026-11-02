# StitchFoundry cloud service

> **Status:** 2026-10-19. Accounts, credits, Stripe Checkout and AI requests paid in credits
> (CLOUD-01); Foundry Account in the app, with Design from Prompt, Redraw Image, Ask About
> Embroidery and Check My Sew-out paid with credits (CLOUD-02); Check My Sew-out's free monthly
> checks, opt-in sharing and follow-ups (SO-01); a Postgres database and a Railway setup
> (CLOUD-03); a monthly budget for the free offers (CLOUD-04). Hosted on Railway at `https://cloud.stitchfoundry.app`
> since 2026-10-19 (Stripe in test mode); the official builds know it (CLOUD-05); the account's
> lasting `id` in its answers, for MakerFoundry's records (CLOUD-06); password reset by email
> (CLOUD-07). 2026-11-01: credits held for a request aren't held after a restart, a free check's
> slot is taken before its request goes out, paid rounds aren't counted against the free budget,
> and only the betas the apps send are passed on (CLOUD-08).

StitchFoundry's AI features (Design from Prompt, Redraw Image with Claude, Ask About Embroidery,
Check My Sew-out) work with the user's own Anthropic API key. The cloud service is the other way:
people make a Foundry account, buy credits, and the app sends their AI requests through the
service, which pays Anthropic with its own key and takes credits for what each request used. It is
open source like the app (`apps/stitchfoundry-cloud`, MIT OR Apache-2.0), and whoever runs it sets
its keys and prices; nothing secret is in the repository.

## Who pays for what (owner, 2026-10-22)

SiteSentrix earns in three ways, all through the owner's one Stripe account:

| Who | Pays for | Through |
|---|---|---|
| StitchFoundry, LaserFoundry, PatternFoundry and ScreenFoundry users | AI, in Foundry credits | this service: credit packs on Stripe Checkout, `/v1/messages` |
| MakerFoundry buyers | designs that makers sell | MakerFoundry's own Stripe Connect code; MakerFoundry keeps 10% (plus the card fee), the rest goes to the maker |
| StitchBench and StitchShop shops | the hosted option, $25 a month or $250 a year a shop | StitchHub's Stripe subscription (off while `PROGRAM_FREE` is on) |

Not through SiteSentrix at all: a shop's own customers pay at the shop's own point of sale
(StitchBench and StitchShop leave that to the shop), and parts one shop buys from another are paid
between the two shops directly (PayPal, Zelle, a check). StitchHub only keeps the order, the
shipping and its tracking number, and the messages between the shops.

One Foundry Account signs in to all of them and holds one credit balance. Only the credit packs
pass through this service; sales and subscriptions are the other services' own.

## What it does

- **Accounts:** sign up and sign in with an email and a password (in the app: **Embroidery ›
  Foundry Account…**). Passwords are kept only as Argon2id hashes; a sign-in gives the app a
  session token (kept by the service only as its SHA-256), which lasts 90 days unused. Five wrong
  passwords in 15 minutes lock that email out for the rest of the 15 minutes. The same account
  signs in to MakerFoundry (owner's decision, 2026-10-19), which keeps its own records under the
  account's `id`.
- **Forgotten passwords:** `POST /v1/password/forgot` emails a link (through Resend, from
  `SF_CLOUD_MAIL_FROM`) that opens `/reset`, a page with no scripts where the new password is set.
  The link works once, for an hour; the service keeps only its SHA-256. Setting a new password
  signs the account out everywhere. The answer is the same whether or not an account has the
  email, and an address gets at most five reset emails in 15 minutes.
- **Credits:** a new account starts with `SF_CLOUD_SIGNUP_CREDITS` (100). Before an AI request goes
  out, the most it could cost is held (or, when that is more than the account has, all it has, if
  at least 25 credits); when the answer is in, only what it used is taken, never more than was
  held, and a request that fails or is refused costs nothing. Every change is a ledger entry.
  A hold lasts only as long as its request: it is never written, so after a restart (a redeploy
  in the middle of a request) no credits stay held.
- **Buying credits:** the app asks for a checkout page for a pack (`POST /v1/checkout`); the buyer
  pays on Stripe's own page in the browser; Stripe tells the service with a signed webhook, which
  adds the credits once (even if Stripe sends it twice). Card details never reach the service. The
  checkout page and the receipt name them "Foundry credits", since every Foundry app uses them.
- **AI requests:** `POST /v1/messages` takes the same request the app sends Anthropic, checks it
  (the offered model, at most 16,000 tokens a reply, no streaming, at most three at once an
  account; tools only the app's own and the basic web search, `web_search_20250305` with at most
  two searches), and sends it with the service's key. Tools that run rounds of their own on
  Anthropic's side (code, the search that filters with code, fetching pages) are refused: they
  can run up to ten rounds in one request, each reading everything again, and in the AI test jobs
  of 2026-10-22 one drawing so read 928,000 tokens ($4.30), more than any hold. A web search's hold
  counts each search as a round that reads everything again, with 40,000 tokens of results each. The answer comes back as Anthropic gave it, with
  `x-stitchfoundry-credits` (what is left) and `x-stitchfoundry-spent`. Of the `anthropic-beta`
  header, only the betas the apps send are passed on (`server-side-fallback-2026-07-01`); any
  other, which could be billed above what the service charges, is dropped.
- **Check My Sew-out:** `POST /v1/sewout` takes a check's requests (its first message must hold the
  photos) and sends them with the service's own sew-out instructions and tools, whatever the app
  sent, so the free checks can't pay for other requests. Each account gets `SF_CLOUD_FREE_CHECKS`
  (3) checks a calendar month free; after them a check costs `SF_CLOUD_CHECK_CREDITS` (10), once,
  whatever it uses. A check is named by the app (`x-stitchfoundry-check`) and covers 16 requests
  (the answer, the rounds of design changes, follow-up questions) for a day; after that its
  requests are paid as any other. A free check takes its place among the month's free checks
  before its first request goes out (and gives it back if that request fails), so checks started
  at the same time can't all take the last free one; only its requests charged nothing count
  against the free budget, not the paid ones after its day.
- **Shared checks:** with the user's tick ("Share this check to help improve StitchFoundry", sent
  as `x-stitchfoundry-share: 1`), the service keeps the check: its photos, the app's summary of the
  design, Claude's answers and the changes it made, one round at a time. A later check made with
  "This is the sew-out after the fix from my last check" (`x-stitchfoundry-follow-up`) is tied to
  it: whether the fix worked. Shared checks are private to the user and the project, never
  published or committed, and **Delete My Shared Sew-out Checks** in the Account window
  (`POST /v1/sewout/delete`) removes them all.

## Prices

The owner's prices (2026-10-18), the defaults; each is a setting:

- A credit is `SF_CLOUD_CENTS_PER_CREDIT` cents (1), as ArtCraft's credits are.
- A request costs what Anthropic charges for its tokens times `SF_CLOUD_MARKUP` (3), rounded up to
  whole credits. With Claude Opus 5.5 at $4 a million input tokens and $20 a million output
  tokens, an Ask question costs about 5 to 15 credits and a Design from Prompt drawing about 50 to
  100 (50 cents to a dollar, against $20–30 for a digitizer).
- Check My Sew-out: 3 checks a month free, then 10 credits a check (about what one costs).
- The packs on sale, `SF_CLOUD_PACKS` (`id:credits:cents` apart by commas): `small:1000:1000`
  ($10 for 1,000), `medium:2750:2500` ($25 for 2,750), `large:6000:5000` ($50 for 6,000). Nothing
  under $10: card fees take too large a share of smaller payments.
- 100 credits on sign-up.
- The free offers (the sign-up credits and the free checks) may cost at most
  `SF_CLOUD_FREE_BUDGET_CENTS` (2500: $25) of AI a calendar month. Sign-up credits count at the
  most they can buy (100 credits are 34 cents of AI at a markup of 3), free checks at what each of
  their requests used. Once the month's budget is spent, new accounts start with no credits and
  checks are paid at their flat price, until the next month; nothing already given is taken back.

## Hosting on Railway

Railway runs the service from this repository's `packaging/cloud/Dockerfile`, with a Postgres
database beside it.

1. **Make an account and a project** at railway.com (the Hobby plan, $5 a month, is enough to
   start; it includes $5 of use).
2. **Add a database:** in the project, **+ Create › Database › PostgreSQL**.
3. **Add the service:** **+ Create › GitHub Repo**, and choose the StitchFoundry repository. Leave
   the root directory empty (the build needs the whole repository).
4. **Variables** of the service (**Variables** tab):

   | Variable | Value |
   |---|---|
   | `RAILWAY_DOCKERFILE_PATH` | `packaging/cloud/Dockerfile` |
   | `DATABASE_URL` | `${{Postgres.DATABASE_URL}}` (Railway's reference to the database's private address) |
   | `ANTHROPIC_API_KEY` | the service's Anthropic key (from the Anthropic Console, on the business account) |
   | `STRIPE_SECRET_KEY` | Stripe's secret key (`sk_test_…` first, `sk_live_…` when ready) |
   | `STRIPE_WEBHOOK_SECRET` | the webhook's signing secret (step 6) |
   | `SF_CLOUD_PUBLIC_URL` | the service's address (step 5), for Stripe's return pages |

   The prices have defaults (above); set `SF_CLOUD_MARKUP`, `SF_CLOUD_PACKS`,
   `SF_CLOUD_SIGNUP_CREDITS`, `SF_CLOUD_FREE_CHECKS`, `SF_CLOUD_CHECK_CREDITS` or
   `SF_CLOUD_FREE_BUDGET_CENTS` only to change
   them.
5. **An address:** **Settings › Networking › Generate Domain** gives a `….up.railway.app` address
   with HTTPS; or add your own domain there (a `CNAME` at your domain's registrar). Railway gives
   the service its port in `PORT`, which it listens on.
6. **Stripe:** in the Stripe Dashboard, **Developers › Webhooks › Add endpoint** at
   `https://<the address>/v1/stripe/webhook`, for the events `checkout.session.completed` and
   `checkout.session.async_payment_succeeded`; put its signing secret in `STRIPE_WEBHOOK_SECRET`.
   Try it with Stripe's test keys and test cards before the live ones.
7. **Check it:** `https://<the address>/v1/health` answers `{"ok": true, "ai": true, "payments":
   true, "email": true}` when everything is set.
8. **The app:** builds made with `SF_CLOUD_URL=https://<the address>` know the service. The
   official builds (`builds.yml`, `release.yml`, `pages.yml` and the Windows workflows) are made
   with `SF_CLOUD_URL=https://cloud.stitchfoundry.app`, StitchFoundry's own service; builds of
   your own know none until you set it. Anyone can type another address in StitchFoundry
   Account's **Server** row (or set `STITCHFOUNDRY_CLOUD_URL`).

Railway backs up the database (Postgres **Backups**); turn them on. Railway is retiring its
`railway.json` configuration files, which is why the Dockerfile's path is a variable.

## Running it elsewhere

```sh
cargo build --release -p stitchfoundry-cloud
DATABASE_URL=postgres://user:password@db.internal/stitchfoundry \
SF_CLOUD_PUBLIC_URL=https://cloud.example.com \
ANTHROPIC_API_KEY=sk-ant-… STRIPE_SECRET_KEY=sk_live_… STRIPE_WEBHOOK_SECRET=whsec_… \
target/release/stitchfoundry-cloud
```

| Setting | What it is |
|---|---|
| `SF_CLOUD_ADDR` | where it listens (else `0.0.0.0:$PORT` when `PORT` is set, else `127.0.0.1:8080`) |
| `DATABASE_URL` | a Postgres database, reached over a private network (no TLS); without it, the data folder |
| `SF_CLOUD_DATA` | the data folder (`state.json`, `ledger.jsonl`, `shared/`) when there is no database: back it up |
| `SF_CLOUD_PUBLIC_URL` | the address people reach it at, for Stripe's return pages |
| `ANTHROPIC_API_KEY` | the service's Anthropic key; without it AI requests answer "not open yet" |
| `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET` | Stripe's secret key and the webhook's signing secret; without them nothing can be bought |
| `SF_CLOUD_SIGNUP_CREDITS` | credits a new account starts with (100) |
| `SF_CLOUD_CENTS_PER_CREDIT`, `SF_CLOUD_MARKUP` | a credit's price in cents (1) and the markup on the AI's cost (3) |
| `SF_CLOUD_PACKS` | the packs on sale (see Prices) |
| `SF_CLOUD_FREE_CHECKS`, `SF_CLOUD_CHECK_CREDITS` | sew-out checks free a month (3) and a check's price after them (10) |
| `SF_CLOUD_FREE_BUDGET_CENTS` | the most the free offers may cost in AI a month (2500: $25; see Prices) |
| `SF_CLOUD_MAIL_FROM` | who password-reset emails come from (`StitchFoundry <accounts@stitchfoundry.app>`), through Resend (`RESEND_API_KEY`); without the key there's no reset by email |
| `RESEND_API_KEY`, `SF_CLOUD_REPORTS_TO`, `SF_CLOUD_REPORTS_FROM` | sew-out reports emailed through Resend to this address (e.g. `support@stitchfoundry.app`), from an address on a domain Resend has verified (`StitchFoundry <reports@stitchfoundry.app>`); without them reports are only kept |
| `SF_CLOUD_END_SESSIONS_BEFORE` | sessions made before this UTC time (`2026-10-22T09:00:00Z`, a date, or Unix seconds) end when the service starts: everyone signs in again (see After a breach) |

It speaks plain HTTP: put it behind a TLS proxy (Caddy, nginx, or the host's load balancer, as
Railway's is) so the app reaches it over HTTPS. One small machine is enough to start: it keeps
everything in one process with a lock, writing each change to the database. In the database it
keeps `sf_state` (the accounts, sessions and checks, as one record), `sf_ledger` (every credit
added or spent) and `sf_shared_files` and `sf_shared_rounds` (shared checks).

To try the database code against a database of your own: `SF_CLOUD_TEST_DATABASE_URL=postgres://…
cargo test -p stitchfoundry-cloud` (it empties StitchFoundry's tables there).

## After a breach

If the host, a key or the database may have been exposed, give every secret a new value and sign
everyone out. For each key: make a new one, put it in the Railway variable, let the service
redeploy, then revoke the old one, so the service never stops.

1. `ANTHROPIC_API_KEY`: a new key in the Anthropic Console, the old one deleted (and the usage
   page checked for spending you don't know). The repository secret of the same name, which the
   AI test jobs use, if it was the same key.
2. `STRIPE_SECRET_KEY`: rolled in Stripe (Developers › API keys); `STRIPE_WEBHOOK_SECRET`: the
   endpoint's signing secret rolled (Developers › Webhooks).
3. `RESEND_API_KEY`: a new key in Resend, the old one deleted.
4. `DATABASE_URL`: the database's credentials regenerated in Railway.
5. `SF_CLOUD_END_SESSIONS_BEFORE` set to the time of the change (UTC): every session made before
   it ends when the service restarts, so a session token copied from the database stops working
   and everyone signs in again (passwords are kept only as hashes). Later restarts end nothing
   more; remove the variable when you like.
6. Railway itself: a new password, two-factor sign-in, its API tokens revoked, the project's
   members and deploys checked.

## What the owner needs

- An Anthropic account for the service's API key (billed to the owner; credits pay it back).
- A Stripe account (business details, a bank account for payouts).
- A Railway account and project (above), and a domain if you want your own address.
- Terms of service and a privacy policy (what is kept: email, password hash, credit history,
  shared sew-out checks with their photos; what goes to Anthropic: the request itself).

## Endpoints

| | |
|---|---|
| `GET /v1/health` | `{ok, ai, payments, email}`: whether AI requests, payments and emails (Resend) are set up |
| `POST /v1/signup {email, password}` | a new account → `{token, account: {id, email, credits}}` |
| `POST /v1/signin {email, password}` | → `{token, account}` |
| `POST /v1/signout` | ends the session |
| `POST /v1/password/forgot {email}` | emails a reset link if there's an account with that email → `{sent, message}` (the same answer either way; 503 without Resend, 429 after five in 15 minutes) |
| `GET /reset?token=…`, `POST /reset` | the page the link opens, and its form (`token`, `password`, `again`) |
| `POST /v1/password/reset {token, password}` | the same for apps → `{ok}`; 400 when the link expired, was used, or the password is too short |
| `GET` or `POST /v1/account` | → `{id, email, credits, freeChecks, checkCredits, sharedChecks, packs, payments, history}` |
| `POST /v1/checkout {pack}` | → `{url}`: Stripe's checkout page |
| `POST /v1/stripe/webhook` | Stripe's signed events |
| `POST /v1/messages` | an Anthropic Messages request, paid in credits |
| `POST /v1/sewout` | a Check My Sew-out request (`x-stitchfoundry-check`, `-share`, `-follow-up`) → as `/v1/messages`, with `x-stitchfoundry-free-checks` |
| `POST /v1/sewout/delete {check?}` | deletes a shared check, or all of the account's and its reports → `{deleted, reports}` |
| `POST /v1/report {consent: true, title, report, note, photos: [{mediaType, data}], conversation?, design?, app, check?}` | Report to StitchFoundry: kept privately, emailed to the owner when Resend is set up (five an account a day); the credits the reported check cost are given back, once → `{report, emailed, refunded}` |

Requests other than sign-up and sign-in carry `Authorization: Bearer <token>`. Every answer allows
the web app to call from a browser (CORS; sessions are bearer tokens, never cookies). Errors come
in the Anthropic API's shape, `{"type": "error", "error": {"type", "message"}}`: 401 to sign in
again, 402 for too few credits, 429 to wait, 503 for what isn't set up.
