//! Job orders (DM-02): a customer's order for a design kept with its quantity, due date, garment,
//! placement, notes and status; its worksheet printed with the job's number as the barcode and
//! its machine files written for it in one step.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

/// Jobs kept, at most.
const MAX_JOBS: usize = 2000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.jobs",
            "Job Orders",
            [],
            None,
            "{status?: open|production|done|cancelled, query?: text in the number (customer, design or garment too)} the job orders kept, newest first → {jobs: [{id, number, customer, design, quantity, due, garment, placement, notes, status, created}]}",
            always,
            list
        ),
        cmd!(
            "embroidery.job.create",
            "New Job Order",
            [],
            None,
            "{customer, design?: the design's file (the active document's), quantity?: pieces (1), due?: a date as text (YYYY-MM-DD), garment?, placement?: left-chest|right-chest|full-front|full-back|cap-front|sleeve|yoke|pocket|text, notes?} a job order, numbered J-0001 on → {id, number}",
            always,
            create
        ),
        cmd!(
            "embroidery.job.update",
            "Update Job Order",
            [],
            None,
            "{id, status?: open|production|done|cancelled, customer?, design?, quantity?, due?, garment?, placement?, notes?} change a job order → {id, number, status}",
            always,
            update
        ),
        cmd!("embroidery.job.delete", "Delete Job Order", [], None, "{id} forget a job order → {id}", always, delete),
        cmd!(
            "embroidery.job.worksheet",
            "Job Worksheet",
            [],
            None,
            "{id, kind?: production|approval (production)} the job's worksheet: its design opened (when it isn't the active one), and a worksheet of it with the customer, the job's number as the barcode, the quantity, due date, garment, placement, notes and the cost for the quantity → {number, kind}",
            always,
            worksheet
        ),
        cmd!(
            "embroidery.job.output",
            "Job Output",
            [],
            None,
            "{id, dir?, profile?, formats?, worksheet?, design?} the job's files in one step (embroidery.output), named after the job's number and design, its design opened first when it isn't the active one → embroidery.output's result",
            always,
            output
        ),
    ]
}

/// A job's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    #[default]
    Open,
    Production,
    Done,
    Cancelled,
}

impl JobStatus {
    pub fn id(self) -> &'static str {
        match self {
            JobStatus::Open => "open",
            JobStatus::Production => "production",
            JobStatus::Done => "done",
            JobStatus::Cancelled => "cancelled",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "open" => JobStatus::Open,
            "production" => JobStatus::Production,
            "done" => JobStatus::Done,
            "cancelled" => JobStatus::Cancelled,
            _ => return None,
        })
    }
}

/// A customer's order for a design.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Job {
    pub id: u64,
    pub number: String,
    pub customer: String,
    /// The design's file.
    pub design: String,
    pub quantity: u32,
    pub due: String,
    pub garment: String,
    pub placement: String,
    pub notes: String,
    pub status: JobStatus,
    /// When it was made (Unix seconds).
    pub created: Option<i64>,
}

impl Job {
    fn json(&self) -> Value {
        json!({
            "id": self.id, "number": self.number, "customer": self.customer, "design": self.design,
            "quantity": self.quantity, "due": self.due, "garment": self.garment, "placement": self.placement,
            "notes": self.notes, "status": self.status.id(), "created": self.created,
        })
    }

    /// The job's fields given in `p`, checked.
    fn apply(&mut self, p: &Value, cmd: &str) -> Result<()> {
        let text = |k: &str, max: usize| p.get(k).and_then(Value::as_str).map(|t| t.trim().chars().take(max).collect::<String>());
        if let Some(t) = text("customer", 200) {
            self.customer = t;
        }
        if let Some(t) = text("design", 2000) {
            self.design = t;
        }
        if let Some(t) = text("garment", 200) {
            self.garment = t;
        }
        if let Some(t) = text("placement", 200) {
            self.placement = t;
        }
        if let Some(t) = text("notes", 4000) {
            self.notes = t;
        }
        if let Some(t) = text("due", 40) {
            self.due = t;
        }
        if let Some(v) = p.get("quantity").filter(|v| !v.is_null()) {
            self.quantity = v.as_u64().filter(|q| (1..=1_000_000).contains(q)).ok_or_else(|| bad(cmd, "`quantity` is 1 to 1000000"))? as u32;
        }
        if let Some(v) = p.get("status").filter(|v| !v.is_null()) {
            self.status = v.as_str().and_then(JobStatus::from_id).ok_or_else(|| bad(cmd, "`status` is open, production, done or cancelled"))?;
        }
        Ok(())
    }
}

fn id_param(p: &Value, cmd: &str) -> Result<u64> {
    p.get("id").and_then(Value::as_u64).ok_or_else(|| bad(cmd, "missing `id`: a job's id"))
}

fn find(s: &Session, id: u64, cmd: &str) -> Result<Job> {
    s.prefs.embroidery_jobs.iter().find(|j| j.id == id).cloned().ok_or_else(|| bad(cmd, format!("no job order {id}")))
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.jobs";
    let status = match str_param(p, "status") {
        None => None,
        Some(id) => Some(JobStatus::from_id(id).ok_or_else(|| bad(CMD, "`status` is open, production, done or cancelled"))?),
    };
    let query = str_param(p, "query").map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    let jobs: Vec<Value> = s
        .prefs
        .embroidery_jobs
        .iter()
        .rev()
        .filter(|j| status.is_none_or(|st| j.status == st))
        .filter(|j| {
            query.as_ref().is_none_or(|q| [&j.number, &j.customer, &j.design, &j.garment].iter().any(|f| f.to_lowercase().contains(q.as_str())))
        })
        .map(Job::json)
        .collect();
    Ok(json!({ "jobs": jobs }))
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.create";
    if str_param(p, "customer").map(str::trim).is_none_or(str::is_empty) {
        return Err(bad(CMD, "missing `customer`"));
    }
    if s.prefs.embroidery_jobs.len() >= MAX_JOBS {
        return Err(bad(CMD, format!("{MAX_JOBS} job orders at most: delete finished ones first")));
    }
    let id = s.prefs.embroidery_jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let mut job = Job { id, number: format!("J-{id:04}"), quantity: 1, created: vectorcraft_doc::metadata::now_unix(), ..Job::default() };
    // the active design's file unless another is named
    job.design = s.active().and_then(|d| d.path.clone()).unwrap_or_default();
    job.apply(p, CMD)?;
    let out = json!({ "id": job.id, "number": job.number });
    s.prefs.embroidery_jobs.push(job);
    Ok(out)
}

fn update(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.update";
    let id = id_param(p, CMD)?;
    let mut job = find(s, id, CMD)?;
    job.apply(p, CMD)?;
    let out = json!({ "id": job.id, "number": job.number, "status": job.status.id() });
    if let Some(j) = s.prefs.embroidery_jobs.iter_mut().find(|j| j.id == id) {
        *j = job;
    }
    Ok(out)
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.delete";
    let id = id_param(p, CMD)?;
    let before = s.prefs.embroidery_jobs.len();
    s.prefs.embroidery_jobs.retain(|j| j.id != id);
    if s.prefs.embroidery_jobs.len() == before {
        return Err(bad(CMD, format!("no job order {id}")));
    }
    Ok(json!({ "id": id }))
}

/// The job's design made the active document: opened from its file unless it is open already.
fn open_design(s: &mut Session, job: &Job, cmd: &str) -> Result<()> {
    if job.design.is_empty() {
        return if s.active().is_some() { Ok(()) } else { Err(bad(cmd, "the job has no design: open it, or give the job its `design`")) };
    }
    if s.active().and_then(|d| d.path.as_deref()) == Some(job.design.as_str()) {
        return Ok(());
    }
    if let Some(i) = s.documents().iter().position(|d| d.path.as_deref() == Some(job.design.as_str())) {
        s.set_active(i);
        return Ok(());
    }
    s.execute("file.open", &json!({ "path": job.design }))?;
    Ok(())
}

fn worksheet(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.worksheet";
    let job = find(s, id_param(p, CMD)?, CMD)?;
    let kind = str_param(p, "kind").unwrap_or("production");
    if !["production", "approval"].contains(&kind) {
        return Err(bad(CMD, "`kind` is production or approval"));
    }
    open_design(s, &job, CMD)?;
    let mut notes = vec![format!("Job {}   {} piece{}", job.number, job.quantity, if job.quantity == 1 { "" } else { "s" })];
    if !job.due.is_empty() {
        notes.push(format!("Due {}", job.due));
    }
    if !job.garment.is_empty() {
        notes.push(format!("Garment: {}", job.garment));
    }
    if let Ok(c) = s.execute("embroidery.cost", &json!({ "quantity": job.quantity })) {
        let total = c["total"].as_f64().unwrap_or(0.0);
        notes.push(format!("Cost for {}: {}{total:.2}", job.quantity, c["currency"].as_str().unwrap_or_default()));
    }
    if !job.notes.is_empty() {
        notes.push(job.notes.clone());
    }
    let mut q = json!({ "kind": kind, "customer": job.customer, "notes": notes.join("\n"), "barcode": "code128", "barcodeText": job.number });
    if ["left-chest", "right-chest", "full-front", "full-back", "cap-front", "sleeve", "yoke", "pocket"].contains(&job.placement.as_str()) {
        q["placement"] = json!(job.placement);
    }
    s.execute("embroidery.worksheet", &q)?;
    Ok(json!({ "number": job.number, "kind": kind }))
}

fn output(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.output";
    let job = find(s, id_param(p, CMD)?, CMD)?;
    open_design(s, &job, CMD)?;
    let mut q = p.clone();
    if let Some(o) = q.as_object_mut() {
        o.remove("id");
        let design = super::embroidery_lettering::safe_name(&super::fileio::file_stem(&s.doc()?.doc.title));
        o.entry("name").or_insert(json!(format!("{} {design}", job.number)));
    }
    s.execute("embroidery.output", &q)
}
