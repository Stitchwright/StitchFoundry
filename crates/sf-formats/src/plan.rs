//! The shared preparation every writer runs ([`build_plan`]), its
//! format-independent public form ([`normalize`]) and the decoder readers
//! build patterns with ([`Decoder`]).

use crate::FormatError;
use sf_core::threads::{ThreadChart, nearest_index};
use sf_core::{Cmd, Pattern, Stitch, Thread};

/// Readers refuse files that decode to more records than this.
pub(crate) const MAX_RECORDS: usize = 5_000_000;

/// One machine record in file units, absolute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rec {
    pub cmd: Cmd,
    pub x: i64,
    pub y: i64,
}

/// A prepared pattern in file units.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    pub recs: Vec<Rec>,
    /// One thread per colour block.
    pub threads: Vec<Thread>,
}

/// How a format wants its records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlanCfg {
    /// File units per millimetre (10 for 0.1 mm).
    pub units_per_mm: f64,
    /// The file's +y points up (the pattern's points down).
    pub y_up: bool,
    /// Longest stitch per axis, in units.
    pub max_stitch: i64,
    /// Longest jump per axis, in units.
    pub max_jump: i64,
    /// Largest absolute coordinate, in units.
    pub max_coord: i64,
    /// Centre the stitches on the origin.
    pub center: bool,
    /// The format has a stop command (else stops become colour changes).
    pub native_stop: bool,
    /// The needle starts at the origin: a jump leads to the first stitch.
    pub from_origin: bool,
    /// Keep every stop, also stops with nothing sewn between them (a chenille machine's stops
    /// each set something: the colour, the stitch, the needle height).
    pub every_stop: bool,
}

impl PlanCfg {
    /// The usual 0.1 mm machine configuration.
    pub fn tenth_mm(y_up: bool, max_stitch: i64, max_jump: i64, center: bool) -> Self {
        PlanCfg {
            units_per_mm: 10.0,
            y_up,
            max_stitch,
            max_jump,
            // 10 m: far beyond any sewing field, small enough that splitting stays bounded.
            max_coord: 100_000,
            center,
            native_stop: false,
            from_origin: true,
            every_stop: false,
        }
    }

    /// This configuration as the write options ask (keeping every stop).
    pub fn with(self, opts: &crate::WriteOptions) -> Self {
        PlanCfg { every_stop: opts.every_stop, ..self }
    }

    /// Pattern millimetres → file units.
    fn to_units(self, x: f64, y: f64) -> Result<(i64, i64), FormatError> {
        let ux = (x * self.units_per_mm).round();
        let uy = (if self.y_up { -y } else { y } * self.units_per_mm).round();
        if !(ux.is_finite() && uy.is_finite()) {
            return Err(FormatError::Invalid("non-finite stitch coordinate".into()));
        }
        let lim = self.max_coord as f64;
        if ux.abs() > lim || uy.abs() > lim {
            return Err(FormatError::TooLarge);
        }
        Ok((ux as i64, uy as i64))
    }

    /// File units → pattern millimetres.
    pub fn to_mm(self, x: i64, y: i64) -> (f64, f64) {
        let mx = x as f64 / self.units_per_mm;
        let my = y as f64 / self.units_per_mm;
        (mx, if self.y_up { -my } else { my })
    }
}

impl Plan {
    /// The plan as a pattern in millimetres (+y down).
    pub fn to_pattern(&self, cfg: &PlanCfg) -> Pattern {
        let mut p = Pattern::new();
        p.stitches = self
            .recs
            .iter()
            .map(|r| {
                let (x, y) = cfg.to_mm(r.x, r.y);
                Stitch::new(x, y, r.cmd)
            })
            .collect();
        p.threads = self.threads.clone();
        p
    }

    /// Number of colour changes.
    pub fn color_changes(&self) -> usize {
        self.recs.iter().filter(|r| r.cmd == Cmd::ColorChange).count()
    }

    /// Bounds `(min_x, min_y, max_x, max_y)` of the records with `cmd` in
    /// `cmds`, or `None` if there are none.
    pub fn bounds(&self, cmds: &[Cmd]) -> Option<(i64, i64, i64, i64)> {
        let mut it = self.recs.iter().filter(|r| cmds.contains(&r.cmd));
        let first = it.next()?;
        Some(it.fold((first.x, first.y, first.x, first.y), |(a, b, c, d), r| (a.min(r.x), b.min(r.y), c.max(r.x), d.max(r.y))))
    }

    /// Replaces the threads with the generic palette (formats without colours).
    pub fn use_generic_threads(&mut self) {
        self.threads = generic_threads(self.color_changes() + 1);
    }

    /// Maps every thread to the nearest colour of `chart` (whose entry 0 is a
    /// placeholder and never chosen); returns the chart indices.
    pub fn map_to_chart(&mut self, chart: &ThreadChart) -> Vec<usize> {
        self.map_to_chart_from(chart, 1)
    }

    /// As [`Plan::map_to_chart`], choosing from the chart's threads from index `first` on (1 for
    /// the PEC and JEF palettes, whose index 0 is a placeholder; 0 for HUS's, which starts with
    /// black).
    pub fn map_to_chart_from(&mut self, chart: &ThreadChart, first: usize) -> Vec<usize> {
        let mut indices = Vec::with_capacity(self.threads.len());
        for t in &mut self.threads {
            let i = nearest_index(chart.threads.iter().skip(first).map(|t| t.color), t.color).map_or(first, |i| i + first);
            if let Some(c) = chart.threads.get(i) {
                *t = c.clone();
            }
            indices.push(i);
        }
        indices
    }
}

/// `n` threads from the generic palette.
pub(crate) fn generic_threads(n: usize) -> Vec<Thread> {
    let empty = Pattern::new();
    (0..n).map(|i| empty.thread(i)).collect()
}

/// `a / b` rounded half away from zero (`b > 0`).
fn div_round(a: i128, b: i128) -> i128 {
    if a >= 0 { (2 * a + b) / (2 * b) } else { -((-2 * a + b) / (2 * b)) }
}

/// Appends a move from `cur` to `to` as `cmd` records, split into equal steps
/// so that no step exceeds `limit` units on either axis.
fn push_move(recs: &mut Vec<Rec>, cur: &mut (i64, i64), to: (i64, i64), cmd: Cmd, limit: i64) {
    let limit = i128::from(limit.max(1));
    let (dx, dy) = (i128::from(to.0) - i128::from(cur.0), i128::from(to.1) - i128::from(cur.1));
    let steps = |d: i128| (d.abs() + limit - 1) / limit;
    let n = steps(dx).max(steps(dy)).max(1);
    for i in 1..n {
        let x = i128::from(cur.0) + div_round(dx * i, n);
        let y = i128::from(cur.1) + div_round(dy * i, n);
        // Between `cur` and `to`, so within i64.
        recs.push(Rec { cmd, x: x as i64, y: y as i64 });
    }
    recs.push(Rec { cmd, x: to.0, y: to.1 });
    *cur = to;
}

/// Removes the stops after the last stitch (redundant before a colour change or the end).
fn drop_trailing_stops(recs: &mut Vec<Rec>) {
    let start = recs.iter().rposition(|r| r.cmd == Cmd::Stitch).map_or(0, |i| i + 1);
    let tail: Vec<Rec> = recs.drain(start..).filter(|r| r.cmd != Cmd::Stop).collect();
    recs.extend(tail);
}

/// The shared preparation every writer runs: see the crate documentation.
pub(crate) fn build_plan(pattern: &Pattern, cfg: &PlanCfg) -> Result<Plan, FormatError> {
    if pattern.stitches.iter().any(|s| s.is_move() && !(s.x.is_finite() && s.y.is_finite())) {
        return Err(FormatError::Invalid("non-finite stitch coordinate".into()));
    }
    let offset = if cfg.center {
        let c = pattern.bounds().center();
        (-c.x, -c.y)
    } else {
        (0.0, 0.0)
    };

    // Colour blocks that sew something; the records of empty blocks (jumps,
    // trims, stops) move on to the next block that does.
    let mut kept: Vec<(Thread, Vec<Stitch>)> = Vec::new();
    let mut carry: Vec<Stitch> = Vec::new();
    for block in pattern.color_blocks() {
        let recs = pattern.stitches.get(block.range.clone()).unwrap_or_default();
        let mut list = std::mem::take(&mut carry);
        list.extend(recs.iter().copied().filter(|s| !matches!(s.cmd, Cmd::End | Cmd::ColorChange)));
        if recs.iter().any(|s| s.cmd == Cmd::Stitch) {
            kept.push((pattern.thread(block.thread), list));
        } else {
            carry = list;
        }
    }
    if let Some(last) = kept.last_mut() {
        last.1.append(&mut carry);
    } else {
        return Err(FormatError::Empty);
    }

    let mut recs: Vec<Rec> = Vec::new();
    let mut threads = Vec::with_capacity(kept.len());
    let mut cur = (0i64, 0i64);
    let mut first_move = true;
    // Whether anything was sewn since the last trim / stop (or colour change, or the start).
    let mut sewn_since_trim = false;
    let mut sewn_since_stop = false;
    for (k, (thread, list)) in kept.into_iter().enumerate() {
        if k > 0 {
            drop_trailing_stops(&mut recs);
            recs.push(Rec { cmd: Cmd::ColorChange, x: cur.0, y: cur.1 });
            sewn_since_trim = false;
            sewn_since_stop = false;
        }
        threads.push(thread);
        for s in list {
            match s.cmd {
                Cmd::Stitch | Cmd::Jump => {
                    let to = cfg.to_units(s.x + offset.0, s.y + offset.1)?;
                    if first_move {
                        first_move = false;
                        if !cfg.from_origin {
                            recs.push(Rec { cmd: s.cmd, x: to.0, y: to.1 });
                            cur = to;
                            if s.cmd == Cmd::Stitch {
                                sewn_since_trim = true;
                                sewn_since_stop = true;
                            }
                            continue;
                        }
                        if s.cmd == Cmd::Stitch && to != cur {
                            push_move(&mut recs, &mut cur, to, Cmd::Jump, cfg.max_jump);
                        }
                    }
                    if s.cmd == Cmd::Jump {
                        if to != cur {
                            push_move(&mut recs, &mut cur, to, Cmd::Jump, cfg.max_jump);
                        }
                    } else {
                        push_move(&mut recs, &mut cur, to, Cmd::Stitch, cfg.max_stitch);
                        sewn_since_trim = true;
                        sewn_since_stop = true;
                    }
                }
                Cmd::Trim => {
                    if sewn_since_trim {
                        recs.push(Rec { cmd: Cmd::Trim, x: cur.0, y: cur.1 });
                        sewn_since_trim = false;
                    }
                }
                Cmd::Stop => {
                    if sewn_since_stop || cfg.every_stop {
                        recs.push(Rec { cmd: Cmd::Stop, x: cur.0, y: cur.1 });
                        sewn_since_stop = false;
                    }
                }
                Cmd::ColorChange | Cmd::End => {}
            }
        }
    }
    drop_trailing_stops(&mut recs);
    recs.push(Rec { cmd: Cmd::End, x: cur.0, y: cur.1 });

    if !cfg.native_stop {
        // A stop is a colour change that keeps the thread.
        let mut out = Vec::with_capacity(threads.len());
        let mut block = 0;
        out.extend(threads.first().cloned());
        for r in &mut recs {
            match r.cmd {
                Cmd::ColorChange => {
                    block += 1;
                    out.extend(threads.get(block).cloned());
                }
                Cmd::Stop => {
                    r.cmd = Cmd::ColorChange;
                    let same = out.last().cloned();
                    out.extend(same);
                }
                _ => {}
            }
        }
        threads = out;
    }
    Ok(Plan { recs, threads })
}

/// Format-independent clean-up of a pattern, as every writer does it first.
///
/// * leading trims, stops and colour changes are dropped, and so is a trim or
///   stop when nothing was sewn since the previous one (or since a colour
///   change), a stop right before a colour change or the end, and a jump that
///   does not move;
/// * colour blocks without stitches are removed together with their thread
///   (their jumps move on to the next block), so thread `i` still belongs to
///   colour block `i`;
/// * a move longer than `max_step_mm` on either axis (machines limit each axis
///   separately) is split into equal steps: stitches into stitches, jumps into
///   jumps;
/// * end records inside the pattern are dropped and exactly one closes it.
///
/// Positions are rounded to 1 µm; move records with non-finite coordinates or
/// beyond ±10 m are dropped. `max_step_mm` is at least 0.1 mm. Writers also
/// centre the design, add a jump from the origin to the first stitch and round
/// to the file's resolution: [`crate::prepare`] shows that full result.
pub fn normalize(pattern: &Pattern, max_step_mm: f64) -> Pattern {
    const UNITS: f64 = 1_000.0;
    const MAX_COORD: i64 = 10_000_000;
    let step = if max_step_mm >= 0.1 { max_step_mm } else { 0.1 };
    let limit = if step * UNITS < (4 * MAX_COORD) as f64 { (step * UNITS).floor() as i64 } else { 4 * MAX_COORD };
    let cfg = PlanCfg {
        units_per_mm: UNITS,
        y_up: false,
        max_stitch: limit,
        max_jump: limit,
        max_coord: MAX_COORD,
        center: false,
        native_stop: true,
        from_origin: false,
        every_stop: false,
    };
    let reach = MAX_COORD as f64 / UNITS;
    let mut clean = pattern.clone();
    clean.stitches.retain(|s| !s.is_move() || (s.x.is_finite() && s.y.is_finite() && s.x.abs() <= reach && s.y.abs() <= reach));
    let mut out = match build_plan(&clean, &cfg) {
        Ok(plan) => plan.to_pattern(&cfg),
        Err(_) => {
            let mut p = Pattern::new();
            p.stitches.push(Stitch::new(0.0, 0.0, Cmd::End));
            p
        }
    };
    out.name = pattern.name.clone();
    out
}

/// Builds a pattern from the relative moves a reader decodes.
#[derive(Debug)]
pub(crate) struct Decoder {
    units_per_mm: f64,
    y_up: bool,
    /// Current position in file units.
    x: i64,
    y: i64,
    out: Vec<Stitch>,
}

impl Decoder {
    pub fn new(units_per_mm: f64, y_up: bool) -> Self {
        Decoder { units_per_mm, y_up, x: 0, y: 0, out: Vec::new() }
    }

    fn push(&mut self, cmd: Cmd) -> Result<(), FormatError> {
        if self.out.len() >= MAX_RECORDS {
            return Err(FormatError::TooLarge);
        }
        let mx = self.x as f64 / self.units_per_mm;
        let my = self.y as f64 / self.units_per_mm;
        self.out.push(Stitch::new(mx, if self.y_up { -my } else { my }, cmd));
        Ok(())
    }

    fn step(&mut self, dx: i64, dy: i64) {
        self.x = self.x.saturating_add(dx);
        self.y = self.y.saturating_add(dy);
    }

    /// A stitch by `(dx, dy)` file units.
    pub fn stitch(&mut self, dx: i64, dy: i64) -> Result<(), FormatError> {
        self.step(dx, dy);
        self.push(Cmd::Stitch)
    }

    /// A jump by `(dx, dy)` file units.
    pub fn jump(&mut self, dx: i64, dy: i64) -> Result<(), FormatError> {
        self.step(dx, dy);
        self.push(Cmd::Jump)
    }

    /// A jump to the absolute position `(x, y)` unless already there.
    pub fn jump_to(&mut self, x: i64, y: i64) -> Result<(), FormatError> {
        if (x, y) == (self.x, self.y) {
            return Ok(());
        }
        self.x = x;
        self.y = y;
        self.push(Cmd::Jump)
    }

    /// A command record (trim, stop, colour change) at the current position.
    pub fn command(&mut self, cmd: Cmd) -> Result<(), FormatError> {
        self.push(cmd)
    }

    /// Number of colour changes so far.
    pub fn color_changes(&self) -> usize {
        self.out.iter().filter(|s| s.cmd == Cmd::ColorChange).count()
    }

    /// The finished pattern: one end record appended, exactly one thread per
    /// colour block (`threads` first, then the generic palette).
    pub fn finish(mut self, mut threads: Vec<Thread>, name: String) -> Result<Pattern, FormatError> {
        if !self.out.iter().any(|s| s.cmd == Cmd::Stitch) {
            return Err(FormatError::Empty);
        }
        let (mx, my) = (self.x as f64 / self.units_per_mm, self.y as f64 / self.units_per_mm);
        self.out.push(Stitch::new(mx, if self.y_up { -my } else { my }, Cmd::End));
        threads.truncate(self.color_changes() + 1);
        let mut p = Pattern::new();
        p.stitches = self.out;
        p.threads = threads;
        p.name = name;
        p.fill_missing_threads();
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::Point;

    fn cmds(p: &Pattern) -> Vec<Cmd> {
        p.stitches.iter().map(|s| s.cmd).collect()
    }

    #[test]
    fn rounding_halves_away_from_zero() {
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-4, 3), -1);
    }

    #[test]
    fn long_moves_split_into_equal_steps_within_the_limit() {
        let mut recs = Vec::new();
        let mut cur = (0, 0);
        push_move(&mut recs, &mut cur, (1000, -363), Cmd::Jump, 121);
        assert_eq!(recs.len(), 9);
        let mut prev = (0, 0);
        for r in &recs {
            assert!((r.x - prev.0).abs() <= 121 && (r.y - prev.1).abs() <= 121, "{r:?}");
            prev = (r.x, r.y);
        }
        assert_eq!(prev, (1000, -363));
        assert_eq!(cur, (1000, -363));
    }

    #[test]
    fn normalize_cleans_commands_and_blocks() {
        let mut p = Pattern::new();
        p.color_change(); // leading: dropped with its thread
        p.trim(); // leading: dropped
        p.stitch(Point::new(0.0, 0.0));
        p.stitch(Point::new(30.0, 0.0)); // split into 3 steps of 10 mm
        p.trim();
        p.trim(); // duplicate
        p.jump(Point::new(30.0, 0.0)); // does not move
        p.stop();
        p.color_change(); // stop right before a colour change: dropped
        p.jump(Point::new(25.0, 5.0)); // empty block: its jump moves on
        p.color_change();
        p.stitch(Point::new(25.0, 6.0));
        p.add(Point::new(25.0, 6.0), Cmd::End);
        p.stitch(Point::new(25.0, 7.0)); // after an inner end: kept
        p.threads = vec![
            Thread::new(sf_core::Rgb::new(1, 1, 1), "dropped"),
            Thread::new(sf_core::Rgb::new(2, 2, 2), "first"),
            Thread::new(sf_core::Rgb::new(3, 3, 3), "empty"),
            Thread::new(sf_core::Rgb::new(4, 4, 4), "second"),
        ];
        let n = normalize(&p, 10.0);
        use Cmd::*;
        assert_eq!(cmds(&n), vec![Stitch, Stitch, Stitch, Stitch, Trim, ColorChange, Jump, Stitch, Stitch, End]);
        assert_eq!(n.threads.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["first", "second"]);
        assert!((n.stitches[1].x - 10.0).abs() < 1e-9);
        assert_eq!(n.stitches[6].pos(), Point::new(25.0, 5.0));
    }

    #[test]
    fn stops_become_colour_changes_keeping_the_thread() {
        let mut p = Pattern::new();
        p.stitch(Point::new(0.0, 0.0));
        p.stitch(Point::new(1.0, 0.0));
        p.stop();
        p.stitch(Point::new(2.0, 0.0));
        p.color_change();
        p.stitch(Point::new(3.0, 0.0));
        p.threads = vec![Thread::new(sf_core::Rgb::new(1, 1, 1), "a"), Thread::new(sf_core::Rgb::new(2, 2, 2), "b")];
        let cfg = PlanCfg::tenth_mm(true, 121, 121, false);
        let plan = build_plan(&p, &cfg).unwrap();
        assert_eq!(plan.color_changes(), 2);
        assert_eq!(plan.threads.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["a", "a", "b"]);
        let native = build_plan(&p, &PlanCfg { native_stop: true, ..cfg }).unwrap();
        assert_eq!(native.color_changes(), 1);
        assert_eq!(native.recs.iter().filter(|r| r.cmd == Cmd::Stop).count(), 1);
    }

    #[test]
    fn plans_start_with_a_jump_from_the_origin() {
        let mut p = Pattern::new();
        p.stitch(Point::new(-20.0, 10.0));
        p.stitch(Point::new(-19.0, 10.0));
        let cfg = PlanCfg::tenth_mm(true, 121, 121, false);
        let plan = build_plan(&p, &cfg).unwrap();
        let c: Vec<Cmd> = plan.recs.iter().map(|r| r.cmd).collect();
        assert_eq!(c, vec![Cmd::Jump, Cmd::Jump, Cmd::Stitch, Cmd::Stitch, Cmd::End]);
        assert_eq!((plan.recs[1].x, plan.recs[1].y), (-200, -100));
        let back = plan.to_pattern(&cfg);
        assert_eq!(back.stitches[2].pos(), Point::new(-20.0, 10.0));
    }

    #[test]
    fn invalid_patterns_are_refused() {
        let cfg = PlanCfg::tenth_mm(true, 121, 121, true);
        assert_eq!(build_plan(&Pattern::new(), &cfg), Err(FormatError::Empty));
        let mut p = Pattern::new();
        p.jump(Point::new(1.0, 1.0));
        p.trim();
        assert_eq!(build_plan(&p, &cfg), Err(FormatError::Empty));
        p.stitch(Point::new(f64::NAN, 0.0));
        assert!(matches!(build_plan(&p, &cfg), Err(FormatError::Invalid(_))));
        let mut far = Pattern::new();
        far.stitch(Point::new(0.0, 0.0));
        far.stitch(Point::new(20_000.0, 0.0));
        assert_eq!(build_plan(&far, &PlanCfg { center: false, ..cfg }), Err(FormatError::TooLarge));
        // normalize never fails.
        assert_eq!(cmds(&normalize(&far, f64::NAN)).last(), Some(&Cmd::End));
        assert_eq!(cmds(&normalize(&Pattern::new(), 1.0)), vec![Cmd::End]);
    }
}
