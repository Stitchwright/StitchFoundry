// Adapted from PhotoCraft (https://github.com/storytold/photocraft, commit 04d0212, crates/algo/src/photo_util.rs), MIT OR Apache-2.0.
//! Small helpers shared by the selection and segmentation modules: parallel row loops (rayon on
//! native targets, sequential on wasm32).

/// Runs `f(y, row)` for every row of a `w × h × ch` buffer, in parallel on native targets.
pub(crate) fn par_rows<T: Send + Sync>(buf: &mut [T], w: usize, ch: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    let stride = (w * ch).max(1);
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        buf.par_chunks_mut(stride).enumerate().for_each(|(y, row)| f(y, row));
    }
    #[cfg(target_arch = "wasm32")]
    buf.chunks_mut(stride).enumerate().for_each(|(y, row)| f(y, row));
}
