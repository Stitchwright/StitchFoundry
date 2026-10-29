//! Builds a demo design exercising every object type and prints the resulting
//! machine pattern as JSON (used for visual checks of the stitch engine).
//!
//! `cargo run -p sf-core --example engine_demo > pattern.json`

use sf_core::geom::{Column, Contour, Node, NodeKind, Point, Polygon, Rung, Shape};
use sf_core::model::{
    AppliqueObject, AppliqueParams, Design, FillKind, FillObject, FillParams, ObjectKind, RunKind, RunObject, RunParams, SatinObject, SatinParams,
};

fn main() {
    let mut d = Design::new();
    // Tatami ellipse with a hole.
    let mut shape = Shape::ellipse(Point::new(-35.0, 0.0), 22.0, 16.0);
    shape.holes.push(Shape::ellipse(Point::new(-35.0, 0.0), 7.0, 5.0).outer);
    d.add(ObjectKind::Fill(FillObject::new(shape, FillParams { angle: 30.0, ..Default::default() })), 0);
    // U-shaped tatami.
    let u = Polygon::from_ring(vec![
        Point::new(0.0, -15.0),
        Point::new(6.0, -15.0),
        Point::new(6.0, 8.0),
        Point::new(14.0, 8.0),
        Point::new(14.0, -15.0),
        Point::new(20.0, -15.0),
        Point::new(20.0, 15.0),
        Point::new(0.0, 15.0),
    ]);
    d.add(ObjectKind::Fill(FillObject::new(Shape::from_polygon(&u), FillParams { angle: 0.0, ..Default::default() })), 1);
    // Curved satin column.
    let rungs: Vec<Rung> = (0..=6)
        .map(|i| {
            let a = std::f64::consts::PI * (1.0 + i as f64 / 6.0);
            let c = Point::new(45.0, 10.0);
            let dir = Point::new(a.cos(), a.sin());
            Rung::new(c + dir * 18.0, c + dir * 13.0, NodeKind::Curve)
        })
        .collect();
    d.add(ObjectKind::Satin(SatinObject { column: Column::new(rungs), params: SatinParams::default() }), 2);
    // Runs.
    let path = Contour::new(
        vec![
            Node::corner(Point::new(-55.0, 25.0)),
            Node::curve(Point::new(-40.0, 32.0)),
            Node::curve(Point::new(-25.0, 25.0)),
            Node::corner(Point::new(-10.0, 32.0)),
        ],
        false,
    );
    d.add(ObjectKind::Run(RunObject { path: path.clone(), params: RunParams { kind: RunKind::Triple, ..Default::default() } }), 3);
    let mut p2 = path.clone();
    p2.transform(&sf_core::Affine::translate(Point::new(0.0, 8.0)));
    d.add(ObjectKind::Run(RunObject { path: p2, params: RunParams { kind: RunKind::Motif, width: 3.0, spacing: 4.0, ..Default::default() } }), 3);
    // Satin fill and radial / contour / spiral fills.
    let star: Vec<Point> = (0..10)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 10.0 - std::f64::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 12.0 } else { 5.0 };
            Point::new(5.0 + r * a.cos(), 40.0 + r * a.sin())
        })
        .collect();
    d.add(
        ObjectKind::Fill(FillObject::new(
            Shape::from_polygon(&Polygon::from_ring(star)),
            FillParams { kind: FillKind::Satin, angle: 90.0, ..Default::default() },
        )),
        4,
    );
    for (i, kind) in [FillKind::Contour, FillKind::Spiral, FillKind::Radial].into_iter().enumerate() {
        let c = Point::new(32.0 + i as f64 * 22.0, 40.0);
        let params = FillParams { kind, spacing: if kind == FillKind::Radial { 0.8 } else { 0.6 }, ..Default::default() };
        d.add(ObjectKind::Fill(FillObject::new(Shape::ellipse(c, 10.0, 8.0), params)), 5 + i);
    }
    // Appliqué.
    d.add(
        ObjectKind::Applique(AppliqueObject {
            shape: Shape::ellipse(Point::new(-35.0, 45.0), 14.0, 10.0),
            params: AppliqueParams::default(),
            covered: Vec::new(),
            extra: Vec::new(),
        }),
        8,
    );
    let built = sf_core::engine::build(&d);
    let stats = sf_core::stats::pattern_stats(&built.pattern, &d.settings);
    eprintln!(
        "stitches {} trims {} colors {} size {:.1}x{:.1} mm time {}",
        stats.stitches,
        stats.trims,
        stats.colors.len(),
        stats.width_mm,
        stats.height_mm,
        stats.time_string()
    );
    match serde_json::to_string(&built.pattern) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("cannot serialise pattern: {e}"),
    }
}
