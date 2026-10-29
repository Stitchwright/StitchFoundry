//! # sf-photo
//!
//! Cutting a photo's subject from its background before it is digitized for embroidery.
//! Everything works on plain in-memory buffers (no tiled surfaces):
//!
//! * [`segment`]: Select Subject (saliency-seeded GrabCut), GrabCut from a box, a click or a
//!   trimap of hints, plus mask clean-up ([`segment::clean_mask`],
//!   [`segment::fill_holes_by_model`], [`segment::keep_seeded`]). Pixels are read through a
//!   [`segment::Sampler`] (for example [`segment::ImageSampler`] over an [`segment::RgbImage`]);
//!   results are 8-bit coverage [`region::Region`]s.
//! * [`matting`]: edge refinement (guided filter, Refine Edge Brush, feather, contrast, shift
//!   edge), grey morphology and colour decontamination.
//! * [`distance`]: the exact Euclidean distance transform of a binary mask.
//! * [`magnetic`]: live-wire (magnetic lasso) paths and polyline simplification.
//!
//! Long operations take an [`Interrupt`] for cancellation and progress.
//!
//! The algorithms are vendored under MIT OR Apache-2.0; each file names its upstream source.

#![forbid(unsafe_code)]
pub mod distance;
pub mod geom;
pub mod interrupt;
pub mod magnetic;
pub mod matting;
pub mod region;
pub mod segment;

mod photo_util;

pub use geom::Rect;
pub use interrupt::{Cancelled, Interrupt};
