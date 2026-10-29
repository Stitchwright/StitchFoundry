//! Longarm quilting (Q1–Q7): continuous-line designs for computerized longarm machines, drawn as
//! ordinary lines (a stroke, no fill) and written as single-stroke PLT, DXF and SVG for the
//! quilting software that runs the machine. The geometry is `sf_core::quilt`'s. The machines' own
//! formats (QLI, HQF, IQP, BQM, CQP, PAT, SSD) have no public description, so they aren't written.

use std::sync::Arc;

use kurbo::BezPath;
use serde_json::{Value, json};
use sf_core::Point as Mm;
use sf_core::quilt::{self, EdgeHeight};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, Node, NodeId};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::PathData;

use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, has_selection, str_param, targets};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "quilt.check",
            "Check Continuous Line",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{ids?, tolerance?: mm (0.5)} whether the selected lines make one continuous line, as a longarm sews it → {pieces, joins: gaps a join would bridge, longestJoin: mm, continuous: bool, lengthMm}",
            has_selection,
            check
        ),
        cmd!(
            "quilt.join",
            "Join into One Line",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{ids?, tolerance?: mm (0.5)} the selected lines and shapes' outlines chained into one continuous line, from the leftmost end, each time on to the nearest end (a closed one sewn round from its nearest point); it takes their place → {id, joins, longestJoin: mm, pieces}",
            has_selection,
            join
        ),
        cmd!(
            "quilt.edges",
            "Start and End on the Edges",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{id?, height?: start|middle (start)} the selected line led in from the left edge of its box and out to the right edge at one height, so copies set side by side run on from one to the next (turned to run left to right first) → {id}",
            has_selection,
            edges
        ),
        cmd!(
            "quilt.pantograph",
            "Edge-to-Edge Rows…",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{id?, width: mm (quilt width, 10 to 20000), height: mm (10 to 20000), row?: mm between rows (the line's height), nest?: bool (true: every other row half a repeat along, so rows fit into one another)} the selected line, run edge to edge, repeated across a quilt in rows, each row one continuous line, from the line's top left → {id: group, rows}",
            has_selection,
            pantograph
        ),
        cmd!(
            "quilt.border",
            "Border or Corner…",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{id?, length: mm (10 to 20000), depth: mm (2 to 1000), down?: mm (a corner: the border turns a quarter and runs this far down)} the selected line fitted a whole number of times along a border, one continuous line → {id, repeats?}",
            has_selection,
            border
        ),
        cmd!(
            "quilt.fill",
            "Continuous Fill…",
            ["Embroidery", "Longarm Quilting"],
            None,
            "{ids?, kind: stipple|meander|echo|ditch, spacing?: mm (6; 2 to 100)} a continuous line inside each selected closed shape (stipple, meander or echo rings), or along all their outlines as one line (ditch: stitch in the ditch of the seams) → {ids}",
            has_selection,
            fill
        ),
        cmd!(
            query "quilt.export",
            "Export Quilting Lines",
            [],
            None,
            "{format: plt|dxf|svg, ids?} the selected lines (else every line in the document) as a single-stroke file for quilting software: HP-GL PLT (40 units a mm), DXF R12 polylines in mm, or SVG polylines in mm → {dataBase64, format, lines}",
            has_doc,
            export
        ),
    ]
}

fn mm_param(p: &Value, key: &str, lo: f64, hi: f64, default: Option<f64>, cmd: &str) -> Result<f64> {
    match (p.get(key), default) {
        (None | Some(Value::Null), Some(d)) => Ok(d),
        (None | Some(Value::Null), None) => Err(bad(cmd, format!("missing `{key}` ({lo} to {hi} mm)"))),
        (Some(v), _) => v.as_f64().filter(|v| (lo..=hi).contains(v)).ok_or_else(|| bad(cmd, format!("`{key}` is {lo} to {hi} mm"))),
    }
}

/// Every line of a node: its paths (and type's outlines) as polylines in mm, closed ones ending
/// where they start.
fn lines_of(d: &Document, id: NodeId) -> Vec<Vec<Mm>> {
    let Some(n) = d.node(id) else { return Vec::new() };
    if let Some(children) = n.children() {
        return children.iter().flat_map(|c| lines_of(d, c.id)).collect();
    }
    vectorcraft_embroidery::outlines_mm(n).into_iter().filter(|l| l.len() >= 2).collect()
}

/// The objects (inside groups and layers) under `id` that have lines, each with its lines.
fn leaves_with_lines(d: &Document, id: NodeId, out: &mut Vec<(NodeId, Vec<Vec<Mm>>)>) {
    let Some(n) = d.node(id) else { return };
    if let Some(children) = n.children() {
        for c in children {
            leaves_with_lines(d, c.id, out);
        }
        return;
    }
    let lines: Vec<Vec<Mm>> = vectorcraft_embroidery::outlines_mm(n).into_iter().filter(|l| l.len() >= 2).collect();
    if !lines.is_empty() {
        out.push((id, lines));
    }
}

/// The selected (or `ids`) objects' lines, object by object: a group or layer gives its
/// objects', so what replaces them is never a container.
fn targets_lines(s: &Session, p: &Value) -> Result<Vec<(NodeId, Vec<Vec<Mm>>)>> {
    let d = &s.doc()?.doc;
    let mut out = Vec::new();
    for id in targets(s, p)? {
        leaves_with_lines(d, id, &mut out);
    }
    Ok(out)
}

/// A line drawn as quilting: a 0.75 pt black stroke, no fill.
fn line_node(line: &[Mm], name: &str) -> Option<Node> {
    let mut bp = BezPath::new();
    let mut it = line.iter();
    let first = it.next()?;
    bp.move_to((first.x * PT_PER_MM, first.y * PT_PER_MM));
    for q in it {
        bp.line_to((q.x * PT_PER_MM, q.y * PT_PER_MM));
    }
    let stroke = Paint::solid(Color::rgb8(0x20, 0x20, 0x20));
    let mut n = Node::path(NodeId(0), PathData::from_bezpath(&bp), Appearance::basic(Paint::None, stroke, 0.75));
    n.name = Some(name.to_string());
    Some(n)
}

/// Puts `nodes` where `at` is in the stacking (`(parent, index)`), as a group when there are
/// several, and selects what was added.
fn insert(s: &mut Session, label: &str, nodes: Vec<Node>, at: (Option<NodeId>, usize), group: Option<&str>, replace: &[NodeId]) -> Result<NodeId> {
    let replace = replace.to_vec();
    s.edit(label, |d, sel| {
        for id in &replace {
            d.remove(*id)?;
        }
        let (parent, idx) = at;
        let idx = idx.min(d.children(parent).map_or(0, Vec::len));
        let id = match group {
            Some(name) => {
                let mut kids = Vec::with_capacity(nodes.len());
                for mut n in nodes {
                    n.id = d.alloc_id();
                    kids.push(Arc::new(n));
                }
                let mut g = Node::group(d.alloc_id(), kids);
                g.name = Some(name.to_string());
                d.insert(parent, idx, g)?
            }
            None => {
                let mut n = nodes.into_iter().next().ok_or_else(|| EngineError::Other("nothing to add".into()))?;
                n.id = d.alloc_id();
                d.insert(parent, idx, n)?
            }
        };
        sel.set([id]);
        Ok(id)
    })
}

/// Where the first of `ids` is in the stacking, else the top of the layer new art goes in.
fn place_of(s: &Session, ids: &[NodeId]) -> Result<(Option<NodeId>, usize)> {
    let d = &s.doc()?.doc;
    match ids.first().and_then(|id| d.position(*id)) {
        Some((parent, idx, _)) => Ok((parent, idx)),
        None => Ok((s.doc()?.target_parent()?, usize::MAX)),
    }
}

fn tolerance(p: &Value, cmd: &str) -> Result<f64> {
    mm_param(p, "tolerance", 0.0, 50.0, Some(0.5), cmd)
}

fn check(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.check";
    let tol = tolerance(p, CMD)?;
    let pieces: Vec<Vec<Mm>> = targets_lines(s, p)?.into_iter().flat_map(|(_, l)| l).collect();
    let j = quilt::join(&pieces, tol).ok_or_else(|| bad(CMD, "select lines to check"))?;
    Ok(json!({
        "pieces": pieces.len(),
        "joins": j.joins,
        "longestJoin": j.longest_join,
        "continuous": j.joins == 0,
        "lengthMm": sf_core::geom::polyline::length(&j.line),
    }))
}

fn join(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.join";
    let tol = tolerance(p, CMD)?;
    let found = targets_lines(s, p)?;
    let pieces: Vec<Vec<Mm>> = found.iter().flat_map(|(_, l)| l.clone()).collect();
    let j = quilt::join(&pieces, tol).ok_or_else(|| bad(CMD, "select the lines to join"))?;
    let ids: Vec<NodeId> = found.iter().map(|(id, _)| *id).collect();
    let at = place_of(s, &ids)?;
    let node = line_node(&j.line, "Quilting line").ok_or_else(|| bad(CMD, "nothing to join"))?;
    let id = insert(s, "Join into One Line", vec![node], at, None, &ids)?;
    Ok(json!({ "id": id.0, "joins": j.joins, "longestJoin": j.longest_join, "pieces": pieces.len() }))
}

/// The one line the command works on: the selected (or `id`) object's, joined when it has
/// several pieces.
fn one_line(s: &Session, p: &Value, cmd: &str) -> Result<(NodeId, Vec<Mm>)> {
    let found = targets_lines(s, p)?;
    let (id, lines) = found.into_iter().next().ok_or_else(|| bad(cmd, "select a line"))?;
    let line = quilt::join(&lines, 0.5).map(|j| j.line).ok_or_else(|| bad(cmd, "select a line"))?;
    Ok((id, line))
}

fn edges(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.edges";
    let height = match str_param(p, "height") {
        None | Some("start") => EdgeHeight::Start,
        Some("middle") => EdgeHeight::Middle,
        Some(_) => return Err(bad(CMD, "`height` is start or middle")),
    };
    let (id, line) = one_line(s, p, CMD)?;
    let out = quilt::to_edges(&line, height);
    let node = line_node(&out, "Quilting line").ok_or_else(|| bad(CMD, "the line is empty"))?;
    let at = place_of(s, &[id])?;
    let new = insert(s, "Start and End on the Edges", vec![node], at, None, &[id])?;
    Ok(json!({ "id": new.0 }))
}

fn pantograph(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.pantograph";
    let width = mm_param(p, "width", 10.0, 20_000.0, None, CMD)?;
    let height = mm_param(p, "height", 10.0, 20_000.0, None, CMD)?;
    let nest = bool_or(p, "nest", true);
    let (id, line) = one_line(s, p, CMD)?;
    let motif = quilt::to_edges(&line, EdgeHeight::Start);
    let bb = sf_core::geom::polyline::bbox(&motif);
    let row = mm_param(p, "row", 1.0, 5_000.0, Some(bb.height().max(1.0)), CMD)?;
    let rows = quilt::pantograph(&motif, width, height, row, nest);
    if rows.is_empty() {
        return Err(bad(CMD, "the line is too small, or the quilt too big for it"));
    }
    // the rows from the line's top left
    let (ox, oy) = (bb.min.x, bb.min.y);
    let nodes: Vec<Node> = rows
        .iter()
        .enumerate()
        .filter_map(|(k, r)| {
            let moved: Vec<Mm> = r.iter().map(|q| Mm::new(q.x + ox, q.y + oy)).collect();
            line_node(&moved, &format!("Row {}", k + 1))
        })
        .collect();
    let n = nodes.len();
    let at = place_of(s, &[id])?;
    let g = insert(s, "Edge-to-Edge Rows", nodes, (at.0, at.1 + 1), Some("Edge-to-edge"), &[])?;
    Ok(json!({ "id": g.0, "rows": n }))
}

fn border(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.border";
    let length = mm_param(p, "length", 10.0, 20_000.0, None, CMD)?;
    let depth = mm_param(p, "depth", 2.0, 1_000.0, None, CMD)?;
    let down = match p.get("down") {
        None | Some(Value::Null) => None,
        Some(_) => Some(mm_param(p, "down", 10.0, 20_000.0, None, CMD)?),
    };
    let (id, line) = one_line(s, p, CMD)?;
    let motif = quilt::to_edges(&line, EdgeHeight::Start);
    let bb = sf_core::geom::polyline::bbox(&motif);
    let out = match down {
        Some(d) => quilt::corner(&motif, length, d, depth),
        None => quilt::border(&motif, length, depth),
    };
    if out.is_empty() {
        return Err(bad(CMD, "the line has no height to fit into a border"));
    }
    let moved: Vec<Mm> = out.iter().map(|q| Mm::new(q.x + bb.min.x, q.y + bb.min.y)).collect();
    let node = line_node(&moved, if down.is_some() { "Border corner" } else { "Border" }).ok_or_else(|| bad(CMD, "the border is empty"))?;
    let at = place_of(s, &[id])?;
    let new = insert(s, "Border", vec![node], (at.0, at.1 + 1), None, &[])?;
    Ok(json!({ "id": new.0 }))
}

fn fill(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.fill";
    let kind = str_param(p, "kind").ok_or_else(|| bad(CMD, "`kind` is stipple, meander, echo or ditch"))?;
    if !["stipple", "meander", "echo", "ditch"].contains(&kind) {
        return Err(bad(CMD, "`kind` is stipple, meander, echo or ditch"));
    }
    let spacing = mm_param(p, "spacing", 2.0, 100.0, Some(6.0), CMD)?;
    let ids = targets(s, p)?;
    let d = &s.doc()?.doc;
    if kind == "ditch" {
        // every outline, one line along the seams
        let pieces: Vec<Vec<Mm>> = ids.iter().flat_map(|id| lines_of(d, *id)).collect();
        let j = quilt::join(&pieces, 0.5).ok_or_else(|| bad(CMD, "select the shapes whose seams to quilt"))?;
        let node = line_node(&j.line, "Stitch in the ditch").ok_or_else(|| bad(CMD, "nothing to quilt"))?;
        let at = place_of(s, &ids)?;
        let new = insert(s, "Stitch in the Ditch", vec![node], (at.0, usize::MAX), None, &[])?;
        return Ok(json!({ "ids": [new.0], "joins": j.joins }));
    }
    let mut made = Vec::new();
    let shapes: Vec<(NodeId, Vec<sf_core::geom::Polygon>)> =
        ids.iter().filter_map(|id| d.node(*id).map(|n| (*id, vectorcraft_embroidery::regions_mm(n)))).filter(|(_, r)| !r.is_empty()).collect();
    if shapes.is_empty() {
        return Err(bad(CMD, "select closed shapes to fill"));
    }
    for (id, regions) in shapes {
        for region in regions.iter().take(200) {
            let line = match kind {
                "stipple" => quilt::meander(region, spacing, 0.6),
                "meander" => quilt::meander(region, spacing, 0.0),
                _ => quilt::echo(region, spacing),
            };
            let name = match kind {
                "stipple" => "Stipple",
                "meander" => "Meander",
                _ => "Echo",
            };
            let Some(node) = line_node(&line, name) else { continue };
            let at = place_of(s, &[id])?;
            made.push(insert(s, "Continuous Fill", vec![node], (at.0, at.1 + 1), None, &[])?.0);
        }
    }
    if made.is_empty() {
        return Err(bad(CMD, "the shapes are too small for that spacing"));
    }
    s.execute("select.set", &json!({ "ids": made }))?;
    Ok(json!({ "ids": made }))
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "quilt.export";
    let format = str_param(p, "format").unwrap_or("plt");
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = match p.get("ids") {
        None | Some(Value::Null) if s.doc()?.selection.objects.is_empty() => d.layers.iter().map(|l| l.id).collect(),
        _ => targets(s, p)?,
    };
    let lines: Vec<Vec<Mm>> = ids.iter().flat_map(|id| lines_of(d, *id)).collect();
    if lines.is_empty() {
        return Err(bad(CMD, "there are no lines to export"));
    }
    let text = match format {
        "plt" => quilt::plt(&lines),
        "dxf" => quilt::dxf(&lines),
        "svg" => quilt::svg(&lines),
        _ => return Err(bad(CMD, "`format` is plt, dxf or svg")),
    };
    Ok(json!({ "dataBase64": vectorcraft_format::base64_encode(text.as_bytes()), "format": format, "lines": lines.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        s
    }

    fn line(s: &mut Session, d: &str) -> u64 {
        s.execute("path.create", &json!({"d": d})).unwrap()["id"].as_u64().unwrap()
    }

    fn node_lines(s: &Session, id: u64) -> Vec<Vec<Mm>> {
        lines_of(&s.doc().unwrap().doc, NodeId(id))
    }

    /// Q1: pieces checked and joined into one line in their place; Undo takes it back.
    #[test]
    fn pieces_check_and_join_into_one_line() {
        let mut s = session();
        let a = line(&mut s, "M 100 100 L 200 100");
        let b = line(&mut s, "M 210 100 L 300 150");
        let c = line(&mut s, "M 400 400 L 500 400");
        s.execute("select.set", &json!({"ids": [a, b, c]})).unwrap();
        let r = s.execute("quilt.check", &json!({})).unwrap();
        assert_eq!((r["pieces"].as_u64(), r["joins"].as_u64(), r["continuous"].as_bool()), (Some(3), Some(2), Some(false)), "{r}");
        let r = s.execute("quilt.join", &json!({})).unwrap();
        assert_eq!(r["joins"], 2);
        let id = r["id"].as_u64().unwrap();
        assert_eq!(node_lines(&s, id).len(), 1, "one line");
        assert!(s.doc().unwrap().doc.node(NodeId(a)).is_none(), "the pieces gave way to it");
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let r = s.execute("quilt.check", &json!({})).unwrap();
        assert_eq!(r["continuous"], true);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(a)).is_some());
    }

    /// Q2–Q4: a motif run edge to edge, repeated in nesting rows across a quilt, and fitted along
    /// a border and round a corner.
    #[test]
    fn a_motif_makes_rows_borders_and_corners() {
        let mut s = session();
        // a wave 72 pt (25.4 mm) wide, drawn right to left
        let w = line(&mut s, "M 172 120 Q 154 100 136 120 Q 118 140 100 120");
        s.execute("select.set", &json!({"ids": [w]})).unwrap();
        let e = s.execute("quilt.edges", &json!({})).unwrap()["id"].as_u64().unwrap();
        let l = node_lines(&s, e);
        let (first, last) = (l[0].first().unwrap(), l[0].last().unwrap());
        assert!(first.x < last.x && (first.y - last.y).abs() < 1e-6, "left to right, ends at the start's height");
        s.execute("select.set", &json!({"ids": [e]})).unwrap();
        let r = s.execute("quilt.pantograph", &json!({"width": 254, "height": 100, "row": 20})).unwrap();
        assert_eq!(r["rows"], 5, "{r}");
        let g = r["id"].as_u64().unwrap();
        let rows = node_lines(&s, g);
        assert_eq!(rows.len(), 5, "a line a row");
        // the second row starts half a repeat along
        let x0 = |k: usize| rows[k].first().unwrap().x;
        assert!((x0(0) - x0(1) - 12.7).abs() < 0.2, "{} {}", x0(0), x0(1));
        s.execute("select.set", &json!({"ids": [e]})).unwrap();
        let b = s.execute("quilt.border", &json!({"length": 200, "depth": 15})).unwrap()["id"].as_u64().unwrap();
        let bb = sf_core::geom::polyline::bbox(&node_lines(&s, b)[0]);
        assert!((bb.width() - 200.0).abs() < 0.01 && (bb.height() - 15.0).abs() < 0.01, "{bb:?}");
        s.execute("select.set", &json!({"ids": [e]})).unwrap();
        let c = s.execute("quilt.border", &json!({"length": 200, "depth": 15, "down": 120})).unwrap()["id"].as_u64().unwrap();
        let cb = sf_core::geom::polyline::bbox(&node_lines(&s, c)[0]);
        assert!((cb.height() - 120.0).abs() < 0.5, "{cb:?}");
    }

    /// Q5: continuous fills in a shape, and stitch in the ditch along shapes' outlines.
    #[test]
    fn fills_and_ditch_lines() {
        let mut s = session();
        let r = s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 200, "height": 150})).unwrap()["id"].as_u64().unwrap();
        let r2 = s.execute("shape.rectangle", &json!({"x": 300, "y": 100, "width": 150, "height": 150})).unwrap()["id"].as_u64().unwrap();
        for kind in ["stipple", "meander", "echo"] {
            s.execute("select.set", &json!({"ids": [r]})).unwrap();
            let out = s.execute("quilt.fill", &json!({"kind": kind, "spacing": 8})).unwrap();
            let id = out["ids"][0].as_u64().unwrap();
            let l = node_lines(&s, id);
            assert_eq!(l.len(), 1, "{kind}: one line");
            assert!(l[0].len() > 10, "{kind}");
        }
        s.execute("select.set", &json!({"ids": [r, r2]})).unwrap();
        let d = s.execute("quilt.fill", &json!({"kind": "ditch"})).unwrap();
        assert_eq!(node_lines(&s, d["ids"][0].as_u64().unwrap()).len(), 1);
    }

    /// Q6, Q7: the lines as PLT, DXF and SVG.
    #[test]
    fn lines_export_as_single_stroke_files() {
        let mut s = session();
        line(&mut s, "M 100 100 L 200 100 L 200 200");
        line(&mut s, "M 300 300 L 400 350");
        s.execute("select.set", &json!({"ids": []})).unwrap();
        for (format, mark) in [("plt", "IN;SP1;"), ("dxf", "POLYLINE"), ("svg", "<polyline")] {
            let r = s.execute("quilt.export", &json!({"format": format})).unwrap();
            assert_eq!(r["lines"], 2, "{format}");
            let text = String::from_utf8(vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
            assert!(text.contains(mark), "{format}: {text}");
        }
        assert!(s.execute("quilt.export", &json!({"format": "qli"})).is_err());
    }

    #[test]
    fn bad_quilting_params_are_refused() {
        let mut s = session();
        let a = line(&mut s, "M 100 100 L 200 120");
        s.execute("select.set", &json!({"ids": [a]})).unwrap();
        for (cmd, p) in [
            ("quilt.pantograph", json!({"width": 1, "height": 100})),
            ("quilt.pantograph", json!({"height": 100})),
            ("quilt.border", json!({"length": 100, "depth": 0})),
            ("quilt.fill", json!({"kind": "zigzag"})),
            ("quilt.fill", json!({"kind": "stipple"})),
            ("quilt.edges", json!({"height": "top"})),
            ("quilt.check", json!({"tolerance": -1})),
        ] {
            assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
        }
    }
}
