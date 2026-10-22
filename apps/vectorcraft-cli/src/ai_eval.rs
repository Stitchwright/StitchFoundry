//! `stitchfoundry-cli ai-eval` (AI-EVAL-01): Design from Prompt's test jobs, run at each effort
//! asked for and put side by side with the owner's reference photos in an HTML report, with what
//! each drawing cost, so a change to the prompt or the effort can be judged on the same jobs.
//!
//! A job is a JSON file (`evals/ai/jobs/*.json`): `{id, title, prompt, sizeMm, colors, hoop:
//! {width, height}, reference?: a photo under the references folder, lookFor: [what a good
//! result shows]}`. References are the owner's own photos and stay on this computer
//! (`evals/ai/references/` is ignored by git). Every run spends money: on the Anthropic account of
//! the key in `ANTHROPIC_API_KEY`, or, without one (or with `--credits`), in credits on the
//! StitchFoundry account the app is signed in to, through the hosted service that holds the key.
//! The estimate is shown and confirmed first.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sf_core::ai::picture::{self, Provider};
use sf_core::ai::{self, ArtRequest, Effort, Usage, cloud};
use vectorcraft_mcp::{Backend, Headless};

/// The most jobs read, and the largest job file.
const MAX_JOBS: usize = 200;
const MAX_JOB_BYTES: u64 = 64 * 1024;
/// How long a drawing may take, answer included: as long as the hosted service waits for it, since
/// a request given up on here is still billed when it finishes.
const TIMEOUT: Duration = Duration::from_secs(600);
/// The hosted service official builds use, when no other is set.
const DEFAULT_SERVER: &str = "https://cloud.stitchfoundry.app";
/// The largest picture read back from a picture service.
const MAX_PICTURE_BYTES: usize = 20 * 1024 * 1024;

pub const USAGE: &str = "  stitchfoundry-cli ai-eval [--jobs DIR] [--references DIR] [--effort quick,normal,thorough|all]
                          [--only ID,…] [--pictures SERVICE[:MODEL],…] [--out DIR]
                          [--credits] [--server URL] [--dry-run] [--yes]
      Run Design from Prompt's test jobs (default evals/ai/jobs) at each effort (default normal)
      with the Anthropic key in ANTHROPIC_API_KEY or, without one (or with --credits), paid in
      credits on the StitchFoundry account the app is signed in to (STITCHFOUNDRY_TOKEN, else the
      app's saved sign-in; the service is --server, else the app's, else
      https://cloud.stitchfoundry.app), and write OUT/report.html (default
      target/ai-eval/<time>): each drawing's stitches beside its reference photo (default
      evals/ai/references), with its tokens, cost, time and what to look for, plus
      results.json. --pictures also makes each job with a picture from each service named
      (google, openai, fal, grok, openrouter; SERVICE:MODEL names its model, such as
      openrouter:google/gemini-3-pro-image), redrawn by Claude as stitches as the app does, with
      that service's key in GOOGLE_API_KEY, OPENAI_API_KEY, FAL_API_KEY, GROK_API_KEY or
      OPENROUTER_API_KEY (billed to that account; the report shows the picture too). Shows the estimated cost and asks before spending anything (--yes doesn't
      ask; --dry-run only shows the plan).
";

/// The environment variable holding `p`'s API key (`GOOGLE_API_KEY`…).
fn picture_key_var(p: Provider) -> String {
    format!("{}_API_KEY", p.id().to_ascii_uppercase())
}

/// What one picture typically costs at `p` (dollars, from its public prices; an estimate, since
/// the services don't say in their replies): shown in the plan only.
fn picture_usd(p: Provider) -> f64 {
    match p {
        Provider::Google => 0.04,
        Provider::OpenAi => 0.17,
        Provider::Fal => 0.01,
        Provider::OpenRouter => 0.17,
        Provider::Grok => 0.05,
    }
}

/// One picture service to compare: the service, the model asked for (empty: its default) and
/// the service's key.
struct Painter {
    provider: Provider,
    model: String,
    key: String,
}

impl Painter {
    /// `google` or `openrouter:google/…`, as in --pictures.
    fn parse(spec: &str) -> Result<(Provider, String), String> {
        let (id, model) = spec.split_once(':').unwrap_or((spec, ""));
        let p = Provider::from_id(id.trim()).ok_or_else(|| format!("--pictures: `{id}` is google, openai, fal, grok or openrouter"))?;
        Ok((p, model.trim().to_string()))
    }

    /// What the report and the plan call it: the service, and the model when one was named.
    fn label(&self) -> String {
        if self.model.is_empty() { self.provider.name().to_string() } else { format!("{} {}", self.provider.name(), self.model) }
    }

    /// A file-name-safe id: `openrouter-google-gemini-3-pro-image`.
    fn slug(&self) -> String {
        let raw = if self.model.is_empty() { self.provider.id().to_string() } else { format!("{}-{}", self.provider.id(), self.model) };
        raw.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
    }
}

/// One test job.
struct Job {
    id: String,
    title: String,
    prompt: String,
    size_mm: f64,
    colors: u32,
    hoop: (f64, f64),
    reference: Option<String>,
    look_for: Vec<String>,
}

impl Job {
    fn read(path: &Path) -> Result<Job, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.len() > MAX_JOB_BYTES {
            return Err(format!("{}: larger than {MAX_JOB_BYTES} bytes", path.display()));
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let id = s("id").filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let (Some(id), Some(prompt)) = (id, s("prompt")) else {
            return Err(format!("{}: a job needs an `id` (letters, digits, -) and a `prompt`", path.display()));
        };
        let num = |p: &str, d: f64| v.pointer(p).and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0).unwrap_or(d);
        Ok(Job {
            title: s("title").unwrap_or_else(|| id.clone()),
            id,
            prompt,
            size_mm: num("/sizeMm", 100.0),
            colors: num("/colors", 6.0).round().clamp(1.0, 12.0) as u32,
            hoop: (num("/hoop/width", 130.0), num("/hoop/height", 180.0)),
            reference: s("reference"),
            look_for: v
                .get("lookFor")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
        })
    }

    fn request(&self, effort: Effort) -> ArtRequest {
        ArtRequest::new(&self.prompt, self.size_mm, self.colors).with_effort(effort)
    }
}

/// Who pays for the drawings: the user's own Anthropic key, or credits on their StitchFoundry
/// account through the hosted service.
enum Payer {
    Key(String),
    Credits { server: String, token: String },
}

impl Payer {
    /// The key or session token the requests are built with.
    fn secret(&self) -> &str {
        match self {
            Payer::Key(k) => k,
            Payer::Credits { token, .. } => token,
        }
    }

    /// Send AI request `http` (built for Anthropic) the way this payer pays → (status, body).
    fn send(&self, http: ai::HttpRequest) -> Result<(u16, String), String> {
        match self {
            Payer::Key(_) => post(&http),
            Payer::Credits { server, .. } => {
                let http = cloud::through_cloud(http, server, cloud::MESSAGES, Vec::new()).map_err(|e| e.to_string())?;
                let (status, body) = post(&http)?;
                // the service's own refusals: sign in again, no credits left, closed
                if matches!(status, 401 | 402 | 403 | 503) { Err(cloud::status_error(status, &body).to_string()) } else { Ok((status, body)) }
            }
        }
    }
}

/// The app's settings folder, where it keeps its sign-in.
fn secrets_dir() -> Option<PathBuf> {
    Some(crate::prefs_dir::prefs_path_for("StitchFoundry", "stitchfoundry")?.parent()?.to_path_buf())
}

/// A secret the app keeps as a file of its own, trimmed; `None` when there is none.
fn saved(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(secrets_dir()?.join(name)).ok()?;
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

fn payer(credits: bool, server: Option<String>) -> Result<Payer, String> {
    let env = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if !credits && let Some(key) = env("ANTHROPIC_API_KEY") {
        return Ok(Payer::Key(key));
    }
    let token = env("STITCHFOUNDRY_TOKEN").or_else(|| saved(cloud::TOKEN_NAME)).ok_or(
        "sign in to your StitchFoundry account in the app (Embroidery › Foundry Account) first, or set ANTHROPIC_API_KEY to an Anthropic key",
    )?;
    let server = server
        .or_else(|| saved("stitchfoundry-server"))
        .or_else(|| env("SF_CLOUD_URL"))
        .or_else(|| option_env!("SF_CLOUD_URL").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_SERVER.to_string());
    let server = cloud::clean_server(&server).ok_or_else(|| format!("`{server}` isn't a service address (https://…)"))?;
    Ok(Payer::Credits { server, token })
}

/// The account's credits left and what a dollar of AI use costs in credits (`None` from a
/// service that doesn't say).
fn account(server: &str, token: &str) -> Result<(u64, Option<f64>), String> {
    let http = cloud::account(server, token).map_err(|e| e.to_string())?;
    let (status, body) = post(&http)?;
    let v = cloud::parse(status, &body).map_err(|e| e.to_string())?;
    Ok((v["credits"].as_u64().unwrap_or(0), v["creditsPerDollar"].as_f64().filter(|r| r.is_finite() && *r > 0.0)))
}

/// What one drawing came to.
struct Outcome {
    job: usize,
    effort: Effort,
    /// Who made the picture Claude redrew (`None`: Claude drew it from the description).
    maker: Option<String>,
    /// The picture as it came, saved next to the report.
    picture: Option<String>,
    seconds: f64,
    usage: Usage,
    model: String,
    /// Whether the check redrew it (Thorough), when it was checked.
    redrawn: Option<bool>,
    /// The stitches image (relative to the report), and the design's numbers.
    image: Option<String>,
    design: Value,
    error: Option<String>,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut jobs_dir = PathBuf::from("evals/ai/jobs");
    let mut refs_dir = PathBuf::from("evals/ai/references");
    let mut efforts = vec![Effort::Normal];
    let mut only: Vec<String> = vec![];
    let mut out: Option<PathBuf> = None;
    let (mut dry, mut yes, mut credits) = (false, false, false);
    let mut server: Option<String> = None;
    let mut pictures: Vec<(Provider, String)> = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |what: &str| it.next().cloned().ok_or_else(|| format!("{a} needs {what}"));
        match a.as_str() {
            "--jobs" => jobs_dir = val("a folder")?.into(),
            "--references" => refs_dir = val("a folder")?.into(),
            "--out" => out = Some(val("a folder")?.into()),
            "--only" => only = val("job ids")?.split(',').map(|s| s.trim().to_string()).collect(),
            "--effort" => {
                let raw = val("quick, normal, thorough or all")?;
                efforts = if raw == "all" {
                    Effort::ALL.to_vec()
                } else {
                    raw.split(',')
                        .map(|e| Effort::from_id(e).ok_or_else(|| format!("--effort: `{e}` is quick, normal or thorough")))
                        .collect::<Result<_, _>>()?
                };
            }
            "--pictures" => {
                pictures =
                    val("picture services")?.split(',').map(str::trim).filter(|p| !p.is_empty()).map(Painter::parse).collect::<Result<_, _>>()?;
            }
            "--dry-run" => dry = true,
            "--credits" => credits = true,
            "--server" => server = Some(val("an address")?),
            "--yes" | "-y" => yes = true,
            other => return Err(format!("unknown ai-eval option `{other}`")),
        }
    }
    let jobs = read_jobs(&jobs_dir, &only)?;
    // the plan and what it should cost
    let mut typical = 0.0;
    let mut most = 0.0;
    // the picture runs need their services' keys, checked before anything is spent
    let mut painters: Vec<Painter> = vec![];
    for (provider, model) in pictures {
        let var = picture_key_var(provider);
        match std::env::var(&var).ok().filter(|k| !k.trim().is_empty()) {
            Some(key) => painters.push(Painter { provider, model, key }),
            None => return Err(format!("--pictures {}: put its API key in {var}", provider.id())),
        }
    }
    eprintln!(
        "{} job(s) × {} effort(s), drawn by Claude{}:",
        jobs.len(),
        efforts.len(),
        painters.iter().map(|p| format!(" and from {}", p.label())).collect::<String>()
    );
    let mut pictures_usd = 0.0;
    for j in &jobs {
        for e in &efforts {
            let est = j.request(*e).estimate();
            typical += est.typical;
            most += est.most;
            eprintln!("  {:<20} {:<9} about ${:.2} (at most ${:.2})", j.id, e.id(), est.typical, est.most);
            for p in &painters {
                // the redraw sends the picture: an image at its most
                let probe = ai::ArtImage { media_type: "image/jpeg", data_base64: String::new() };
                let est = ArtRequest::new(&j.prompt, j.size_mm, j.colors).with_image(probe).with_effort(*e).estimate();
                typical += est.typical;
                most += est.most;
                pictures_usd += picture_usd(p.provider);
                eprintln!(
                    "  {:<20} {:<9} from {}: about ${:.2} (at most ${:.2}) for Claude, about ${:.2} for the picture",
                    j.id,
                    e.id(),
                    p.label(),
                    est.typical,
                    est.most,
                    picture_usd(p.provider),
                );
            }
        }
    }
    if !painters.is_empty() {
        eprintln!("The pictures: about ${pictures_usd:.2} in all, on those services' own accounts.");
    }
    let payer = payer(credits, server);
    match &payer {
        Ok(Payer::Key(_)) => eprintln!("In all about ${typical:.2}, at most ${most:.2}, on the Anthropic account of ANTHROPIC_API_KEY."),
        Err(e) => eprintln!("In all about ${typical:.2} of AI use, at most ${most:.2}. To run it: {e}."),
        Ok(Payer::Credits { server, token }) => match account(server, token) {
            Ok((left, Some(rate))) => {
                let c = |usd: f64| (usd * rate).ceil() as u64;
                eprintln!(
                    "In all about {} credits, at most {}, on your StitchFoundry account through {server}: {left} credits left.",
                    c(typical),
                    c(most)
                );
                if c(most) > left {
                    eprintln!(
                        "That may be more than you have: buy more in the app (Embroidery › Foundry Account), or run fewer jobs (--only, --effort)."
                    );
                }
            }
            Ok((left, None)) => eprintln!(
                "In all about ${typical:.2} of AI use (at most ${most:.2}) before the service's markup, paid in credits through {server}: {left} credits left."
            ),
            Err(e) => eprintln!("The account at {server} couldn't be read: {e}"),
        },
    }
    if dry {
        return Ok(());
    }
    let payer = payer?;
    if !yes && !confirm()? {
        eprintln!("Nothing was sent.");
        return Ok(());
    }
    let out = out.unwrap_or_else(|| PathBuf::from(format!("target/ai-eval/{}", stamp())));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut outcomes = vec![];
    let mut stopped = false;
    for (ji, j) in jobs.iter().enumerate() {
        for e in &efforts {
            eprint!("{} at {}… ", j.id, e.id());
            let _ = std::io::stderr().flush(); // (progress only)
            // the account turned away (signed out, no credits left): the other jobs would be too
            let refused = [ai::ArtError::SignIn, ai::ArtError::NoCredits, ai::ArtError::CloudClosed].map(|e| e.to_string());
            let turned_away = |o: &Outcome| o.error.as_ref().is_some_and(|e| refused.contains(e));
            let o = draw(&payer, ji, j, *e, &out, None);
            match &o.error {
                Some(err) => eprintln!("failed: {err}"),
                None => eprintln!("{:.0} s, {} tokens, ${:.2}", o.seconds, o.usage.total(), o.usage.usd()),
            }
            stopped = turned_away(&o);
            outcomes.push(o);
            for p in &painters {
                if stopped {
                    break;
                }
                eprint!("{} at {} from {}… ", j.id, e.id(), p.label());
                let _ = std::io::stderr().flush(); // (progress only)
                let o = draw(&payer, ji, j, *e, &out, Some(p));
                match &o.error {
                    Some(err) => eprintln!("failed: {err}"),
                    None => eprintln!("{:.0} s, {} tokens, ${:.2} for Claude", o.seconds, o.usage.total(), o.usage.usd()),
                }
                stopped = turned_away(&o);
                outcomes.push(o);
            }
            if stopped {
                eprintln!("Stopped: nothing more was sent.");
                break;
            }
        }
        if stopped {
            break;
        }
    }
    let refs = copy_references(&jobs, &refs_dir, &out);
    write(&out.join("results.json"), &serde_json::to_string_pretty(&results_json(&jobs, &outcomes)).unwrap_or_default())?;
    write(&out.join("report.html"), &report(&jobs, &outcomes, &refs))?;
    eprintln!("Report: {}", out.join("report.html").display());
    Ok(())
}

fn read_jobs(dir: &Path, only: &[String]) -> Result<Vec<Job>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .take(MAX_JOBS)
        .collect();
    paths.sort();
    let jobs: Vec<Job> = paths.iter().map(|p| Job::read(p)).collect::<Result<_, _>>()?;
    let jobs: Vec<Job> = jobs.into_iter().filter(|j| only.is_empty() || only.contains(&j.id)).collect();
    if jobs.is_empty() {
        return Err(format!("no jobs in {}", dir.display()));
    }
    Ok(jobs)
}

fn confirm() -> Result<bool, String> {
    eprint!("Spend it? Type yes: ");
    let _ = std::io::stderr().flush(); // (the prompt only)
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim().eq_ignore_ascii_case("yes"))
}

/// Seconds since 1970 as the run's folder name.
fn stamp() -> String {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()).to_string()
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// A picture from a picture service, as the app sends it on to Claude: decoded within limits,
/// upright, no larger than [`ai::IMAGE_SIDE`], as JPEG; saved as `out/name`.
fn prepare_picture(picture: &ai::ArtImage, out: &Path, name: &str) -> Result<ai::ArtImage, String> {
    let bytes = vectorcraft_format::base64_decode(&picture.data_base64).ok_or("the picture didn't come whole")?;
    if bytes.len() > MAX_PICTURE_BYTES {
        return Err("the picture is too large".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let mut img = reader.decode().map_err(|e| format!("the picture can't be read: {e}"))?;
    let side = ai::IMAGE_SIDE;
    if img.width() > side || img.height() > side {
        img = img.resize(side, side, image::imageops::FilterType::Triangle);
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&img.to_rgb8()).map_err(|e| e.to_string())?;
    write_bytes(&out.join(name), &jpeg)?;
    Ok(ai::ArtImage { media_type: "image/jpeg", data_base64: vectorcraft_format::base64_encode(&jpeg) })
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Send one job at one effort, then digitize the art as the app does and draw its stitches.
/// `maker`: a picture service (and its key) whose picture Claude redraws, as the app does.
fn draw(payer: &Payer, job: usize, j: &Job, effort: Effort, out: &Path, maker: Option<&Painter>) -> Outcome {
    let name = match maker {
        Some(p) => format!("{}-{}-{}", j.id, p.slug(), effort.id()),
        None => format!("{}-{}", j.id, effort.id()),
    };
    let mut o = Outcome {
        job,
        effort,
        maker: maker.map(Painter::label),
        picture: None,
        seconds: 0.0,
        usage: Usage::default(),
        model: String::new(),
        redrawn: None,
        image: None,
        design: Value::Null,
        error: None,
    };
    let started = Instant::now();
    // the request Claude draws from: the description, or the picture with what it shows
    let request = match maker {
        None => Ok(j.request(effort)),
        Some(p) => picture::request(p.provider, &p.key, &p.model, &j.prompt, j.colors)
            .map_err(|e| e.to_string())
            .and_then(|http| post(&http))
            .and_then(|(status, body)| picture::parse_reply(status, &body).map_err(|e| format!("{}: {e}", p.label())))
            .and_then(|pic| prepare_picture(&pic, out, &format!("{name}-picture.jpg")))
            .map(|img| {
                o.picture = Some(format!("{name}-picture.jpg"));
                ArtRequest::new(&format!("The picture shows: {}", j.prompt), j.size_mm, j.colors).with_image(img).with_effort(effort)
            }),
    };
    let request = match request {
        Ok(r) => r,
        Err(e) => {
            o.seconds = started.elapsed().as_secs_f64();
            o.error = Some(e);
            return o;
        }
    };
    let art = request
        .http(payer.secret())
        .map_err(|e| e.to_string())
        .and_then(|http| payer.send(http))
        .and_then(|(status, body)| ai::parse_reply(status, &body).map_err(|e| e.to_string()));
    o.seconds = started.elapsed().as_secs_f64();
    let art = match art {
        Ok(a) => a,
        Err(e) => {
            o.error = Some(e);
            return o;
        }
    };
    // Thorough: the drawing goes back as it stitches, to be checked once
    let art = if effort.checks() {
        let preview = {
            let mut h = crate::with_stores(Headless::new());
            h.call("engine.execute", json!({"command": "embroidery.artPreview", "params": {"svg": art.svg, "size": j.size_mm}}))
        };
        let checked = preview.and_then(|p| {
            let stitched = ai::ArtImage { media_type: "image/png", data_base64: p["dataBase64"].as_str().unwrap_or_default().to_string() };
            let http = request.check_http(payer.secret(), &art, &stitched).map_err(|e| e.to_string())?;
            let (status, body) = payer.send(http)?;
            ai::parse_check(status, &body).map_err(|e| e.to_string())
        });
        o.seconds = started.elapsed().as_secs_f64();
        match checked {
            Ok((ai::Checked::Redrawn(better), u)) => {
                o.redrawn = Some(true);
                let first = art.usage;
                ai::Art { usage: first + u, ..better }
            }
            Ok((ai::Checked::Good, u)) => {
                o.redrawn = Some(false);
                let first = art.usage;
                ai::Art { usage: first + u, ..art }
            }
            Err(e) => {
                eprint!("(check failed: {e}) ");
                art
            }
        }
    } else {
        art
    };
    o.usage = art.usage;
    o.model = art.model.clone();
    let _ = std::fs::write(out.join(format!("{name}.svg")), &art.svg); // (the image below is what the report needs)
    let png = format!("{name}.png");
    let mut h = crate::with_stores(Headless::new());
    h.ensure_document();
    let mut exec = |id: &str, p: Value| h.call("engine.execute", json!({"command": id, "params": p}));
    let done = exec("embroidery.newDesign", json!({"hoop": {"width": j.hoop.0, "height": j.hoop.1}}))
        .and_then(|_| exec("embroidery.placeArt", json!({"svg": art.svg, "size": j.size_mm, "name": j.title})))
        .and_then(|_| exec("select.none", json!({})))
        .and_then(|_| exec("embroidery.info", json!({})))
        .and_then(|info| {
            let path = out.join(&png);
            exec("embroidery.exportImage", json!({"path": path.display().to_string(), "ppmm": 6, "background": "#f4f1ea"})).map(|_| info)
        });
    match done {
        Ok(info) => {
            o.image = Some(png);
            o.design = info;
        }
        Err(e) => o.error = Some(format!("digitizing: {e}")),
    }
    o
}

fn post(req: &ai::HttpRequest) -> Result<(u16, String), String> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(TIMEOUT)).http_status_as_error(false).build().into();
    let mut call = agent.post(&req.url);
    for (name, value) in &req.headers {
        call = call.header(*name, value.as_str());
    }
    let mut res = call.send(req.body.as_str()).map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let body = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// Each job's reference photo copied next to the report (by job), when it is there.
fn copy_references(jobs: &[Job], dir: &Path, out: &Path) -> Vec<Option<String>> {
    jobs.iter()
        .map(|j| {
            // the file name only: a job can't point outside the references folder
            let file = Path::new(j.reference.as_deref()?).file_name()?.to_str()?.to_string();
            let to = format!("reference-{file}");
            std::fs::copy(dir.join(&file), out.join(&to)).ok()?;
            Some(to)
        })
        .collect()
}

fn results_json(jobs: &[Job], outcomes: &[Outcome]) -> Value {
    let rows: Vec<Value> = outcomes
        .iter()
        .map(|o| {
            json!({
                "job": jobs.get(o.job).map(|j| j.id.as_str()),
                "effort": o.effort.id(),
                "picture": o.maker,
                "pictureImage": o.picture,
                "seconds": o.seconds,
                "inputTokens": o.usage.input,
                "outputTokens": o.usage.output,
                "usd": o.usage.usd(),
                "model": o.model,
                "image": o.image,
                "redrawnByCheck": o.redrawn,
                "stitches": o.design.get("stitches"),
                "colorChanges": o.design.get("colorChanges"),
                "error": o.error,
            })
        })
        .collect();
    // per effort: the mean output, to set `Effort::typical_output` from
    let means: Vec<Value> = Effort::ALL
        .iter()
        .filter_map(|e| {
            // Claude's own drawings only: a redrawn picture is another kind of request
            let ok: Vec<&Outcome> = outcomes.iter().filter(|o| o.effort == *e && o.maker.is_none() && o.error.is_none()).collect();
            let n = ok.len() as u64;
            (n > 0).then(|| json!({"effort": e.id(), "drawings": n, "meanOutputTokens": ok.iter().map(|o| o.usage.output).sum::<u64>() / n, "usd": ok.iter().map(|o| o.usage.usd()).sum::<f64>()}))
        })
        .collect();
    json!({ "model": ai::MODEL, "drawings": rows, "byEffort": means })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn report(jobs: &[Job], outcomes: &[Outcome], refs: &[Option<String>]) -> String {
    let mut h = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>AI test jobs</title><style>body{font:15px/1.45 system-ui,sans-serif;margin:24px auto;max-width:1100px;padding:0 16px;color:#222;background:#fff}\
h2{margin-top:2em;border-bottom:1px solid #ddd}.row{display:flex;flex-wrap:wrap;gap:16px}.cell{flex:1 1 300px}\
img{max-width:100%;border:1px solid #ddd;border-radius:6px;background:#f4f1ea}.meta{color:#555;font-size:13px}\
.err{color:#b00}table{border-collapse:collapse}td,th{padding:4px 10px;border-bottom:1px solid #eee;text-align:left}\
@media (prefers-color-scheme:dark){body{background:#1b1b1b;color:#eee}.meta{color:#aaa}h2{border-color:#444}td,th{border-color:#333}}</style>\
<h1>AI test jobs</h1>",
    );
    // the totals by effort
    h.push_str("<table><tr><th>Effort</th><th>Drawings</th><th>Mean output tokens</th><th>Cost</th></tr>");
    for e in Effort::ALL {
        let ok: Vec<&Outcome> = outcomes.iter().filter(|o| o.effort == e && o.maker.is_none() && o.error.is_none()).collect();
        if let Some(n) = std::num::NonZeroU64::new(ok.len() as u64) {
            let mean = ok.iter().map(|o| o.usage.output).sum::<u64>() / n.get();
            let usd: f64 = ok.iter().map(|o| o.usage.usd()).sum();
            h.push_str(&format!("<tr><td>{}</td><td>{n}</td><td>{mean}</td><td>${usd:.2}</td></tr>", e.id()));
        }
    }
    h.push_str("</table>");
    for (ji, j) in jobs.iter().enumerate() {
        h.push_str(&format!("<h2>{}</h2><p>{}</p>", esc(&j.title), esc(&j.prompt)));
        if !j.look_for.is_empty() {
            h.push_str("<p><b>Look for:</b></p><ul>");
            for l in &j.look_for {
                h.push_str(&format!("<li><label><input type=\"checkbox\"> {}</label></li>", esc(l)));
            }
            h.push_str("</ul>");
        }
        h.push_str("<div class=\"row\">");
        match refs.get(ji).and_then(Option::as_ref) {
            Some(r) => h.push_str(&format!(
                "<div class=\"cell\"><img src=\"{}\" alt=\"reference photo\"><div class=\"meta\">Reference photo</div></div>",
                esc(r)
            )),
            None => h.push_str("<div class=\"cell meta\">No reference photo (put it in the references folder).</div>"),
        }
        for o in outcomes.iter().filter(|o| o.job == ji) {
            h.push_str("<div class=\"cell\">");
            let who = match &o.maker {
                Some(p) => format!("{}, from {p}", o.effort.id()),
                None => o.effort.id().to_string(),
            };
            if let Some(pic) = &o.picture {
                h.push_str(&format!("<img src=\"{}\" alt=\"the picture from {}\"><div class=\"meta\">The picture</div>", esc(pic), esc(&who)));
            }
            if let Some(img) = &o.image {
                h.push_str(&format!("<img src=\"{}\" alt=\"stitches at {}\">", esc(img), esc(&who)));
            }
            match &o.error {
                Some(e) => h.push_str(&format!("<div class=\"err\">{}: {}</div>", esc(&who), esc(e))),
                None => h.push_str(&format!(
                    "<div class=\"meta\"><b>{}</b>{}: {:.0} s, {} + {} tokens, ${:.2} for Claude; {} stitches, {} colour changes</div>",
                    esc(&who),
                    match o.redrawn {
                        Some(true) => " (redrawn after its check)",
                        Some(false) => " (passed its check)",
                        None => "",
                    },
                    o.seconds,
                    o.usage.input,
                    o.usage.output,
                    o.usage.usd(),
                    o.design.get("stitches").and_then(Value::as_u64).unwrap_or(0),
                    o.design.get("colorChanges").and_then(Value::as_u64).unwrap_or(0),
                )),
            }
            h.push_str("</div>");
        }
        h.push_str("</div>");
    }
    h.push_str("</html>\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_file(dir: &Path, name: &str, v: Value) {
        std::fs::write(dir.join(name), v.to_string()).unwrap();
    }

    #[test]
    fn jobs_are_read_checked_and_filtered() {
        let dir = std::env::temp_dir().join(format!("sf-ai-eval-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        job_file(
            &dir,
            "a.json",
            json!({"id": "dog", "prompt": "a dog", "sizeMm": 90, "colors": 30, "reference": "../../etc/passwd", "lookFor": ["a tail"]}),
        );
        job_file(&dir, "b.json", json!({"id": "cat", "prompt": "a cat"}));
        std::fs::write(dir.join("notes.txt"), "not a job").unwrap();
        let jobs = read_jobs(&dir, &[]).unwrap();
        assert_eq!(jobs.iter().map(|j| j.id.as_str()).collect::<Vec<_>>(), ["dog", "cat"]);
        assert_eq!((jobs[0].colors, jobs[0].size_mm, jobs[1].hoop), (12, 90.0, (130.0, 180.0)));
        assert_eq!(read_jobs(&dir, &["cat".into()]).unwrap().len(), 1);
        // a reference is looked for by its file name only, inside the references folder
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        assert_eq!(copy_references(&jobs, &dir, &out), vec![None, None]);
        // a bad job is an error, not a crash
        job_file(&dir, "c.json", json!({"id": "../x", "prompt": "p"}));
        assert!(read_jobs(&dir, &[]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_report_escapes_and_totals() {
        let jobs = vec![Job {
            id: "dog".into(),
            title: "A <dog>".into(),
            prompt: "a dog & a ball".into(),
            size_mm: 100.0,
            colors: 6,
            hoop: (130.0, 180.0),
            reference: None,
            look_for: vec!["a tail".into()],
        }];
        let o = |effort, output| Outcome {
            job: 0,
            effort,
            maker: None,
            picture: None,
            seconds: 20.0,
            usage: Usage { input: 1000, output, searches: 0 },
            model: ai::MODEL.into(),
            redrawn: None,
            image: Some("dog.png".into()),
            design: json!({"stitches": 9000, "colorChanges": 4}),
            error: None,
        };
        // a picture from Google redrawn: shown with its picture, left out of the effort means
        let from_google =
            Outcome { maker: Some(Provider::Google.name().into()), picture: Some("dog-google-quick-picture.jpg".into()), ..o(Effort::Quick, 9000) };
        let outcomes = vec![o(Effort::Quick, 3000), o(Effort::Thorough, 12000), from_google];
        let html = report(&jobs, &outcomes, &[None]);
        assert!(html.contains("A &lt;dog&gt;") && html.contains("a dog &amp; a ball") && !html.contains("<dog>"));
        assert!(html.contains("9000 stitches") && html.contains("a tail"));
        let r = results_json(&jobs, &outcomes);
        assert_eq!(r["byEffort"][0]["meanOutputTokens"], 3000);
        assert_eq!(r["drawings"].as_array().unwrap().len(), 3);
        assert_eq!(r["drawings"][2]["picture"], "Google Nano Banana");
        assert!(html.contains("dog-google-quick-picture.jpg") && html.contains("quick, from Google Nano Banana"), "{html}");
        assert_eq!(picture_key_var(Provider::OpenAi), "OPENAI_API_KEY");
        // --pictures: a service, or a service and its model
        assert_eq!(Painter::parse("google").unwrap(), (Provider::Google, String::new()));
        let (p, model) = Painter::parse("openrouter:google/gemini-3-pro-image").unwrap();
        assert_eq!((p, model.as_str()), (Provider::OpenRouter, "google/gemini-3-pro-image"));
        let painter = Painter { provider: p, model, key: "k".into() };
        assert_eq!(painter.slug(), "openrouter-google-gemini-3-pro-image");
        assert!(Painter::parse("midjourney").is_err());
    }
}
