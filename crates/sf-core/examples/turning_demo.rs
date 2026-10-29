//! Turning fills: an S-shaped band with automatically placed inclination lines
//! (satin), and a curved band with hand-placed lines (tatami). Prints the
//! machine pattern as JSON plus the guides on stderr.

use sf_core::direction::{Guide, auto_guides};
use sf_core::geom::{Point, Polygon, Shape};
use sf_core::model::{Design, FillObject, FillParams, ObjectKind};

fn s_band() -> Vec<Point> {
    // centreline of an "S": upper arc (centre (0,-6)) from the right, over the top,
    // down to the middle, then the lower arc (centre (0,6)) round to the left.
    let r = 6.0;
    let mut center = Vec::new();
    for i in 0..=30 {
        let t: f64 = (i as f64 / 30.0 * 270.0).to_radians();
        center.push(Point::new(r * t.cos(), -6.0 - r * t.sin()));
    }
    for i in 1..=30 {
        let t: f64 = (90.0 - i as f64 / 30.0 * 270.0).to_radians();
        center.push(Point::new(r * t.cos(), 6.0 - r * t.sin()));
    }
    let left = sf_core::geom::polyline::offset_polyline(&center, 1.8, false, 2.0);
    let mut right = sf_core::geom::polyline::offset_polyline(&center, -1.8, false, 2.0);
    right.reverse();
    let mut ring = left;
    ring.extend(right);
    ring
}

fn main() {
    let mut d = Design::new();
    let ring = s_band();
    let guides = auto_guides(&ring).unwrap_or_default();
    eprintln!("auto guides: {}", guides.len());
    let mut f = FillObject::new(Shape::from_polygon(&Polygon::from_ring(ring)), FillParams::satin());
    f.guides = guides;
    d.add(ObjectKind::Fill(f), 1);

    let mut band = Vec::new();
    for i in 0..=24 {
        let a = std::f64::consts::PI * i as f64 / 24.0;
        band.push(Point::new(40.0 + 20.0 * a.cos(), 5.0 - 20.0 * a.sin()));
    }
    for i in (0..=24).rev() {
        let a = std::f64::consts::PI * i as f64 / 24.0;
        band.push(Point::new(40.0 + 12.0 * a.cos(), 5.0 - 12.0 * a.sin()));
    }
    let g = |deg: f64| {
        let a: f64 = deg.to_radians();
        Guide::new(Point::new(40.0 + 10.0 * a.cos(), 5.0 - 10.0 * a.sin()), Point::new(40.0 + 22.0 * a.cos(), 5.0 - 22.0 * a.sin()))
    };
    let mut f = FillObject::new(Shape::from_polygon(&Polygon::from_ring(band)), FillParams { spacing: 0.45, ..FillParams::default() });
    f.guides = vec![g(20.0), g(90.0), g(160.0)];
    d.add(ObjectKind::Fill(f), 0);

    let built = sf_core::engine::build(&d);
    match serde_json::to_string(&built.pattern) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("cannot serialise pattern: {e}"),
    }
}
