//! Thread charts of one's own (BAS-19): read from a CSV file (a supplier's list, a shop's stock),
//! made from the design's threads or given thread by thread, written out as CSV, and used like
//! the built-in charts to pick, match and recolour threads.

use serde_json::{Value, json};
use sf_core::threads::{MAX_CHART_THREADS, Rgb, Thread, ThreadChart};

use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

/// Charts of one's own kept, at most.
const MAX_CHARTS: usize = 50;
/// The largest CSV file read (bytes).
const MAX_CSV: usize = 4 << 20;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.charts",
            "Thread Charts",
            [],
            None,
            "{name?: a chart's threads too} the thread charts, built-in and one's own → {charts: [{name, threads, custom}], threads?: [{color, name, code}]}",
            always,
            list
        ),
        cmd!(
            "embroidery.chart.import",
            "Import Thread Chart",
            [],
            None,
            "{name, path | text: CSV or a GIMP palette} a thread chart of one's own from a GIMP palette (.gpl: red, green, blue and a name a line; a number at the start or end of the name is its code) or from CSV: a header naming the columns (color/colour/hex or r, g, b; code/number; name/description) or rows of a code, a name and a #rrggbb colour; commas, semicolons or tabs; replaces one of that name; a built-in chart's name can't be taken → {name, threads}",
            always,
            import
        ),
        cmd!(
            query "embroidery.chart.export",
            "Export Thread Chart",
            [],
            None,
            "{name, path?} a chart (built-in or one's own) as CSV (code,name,color) → {path} or {text}",
            always,
            export
        ),
        cmd!(
            "embroidery.chart.save",
            "Save Thread Chart",
            [],
            None,
            "{name, threads?: [{color: \"#rrggbb\", name?, code?}] (without: the design's threads, as they sew)} a thread chart of one's own, given thread by thread or made from the design → {name, threads}",
            always,
            save
        ),
        cmd!("embroidery.chart.delete", "Delete Thread Chart", [], None, "{name} forget a chart of one's own → {name}", always, delete),
    ]
}

/// The built-in charts, then one's own.
pub fn all_charts(s: &Session) -> Vec<ThreadChart> {
    let mut v = sf_core::threads::charts();
    v.extend(s.prefs.embroidery_thread_charts.iter().cloned());
    v
}

/// The chart named `name`, built-in or one's own.
pub(crate) fn find_chart(s: &Session, name: &str) -> Option<ThreadChart> {
    all_charts(s).into_iter().find(|c| c.name == name)
}

fn chart_name(p: &Value, cmd: &str) -> Result<String> {
    let name: String =
        str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(cmd, "missing `name`"))?.chars().take(80).collect();
    Ok(name)
}

/// Keep `chart` among one's own, replacing one of its name.
fn keep(s: &mut Session, chart: ThreadChart, cmd: &str) -> Result<Value> {
    if sf_core::threads::charts().iter().any(|c| c.name.eq_ignore_ascii_case(&chart.name)) {
        return Err(bad(cmd, format!("`{}` is a built-in chart: give yours another name", chart.name)));
    }
    let (name, n) = (chart.name.clone(), chart.threads.len());
    let own = &mut s.prefs.embroidery_thread_charts;
    let full = own.len() >= MAX_CHARTS;
    match own.iter_mut().find(|c| c.name == chart.name) {
        Some(c) => *c = chart,
        None if full => return Err(bad(cmd, format!("{MAX_CHARTS} charts of your own at most: delete one first"))),
        None => own.push(chart),
    }
    Ok(json!({ "name": name, "threads": n }))
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let own = &s.prefs.embroidery_thread_charts;
    let charts: Vec<Value> =
        all_charts(s).iter().map(|c| json!({ "name": c.name, "threads": c.threads.len(), "custom": own.iter().any(|o| o.name == c.name) })).collect();
    let mut out = json!({ "charts": charts });
    if let Some(name) = str_param(p, "name") {
        let c = find_chart(s, name).ok_or_else(|| bad("embroidery.charts", format!("unknown thread chart `{name}`")))?;
        out["threads"] = c.threads.iter().map(|t| json!({ "color": t.color.to_hex(), "name": t.name, "code": t.code })).collect();
    }
    Ok(out)
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chart.import";
    let name = chart_name(p, CMD)?;
    let text = match (str_param(p, "text"), str_param(p, "path")) {
        (Some(t), _) => t.to_string(),
        (None, Some(path)) => {
            let bytes = super::fileio::read_file(path)?;
            if bytes.len() > MAX_CSV {
                return Err(bad(CMD, "the file is larger than 4 MB"));
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }
        (None, None) => return Err(bad(CMD, "give `path` or `text`")),
    };
    if text.len() > MAX_CSV {
        return Err(bad(CMD, "the text is larger than 4 MB"));
    }
    let chart = ThreadChart::from_text(&name, &text).map_err(|e| bad(CMD, e))?;
    keep(s, chart, CMD)
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chart.export";
    let name = chart_name(p, CMD)?;
    let c = find_chart(s, &name).ok_or_else(|| bad(CMD, format!("unknown thread chart `{name}`")))?;
    let csv = c.to_csv();
    match str_param(p, "path").map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => {
            super::fileio::write_file(path, csv.as_bytes())?;
            Ok(json!({ "path": path, "threads": c.threads.len() }))
        }
        None => Ok(json!({ "text": csv, "threads": c.threads.len() })),
    }
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chart.save";
    let name = chart_name(p, CMD)?;
    let threads: Vec<Thread> = match p.get("threads") {
        None | Some(Value::Null) => {
            // the design's threads, as they sew, each once
            let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
            let st = plan.stats.clone().ok_or_else(|| bad(CMD, "nothing is stitched yet: give the threads instead"))?;
            let mut out: Vec<Thread> = Vec::new();
            for c in st.colors {
                if !out.iter().any(|t| t.color == c.thread.color && t.code == c.thread.code) {
                    out.push(Thread { chart: name.clone(), ..c.thread });
                }
            }
            out
        }
        Some(Value::Array(a)) => {
            let mut out = Vec::new();
            for t in a.iter().take(MAX_CHART_THREADS) {
                let color = t["color"].as_str().and_then(Rgb::from_hex).ok_or_else(|| bad(CMD, "each thread's `color` is \"#rrggbb\""))?;
                let text = |k: &str| t[k].as_str().map(|v| v.chars().take(80).collect::<String>()).unwrap_or_default();
                out.push(Thread::with_code(color, text("name"), text("code"), name.as_str()));
            }
            out
        }
        Some(_) => return Err(bad(CMD, "`threads` is a list of {color, name?, code?}")),
    };
    if threads.is_empty() {
        return Err(bad(CMD, "a chart needs a thread at least"));
    }
    keep(s, ThreadChart { name, threads }, CMD)
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chart.delete";
    let name = chart_name(p, CMD)?;
    let own = &mut s.prefs.embroidery_thread_charts;
    let before = own.len();
    own.retain(|c| c.name != name);
    if own.len() == before {
        return Err(bad(CMD, format!("no chart of yours is named `{name}`")));
    }
    Ok(json!({ "name": name }))
}
