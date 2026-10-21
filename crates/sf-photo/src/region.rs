// Adapted from PhotoCraft (https://github.com/storytold/photocraft, commit 04d0212, crates/algo/src/selection.rs), MIT OR Apache-2.0.
//! An 8-bit coverage region: the result type of segmentation and edge refinement.

use crate::geom::Rect;

/// A selected region: 8-bit coverage over `bbox` (row-major), the bounding box of the pixels
/// touched (including the anti-aliased fringe).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub bbox: Rect,
    pub mask: Vec<u8>,
}

impl Region {
    /// Coverage (0–1) at document pixel `(x, y)`; zero outside the box.
    pub fn at(&self, x: i32, y: i32) -> f32 {
        if !self.bbox.contains(x, y) {
            return 0.0;
        }
        let row = (y.abs_diff(self.bbox.y0) as usize).checked_mul(self.bbox.width() as usize);
        let value = row.and_then(|i| i.checked_add(x.abs_diff(self.bbox.x0) as usize)).and_then(|i| self.mask.get(i)).copied().unwrap_or(0);
        f32::from(value) / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_coverage_outside_a_short_mask_is_zero() {
        let region = Region { bbox: Rect::new(-2, -3, 1, -1), mask: vec![255, 128] };
        assert_eq!(region.at(-2, -3), 1.0);
        assert!((region.at(-1, -3) - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(region.at(0, -3), 0.0);
        assert_eq!(region.at(-2, -2), 0.0);
        assert_eq!(region.at(-3, -3), 0.0);

        let extreme = Region { bbox: Rect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX), mask: vec![] };
        assert_eq!(extreme.at(i32::MAX - 1, i32::MAX - 1), 0.0);
        assert_eq!(extreme.at(i32::MIN, i32::MIN), 0.0);
    }
}
