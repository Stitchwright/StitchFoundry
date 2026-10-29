//! `stitchfoundry-cloud`: serves [`stitchfoundry_cloud::api`] over HTTP on `SF_CLOUD_ADDR`, each
//! request on a thread of its own (AI requests can take minutes). Run it behind a TLS proxy (Caddy,
//! nginx, a cloud load balancer) so the app reaches it over HTTPS. Settings: see `config.rs` and
//! `docs/cloud.md`.

#![forbid(unsafe_code)]
use std::io::Read;
use std::sync::Arc;

use stitchfoundry_cloud::api::{Https, MAX_BODY, Request, Service};
use stitchfoundry_cloud::config::Config;
use stitchfoundry_cloud::store::Store;

fn main() {
    if let Err(e) = run() {
        eprintln!("stitchfoundry-cloud: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cfg = Config::from_env(|k| std::env::var(k).ok())?;
    let store = match &cfg.database_url {
        Some(url) => Store::open_db(url).map_err(|e| e.to_string())?,
        None => Store::open(&cfg.data_dir).map_err(|e| e.to_string())?,
    };
    // after a breach: everyone signs in again (`SF_CLOUD_END_SESSIONS_BEFORE`)
    let mut store = store;
    if let Some(t) = cfg.end_sessions_before {
        let n = store.end_sessions_before(t).map_err(|e| e.to_string())?;
        eprintln!("stitchfoundry-cloud: ended {n} sessions made before SF_CLOUD_END_SESSIONS_BEFORE");
    }
    let addr = cfg.addr.clone();
    eprintln!(
        "stitchfoundry-cloud: listening on {addr}; data in {}; AI {}, payments {}",
        if cfg.database_url.is_some() { "the database" } else { "the data folder" },
        if cfg.anthropic_key.is_some() { "on" } else { "off (no ANTHROPIC_API_KEY)" },
        if cfg.stripe_key.is_some() && cfg.stripe_webhook_secret.is_some() { "on" } else { "off (no STRIPE_SECRET_KEY / STRIPE_WEBHOOK_SECRET)" },
    );
    let service = Arc::new(Service::new(cfg, store, Box::new(Https::default())));
    let server = tiny_http::Server::http(&addr).map_err(|e| format!("can't listen on {addr}: {e}"))?;
    for mut incoming in server.incoming_requests() {
        let service = service.clone();
        let spawned = std::thread::Builder::new().name("request".into()).spawn(move || {
            let mut body = Vec::new();
            let too_big = incoming.body_length().is_some_and(|n| n > MAX_BODY);
            if !too_big {
                // (a body cut short is answered as the bad JSON it is)
                let _ = incoming.as_reader().take(MAX_BODY as u64 + 1).read_to_end(&mut body);
            }
            let req = Request {
                method: incoming.method().as_str().to_uppercase(),
                path: incoming.url().to_string(),
                headers: incoming.headers().iter().map(|h| (h.field.as_str().as_str().to_string(), h.value.as_str().to_string())).collect(),
                body: if too_big { vec![0; MAX_BODY + 1] } else { body },
            };
            let resp = service.handle(&req);
            let mut out = tiny_http::Response::from_string(resp.body).with_status_code(resp.status);
            for (k, v) in std::iter::once(("Content-Type".to_string(), resp.content_type.to_string())).chain(resp.headers) {
                if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                    out.add_header(h);
                }
            }
            // (a client gone away can't be answered)
            let _ = incoming.respond(out);
        });
        if let Err(e) = spawned {
            eprintln!("stitchfoundry-cloud: can't start a thread: {e}");
        }
    }
    Ok(())
}
