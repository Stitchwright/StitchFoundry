//! StitchFoundry's cloud service (phase A): accounts, AI credits bought through Stripe, and the AI
//! requests the app makes with them, sent to Anthropic with the service's own key. Open source like
//! the app; whoever runs it sets its keys and prices in the environment ([`config`]). The app keeps
//! working without it: every AI feature also takes the user's own Anthropic key.

#![forbid(unsafe_code)]
pub mod api;
pub mod auth;
pub mod config;
pub mod pricing;
pub mod sewout;
pub mod store;
pub mod stripe;
