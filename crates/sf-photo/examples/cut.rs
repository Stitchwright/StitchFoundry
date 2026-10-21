//! Smoke test: cut a photo's subject from its background.
//!
//! `cargo run --release --example cut -- IN.rgba WIDTH HEIGHT OUT.alpha`
//!
//! Reads raw straight RGBA8 (`WIDTH × HEIGHT × 4` bytes), runs Select Subject over the whole
//! image (no box) and writes the subject's coverage, one byte per pixel (0 background, 255
//! subject), over the whole image.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use sf_photo::Rect;
use sf_photo::segment::{ImageSampler, RgbImage, subject::select_subject};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        panic!("usage: cut IN.rgba WIDTH HEIGHT OUT.alpha");
    }
    let (w, h): (usize, usize) = (args[2].parse().unwrap(), args[3].parse().unwrap());
    let bytes = std::fs::read(&args[1]).unwrap();
    assert_eq!(bytes.len(), w * h * 4, "file size is not WIDTH × HEIGHT × 4");
    let start = Instant::now();
    let rgba: Vec<[f32; 4]> = bytes.chunks_exact(4).map(|p| [0, 1, 2, 3].map(|c| f32::from(p[c]) / 255.0)).collect();
    let img = RgbImage::from_rgba(&rgba, w, h);
    let canvas = Rect::new(0, 0, w as i32, h as i32);
    let region = select_subject(&ImageSampler { img: &img, origin: (0, 0) }, canvas);
    let mut alpha = vec![0u8; w * h];
    if let Some(r) = &region {
        let bw = r.bbox.width() as usize;
        for (yy, row) in r.mask.chunks_exact(bw).enumerate() {
            let o = (r.bbox.y0 as usize + yy) * w + r.bbox.x0 as usize;
            alpha[o..o + bw].copy_from_slice(row);
        }
    }
    let secs = start.elapsed().as_secs_f64();
    std::fs::write(&args[4], &alpha).unwrap();
    let kept = alpha.iter().filter(|v| **v >= 128).count() as f64 / (w * h) as f64;
    let coverage = alpha.iter().map(|v| f64::from(*v)).sum::<f64>() / (255.0 * (w * h) as f64);
    match &region {
        Some(r) => println!("bbox {:?}", r.bbox),
        None => println!("no subject found"),
    }
    println!("time {secs:.3} s, kept {:.1} % of pixels (alpha >= 128), mean coverage {:.1} %", kept * 100.0, coverage * 100.0);
}
