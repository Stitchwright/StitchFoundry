// Adapted from PhotoCraft (https://github.com/storytold/photocraft, commit 04d0212, crates/geom/src/lib.rs), MIT OR Apache-2.0.
//! Integer rectangles in document pixels (origin at the canvas top-left; negative coordinates
//! are valid).

/// Integer rectangle, half-open: `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub const EMPTY: Rect = Rect { x0: 0, y0: 0, x1: 0, y1: 0 };

    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }
    pub fn from_xywh(x: i32, y: i32, w: u32, h: u32) -> Self {
        // Widen before adding: `w as i32` wraps negative for widths above `i32::MAX`, which
        // would turn the far edge backwards (an empty, inverted rect) instead of clamping it
        // to `i32::MAX` the way `saturating_add` alone would suggest.
        let x1 = (i64::from(x) + i64::from(w)).min(i64::from(i32::MAX)) as i32;
        let y1 = (i64::from(y) + i64::from(h)).min(i64::from(i32::MAX)) as i32;
        Self::new(x, y, x1, y1)
    }
    /// Horizontal extent; 0 when `x1 <= x0`. Exact even for i32-extreme edges.
    pub fn width(&self) -> u32 {
        if self.x1 > self.x0 { self.x1.abs_diff(self.x0) } else { 0 }
    }
    /// Vertical extent; 0 when `y1 <= y0`. Exact even for i32-extreme edges.
    pub fn height(&self) -> u32 {
        if self.y1 > self.y0 { self.y1.abs_diff(self.y0) } else { 0 }
    }
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
    /// `other` lies entirely inside `self` (an empty `other` is contained in anything).
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.is_empty() || (other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1)
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let r = Rect::new(self.x0.max(o.x0), self.y0.max(o.y0), self.x1.min(o.x1), self.y1.min(o.y1));
        if r.is_empty() { Rect::EMPTY } else { r }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Rect::new(self.x0.min(o.x0), self.y0.min(o.y0), self.x1.max(o.x1), self.y1.max(o.y1))
    }
    pub fn translate(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x0.saturating_add(dx), self.y0.saturating_add(dy), self.x1.saturating_add(dx), self.y1.saturating_add(dy))
    }
    pub fn inflate(&self, d: i32) -> Rect {
        Rect::new(self.x0.saturating_sub(d), self.y0.saturating_sub(d), self.x1.saturating_add(d), self.y1.saturating_add(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_basics() {
        let r = Rect::from_xywh(10, 20, 30, 40);
        assert_eq!(r.width(), 30);
        assert_eq!(r.height(), 40);
        assert!(r.contains(10, 20));
        assert!(!r.contains(40, 20));
        assert!(Rect::new(5, 5, 5, 10).is_empty());
    }

    #[test]
    fn intersect_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 15, 15);
        assert_eq!(a.intersect(&b), Rect::new(5, 5, 10, 10));
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));
        assert_eq!(a.intersect(&Rect::new(20, 20, 30, 30)), Rect::EMPTY);
        assert_eq!(Rect::EMPTY.union(&a), a);
    }

    #[test]
    fn translate_and_inflate_saturate_on_overflow() {
        let r = Rect::new(i32::MAX - 10, i32::MIN + 10, i32::MAX, i32::MIN + 20);
        let t = r.translate(100, -100);
        assert_eq!(t.x0, i32::MAX);
        assert_eq!(t.x1, i32::MAX);
        assert_eq!(t.y0, i32::MIN);
        assert_eq!(t.y1, i32::MIN);
        let grown = Rect::new(i32::MAX - 5, i32::MIN + 5, i32::MAX - 1, i32::MIN + 10).inflate(10);
        assert_eq!(grown.x1, i32::MAX);
        assert_eq!(grown.y0, i32::MIN);
    }

    #[test]
    fn from_xywh_clamps_huge_sizes_instead_of_wrapping() {
        // Widths above `i32::MAX` used to wrap negative through `w as i32`, producing an
        // empty inverted rect instead of clamping the far edge to `i32::MAX`.
        assert_eq!(Rect::from_xywh(0, 0, u32::MAX, 10), Rect::new(0, 0, i32::MAX, 10));
        assert_eq!(Rect::from_xywh(-100, 5, u32::MAX, u32::MAX), Rect::new(-100, 5, i32::MAX, i32::MAX));
        assert_eq!(Rect::from_xywh(10, 20, i32::MAX as u32 + 1, 40).width(), i32::MAX as u32 - 10);
        // Ordinary sizes are unchanged.
        assert_eq!(Rect::from_xywh(10, 20, 30, 40), Rect::new(10, 20, 40, 60));
    }

    #[test]
    fn width_and_height_do_not_overflow_on_extreme_edges() {
        // Inverted extreme rect: empty, so 0 (x1 - x0 used to overflow and panic).
        let r = Rect::new(i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        assert_eq!((r.width(), r.height()), (0, 0));
        assert!(r.is_empty());
        // Widest possible rect: the true extent fits in u32.
        let r = Rect::new(i32::MIN, i32::MIN + 20, i32::MAX, i32::MAX - 5);
        assert_eq!(r.width(), u32::MAX);
        assert_eq!(r.height(), u32::MAX - 25);
    }
}
