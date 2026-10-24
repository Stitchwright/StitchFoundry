//! The sew-out animation as an MP4 video (VIDEO-02): each frame encoded as H.264 by `less-avc` (a
//! pure Rust encoder, MIT OR Apache-2.0; Baseline profile, every frame a key frame) and put in an
//! MP4 file written here from the ISO base media file format (ISO/IEC 14496-12) and its AVC
//! file format (ISO/IEC 14496-15): `ftyp`, `mdat` with the frames, and `moov` with one video
//! track. It plays in browsers, phones and on social sites, which a GIF often doesn't.

use less_avc::ycbcr_image::{DataPlane, Planes, YCbCrImage};

/// A box: its size, its four-letter type, its contents.
fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// A full box: a box whose contents start with a version and flags.
fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 4);
    b.push(version);
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn u16b(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
fn u32b(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// The identity matrix of a movie or track header.
const MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

/// The H.264 level for a frame of `w × h` pixels at `fps` frames a second, with every frame a key
/// frame of uncompressed macroblocks (about 12 bits a pixel): written into the sequence parameter
/// set, whose encoder always says level 1.0 (for 176 × 144 video), which strict players refuse.
fn level(w: u32, h: u32, fps: f64) -> u8 {
    let mbs = f64::from(w.div_ceil(16) * h.div_ceil(16));
    let kbps = f64::from(w) * f64::from(h) * 12.0 * fps / 1000.0;
    // (level, frame size in macroblocks, macroblocks a second, Baseline bit rate in kbit/s), Table A-1
    let table: [(u8, f64, f64, f64); 9] = [
        (30, 1620.0, 40_500.0, 10_000.0),
        (31, 3600.0, 108_000.0, 14_000.0),
        (32, 5120.0, 216_000.0, 20_000.0),
        (40, 8192.0, 245_760.0, 20_000.0),
        (41, 8192.0, 245_760.0, 50_000.0),
        (42, 8704.0, 522_240.0, 50_000.0),
        (50, 22_080.0, 589_824.0, 135_000.0),
        (51, 36_864.0, 983_040.0, 240_000.0),
        (52, 36_864.0, 2_073_600.0, 240_000.0),
    ];
    table.iter().find(|(_, fs, mbps, br)| mbs <= *fs && mbs * fps <= *mbps && kbps <= *br).map_or(52, |t| t.0)
}

/// An RGBA picture as 4:2:0 Y′CbCr (BT.601, full range, which the encoder's stream says it is), its size a whole number of 16-pixel
/// macroblocks (the caller pads it).
fn ycbcr(img: &image::RgbaImage) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px = |x: usize, y: usize| -> [f64; 3] {
        let p = img.get_pixel(x as u32, y as u32).0;
        // over white where it is transparent
        let a = f64::from(p[3]) / 255.0;
        [0, 1, 2].map(|i| f64::from(p[i]) * a + 255.0 * (1.0 - a))
    };
    let mut yp = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = px(x, y);
            yp[y * w + x] = (0.299 * r + 0.587 * g + 0.114 * b).round().clamp(0.0, 255.0) as u8;
        }
    }
    let (cw, ch) = (w / 2, h / 2);
    let (mut cb, mut cr) = (vec![128u8; cw * ch], vec![128u8; cw * ch]);
    for y in 0..ch {
        for x in 0..cw {
            let mut s = [0.0; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px(2 * x + dx, 2 * y + dy);
                for i in 0..3 {
                    s[i] += p[i] / 4.0;
                }
            }
            let [r, g, b] = s;
            cb[y * cw + x] = (128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b).round().clamp(0.0, 255.0) as u8;
            cr[y * cw + x] = (128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b).round().clamp(0.0, 255.0) as u8;
        }
    }
    (yp, cb, cr)
}

/// An 8-bit plane of a picture.
fn plane(data: &[u8], stride: usize) -> DataPlane<'_> {
    DataPlane { data, stride, bit_depth: less_avc::BitDepth::Depth8 }
}

/// Encodes frames one at a time and writes them as an MP4 file at the end.
pub(super) struct Mp4 {
    enc: Option<less_avc::LessEncoder>,
    sps: Vec<u8>,
    pps: Vec<u8>,
    /// Each frame's NAL unit (a length before it, as MP4 keeps them) and how long it shows (ms).
    samples: Vec<(Vec<u8>, u32)>,
    width: u32,
    height: u32,
    fps: f64,
}

impl Mp4 {
    pub(super) fn new(fps: f64) -> Self {
        Self { enc: None, sps: Vec::new(), pps: Vec::new(), samples: Vec::new(), width: 0, height: 0, fps }
    }

    /// Adds a frame shown for `ms` milliseconds; every frame is the size of the first.
    pub(super) fn frame(&mut self, img: &image::RgbaImage, ms: u32) -> Result<(), String> {
        let (w, h) = (img.width(), img.height());
        if w == 0 || h == 0 || w % 16 != 0 || h % 16 != 0 || w > 4096 || h > 4096 {
            return Err(format!("a video frame is {w} × {h}: it must be whole macroblocks, at most 4096 a side"));
        }
        if self.enc.is_some() && (w, h) != (self.width, self.height) {
            return Err("the video's frames changed size".into());
        }
        let (yp, cb, cr) = ycbcr(img);
        let (wu, cw) = (w as usize, (w / 2) as usize);
        let pic = YCbCrImage { planes: Planes::YCbCr((plane(&yp, wu), plane(&cb, cw), plane(&cr, cw))), width: w, height: h };
        let nal = match &mut self.enc {
            Some(enc) => enc.encode(&pic).map_err(|e| format!("H.264 encoding failed: {e:?}"))?,
            None => {
                let (first, enc) = less_avc::LessEncoder::new(&pic).map_err(|e| format!("H.264 encoding failed: {e:?}"))?;
                self.sps = first.sps.to_nal_unit();
                // the level for the video's real size and rate (the byte after profile and constraints)
                if let Some(l) = self.sps.get_mut(3) {
                    *l = level(w, h, self.fps);
                }
                self.pps = first.pps.to_nal_unit();
                self.enc = Some(enc);
                (self.width, self.height) = (w, h);
                first.frame
            }
        };
        let unit = nal.to_nal_unit();
        let mut sample = Vec::with_capacity(unit.len() + 4);
        sample.extend_from_slice(&u32b(unit.len() as u32));
        sample.extend_from_slice(&unit);
        self.samples.push((sample, ms.max(1)));
        Ok(())
    }

    /// The MP4 file.
    pub(super) fn finish(self) -> Result<Vec<u8>, String> {
        if self.samples.is_empty() || self.sps.len() < 4 {
            return Err("the video has no frames".into());
        }
        let total: u64 = self.samples.iter().map(|s| u64::from(s.1)).sum();
        let mdat_len: u64 = self.samples.iter().map(|s| s.0.len() as u64).sum();
        if mdat_len + 1_000_000 > u64::from(u32::MAX) {
            return Err("the video would be larger than 4 GB: lower the size or frames".into());
        }
        let duration = u32::try_from(total).unwrap_or(u32::MAX);
        let (w, h) = (self.width, self.height);
        let n = self.samples.len() as u32;
        let ftyp = bx(b"ftyp", &[&b"isom"[..], &u32b(0x200), b"isom", b"iso2", b"avc1", b"mp41"].concat());
        // timescale: milliseconds
        let mvhd = full(
            b"mvhd",
            0,
            0,
            &[
                &u32b(0)[..],
                &u32b(0),
                &u32b(1000),
                &u32b(duration),
                &u32b(0x0001_0000),
                &u16b(0x0100),
                &[0u8; 10],
                &MATRIX.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>(),
                &[0u8; 24],
                &u32b(2),
            ]
            .concat(),
        );
        let tkhd = full(
            b"tkhd",
            0,
            3,
            &[
                &u32b(0)[..],
                &u32b(0),
                &u32b(1),
                &u32b(0),
                &u32b(duration),
                &[0u8; 8],
                &u16b(0),
                &u16b(0),
                &u16b(0),
                &u16b(0),
                &MATRIX.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>(),
                &u32b(w << 16),
                &u32b(h << 16),
            ]
            .concat(),
        );
        // language "und", packed as three 5-bit letters
        let mdhd = full(b"mdhd", 0, 0, &[&u32b(0)[..], &u32b(0), &u32b(1000), &u32b(duration), &u16b(0x55C4), &u16b(0)].concat());
        let hdlr = full(b"hdlr", 0, 0, &[&u32b(0)[..], b"vide", &[0u8; 12], b"StitchFoundry video\0"].concat());
        let vmhd = full(b"vmhd", 0, 1, &[0u8; 8]);
        let dinf = bx(b"dinf", &full(b"dref", 0, 0, &[&u32b(1)[..], &full(b"url ", 0, 1, &[])].concat()));
        // the decoder configuration: the parameter sets, lengths written in four bytes
        let avcc = bx(
            b"avcC",
            &[
                &[1u8, self.sps[1], self.sps[2], self.sps[3], 0xFF, 0xE1][..],
                &u16b(self.sps.len() as u16),
                &self.sps,
                &[1u8],
                &u16b(self.pps.len() as u16),
                &self.pps,
            ]
            .concat(),
        );
        let avc1 = bx(
            b"avc1",
            &[
                &[0u8; 6][..],
                &u16b(1),
                &[0u8; 16],
                &u16b(w as u16),
                &u16b(h as u16),
                &u32b(0x0048_0000),
                &u32b(0x0048_0000),
                &u32b(0),
                &u16b(1),
                &[0u8; 32],
                &u16b(0x0018),
                &0xFFFFu16.to_be_bytes(),
                &avcc,
            ]
            .concat(),
        );
        let stsd = full(b"stsd", 0, 0, &[&u32b(1)[..], &avc1].concat());
        // how long each frame shows, runs of equal ones together
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for (_, ms) in &self.samples {
            match runs.last_mut() {
                Some((count, d)) if d == ms => *count += 1,
                _ => runs.push((1, *ms)),
            }
        }
        let stts =
            full(b"stts", 0, 0, &[u32b(runs.len() as u32).to_vec(), runs.iter().flat_map(|(c, d)| [u32b(*c), u32b(*d)].concat()).collect()].concat());
        // one chunk holding every frame
        let stsc = full(b"stsc", 0, 0, &[&u32b(1)[..], &u32b(1), &u32b(n), &u32b(1)].concat());
        let stsz =
            full(b"stsz", 0, 0, &[u32b(0).to_vec(), u32b(n).to_vec(), self.samples.iter().flat_map(|s| u32b(s.0.len() as u32)).collect()].concat());
        // the chunk's offset, known once moov's size is: moov is written first with a stand-in
        let build = |offset: u32| {
            let stco = full(b"stco", 0, 0, &[&u32b(1)[..], &u32b(offset)].concat());
            let stbl = bx(b"stbl", &[&stsd[..], &stts, &stsc, &stsz, &stco].concat());
            let minf = bx(b"minf", &[&vmhd[..], &dinf, &stbl].concat());
            let mdia = bx(b"mdia", &[&mdhd[..], &hdlr, &minf].concat());
            let trak = bx(b"trak", &[&tkhd[..], &mdia].concat());
            bx(b"moov", &[&mvhd[..], &trak].concat())
        };
        // moov before mdat, so a video plays while it downloads
        let moov_len = build(0).len();
        let offset = u32::try_from(ftyp.len() + moov_len + 8).map_err(|_| "the video is too large".to_string())?;
        let moov = build(offset);
        let mut out = Vec::with_capacity(offset as usize + mdat_len as usize);
        out.extend_from_slice(&ftyp);
        out.extend_from_slice(&moov);
        out.extend_from_slice(&u32b((mdat_len + 8) as u32));
        out.extend_from_slice(b"mdat");
        for (s, _) in &self.samples {
            out.extend_from_slice(s);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_make_an_mp4_with_one_video_track() {
        let mut v = Mp4::new(15.0);
        for k in 0..3u8 {
            let img = image::RgbaImage::from_fn(64, 48, |x, _| image::Rgba([k * 80, (x * 4) as u8, 40, 255]));
            v.frame(&img, if k == 2 { 2000 } else { 67 }).unwrap();
        }
        assert!(v.frame(&image::RgbaImage::new(32, 32), 67).is_err(), "frames keep their size");
        assert!(Mp4::new(15.0).frame(&image::RgbaImage::new(30, 32), 67).is_err(), "whole macroblocks");
        let mp4 = v.finish().unwrap();
        assert_eq!(&mp4[4..8], b"ftyp");
        for kind in [b"moov", b"mvhd", b"trak", b"avc1", b"avcC", b"stts", b"stco", b"mdat"] {
            assert!(mp4.windows(4).any(|w| w == kind), "{}", String::from_utf8_lossy(kind));
        }
        // the chunk offset points at the first frame: its length, then a slice NAL unit (type 5)
        let at = mp4.windows(4).position(|w| w == b"stco").unwrap() + 12;
        let off = u32::from_be_bytes(mp4[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(mp4[off + 4] & 0x1F, 5, "an IDR slice where the chunk starts");
        // the level fits the video, not level 1.0
        assert!(level(400, 400, 15.0) >= 30 && level(1920, 1088, 30.0) >= 50);
        assert!(Mp4::new(15.0).finish().is_err());
    }
}
