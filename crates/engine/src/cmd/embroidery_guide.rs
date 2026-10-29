//! Make a Design (E3): the guide's one step that starts a design — words, a shape, an appliqué
//! piece, a picture turned into stitches, or a sample — in the hoop chosen (else My Machine's).
//! The guide's other steps (colours, watching it sew, saving) are ordinary commands.

use serde_json::{Value, json};

use super::embroidery_photo::{Picture, looks_like_photo};
use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.makeDesign",
        "Make a Design",
        [],
        None,
        "{what: words|shape|applique|picture|sample|ith|quilt, project? (ith: embroidery.ith.projects), pieceSize?: small|regular|large (ith), edge?: square|rounded|scalloped|round (ith), width?, height?, pattern?, spacing? (quilt: embroidery.quiltBlock), hoop?: a hoop's name (My Machine's first), text?, height?: mm (words), shape?: Heart|Star|… (shape, applique), size?: mm (shape, applique), path? | name? | dataBase64? (picture), colors?: 2-12 (picture, 6; 8 for a photo), photo?: bool (picture: whether it is a photo, guessed from its colours; a photo's colours are made flat before it is digitized), removeBackground?: bool (picture, a photo: true; its subject cut out of its background first, false: the whole photo stitched), border?: mm (picture: 0, none; else a filled band this wide round it, as embroidery.addBorder makes), sample?: an id from embroidery.samples} a new design made in one step: words or a shape in the middle of the hoop (words too long for it made smaller to fit), an appliqué piece, a picture turned into stitches at most four fifths of the hoop, or a sample design, or an in-the-hoop project (embroidery.ith.make) → {what, ids, stitches, colors, widthMm, heightMm, fits, time (h:mm:ss sewing), photo (picture), madeSmaller (words made smaller to fit: %)}",
        always,
        make_design
    )]
}

fn make_design(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.makeDesign";
    let what = str_param(p, "what").ok_or_else(|| bad(CMD, "missing `what`: words, shape, applique, picture or sample"))?;
    let hoop = || match p.get("hoop") {
        Some(h) if !h.is_null() => json!({ "hoop": h }),
        _ => json!({}),
    };
    let mut photo = None;
    // words made smaller to fit the hoop: to what percent
    let mut smaller: Option<f64> = None;
    let ids: Vec<Value> = match what {
        "sample" => {
            let id = str_param(p, "sample").ok_or_else(|| bad(CMD, "missing `sample`: see embroidery.samples"))?;
            s.execute("embroidery.sample", &json!({ "id": id }))?;
            Vec::new()
        }
        "words" => {
            let made = s.execute("embroidery.newDesign", &hoop())?;
            let mut q = json!({ "text": p.get("text").cloned().unwrap_or(Value::Null) });
            if let Some(h) = p.get("height") {
                q["height"] = h.clone();
            }
            let id = s.execute("embroidery.addWords", &q)?["id"].clone();
            // words longer than the hoop is wide: made smaller, to nine tenths of the hoop
            let (hw, hh) = (made["width"].as_f64().unwrap_or(0.0), made["height"].as_f64().unwrap_or(0.0));
            let info = s.execute("embroidery.info", &json!({}))?;
            let (w, h) = (info["widthMm"].as_f64().unwrap_or(0.0), info["heightMm"].as_f64().unwrap_or(0.0));
            let k = if w > 0.0 && h > 0.0 { (0.9 * hw / w).min(0.9 * hh / h) } else { 1.0 };
            if k.is_finite() && k > 0.0 && k < 1.0 {
                s.execute("object.scale", &json!({ "ids": [id.clone()], "sx": k * 100.0 }))?;
                smaller = Some(k * 100.0);
            }
            vec![id]
        }
        "shape" | "applique" => {
            s.execute("embroidery.newDesign", &hoop())?;
            let mut q = json!({ "name": p.get("shape").cloned().unwrap_or(Value::Null), "applique": what == "applique" });
            if let Some(v) = p.get("size") {
                q["size"] = v.clone();
            }
            vec![s.execute("embroidery.addShape", &q)?["id"].clone()]
        }
        "picture" => {
            let mut place = json!({});
            match (p.get("path"), p.get("name"), p.get("dataBase64")) {
                (Some(path), _, _) => place["path"] = path.clone(),
                (None, Some(name), Some(data)) => {
                    place["name"] = name.clone();
                    place["dataBase64"] = data.clone();
                }
                _ => return Err(bad(CMD, "missing `path` (or `name` and `dataBase64`): the picture")),
            }
            let colors = match p.get("colors") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().filter(|n| (2..=12).contains(n)).ok_or_else(|| bad(CMD, "`colors` is 2 to 12"))?),
            };
            let given = match p.get("photo") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_bool().ok_or_else(|| bad(CMD, "`photo` is true or false"))?),
            };
            let cut_out = match p.get("removeBackground") {
                None | Some(Value::Null) => true,
                Some(v) => v.as_bool().ok_or_else(|| bad(CMD, "`removeBackground` is true or false"))?,
            };
            let border = match p.get("border") {
                None | Some(Value::Null) => 0.0,
                Some(v) => {
                    v.as_f64().filter(|b| *b == 0.0 || (1.0..=30.0).contains(b)).ok_or_else(|| bad(CMD, "`border` is 0 (none) or 1 to 30 mm"))?
                }
            };
            let made = s.execute("embroidery.newDesign", &hoop())?;
            let (w, h) = (made["width"].as_f64().unwrap_or(100.0), made["height"].as_f64().unwrap_or(100.0));
            s.execute("file.place", &place)?;
            // the placed picture is selected
            let is_photo = match given {
                Some(v) => v,
                None => Picture::get(s, &json!({}), CMD).is_ok_and(|pic| looks_like_photo(&pic.raster)),
            };
            photo = Some(is_photo);
            // four fifths of the hoop's shorter side, so it fits whichever way the picture is, the
            // border round it included
            let size = (w.min(h) * 0.8 - 2.0 * border).clamp(10.0, 400.0);
            let r = if is_photo {
                let colors = colors.unwrap_or(8);
                if cut_out {
                    // A photo with no subject to cut out (a landscape, a texture) is digitized whole.
                    let _ = s.execute("embroidery.image.removeBackground", &json!({ "replace": true }));
                }
                s.execute("embroidery.image.prepare", &json!({ "colors": colors, "smooth": 4, "clean": 3, "replace": true }))?;
                // a photo's many small patches: fewer, larger shapes than for a drawing
                // without the cut-out the whole photo is sewn, its background too
                let q = json!({ "colors": colors, "size": size, "keepImage": false, "detail": 35, "noise": 30, "minArea": 4, "removeBackground": cut_out, "border": border });
                s.execute("embroidery.autoDigitize", &q)?
            } else {
                s.execute("embroidery.autoDigitize", &json!({ "colors": colors.unwrap_or(6), "size": size, "keepImage": false, "border": border }))?
            };
            vec![r["id"].clone()]
        }
        "ith" => {
            let mut q = json!({ "project": p.get("project").cloned().unwrap_or(Value::Null) });
            for k in ["hoop", "text", "shape", "height", "ribbon", "vinyl", "button", "edge"] {
                if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
                    q[k] = v.clone();
                }
            }
            // `size` is a shape's size in mm here: the project's is `pieceSize`
            if let Some(v) = p.get("pieceSize").filter(|v| !v.is_null()) {
                q["size"] = v.clone();
            }
            s.execute("embroidery.ith.make", &q)?;
            Vec::new()
        }
        "quilt" => {
            let mut q = json!({});
            for k in ["width", "height", "unit", "pattern", "spacing", "stitch", "color", "hoop", "corner", "text", "shape", "applique", "clearance"]
            {
                if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
                    q[k] = v.clone();
                }
            }
            s.execute("embroidery.quiltBlock", &q)?;
            Vec::new()
        }
        other => return Err(bad(CMD, format!("`what` is words, shape, applique, picture, sample, ith or quilt, not `{other}`"))),
    };
    let info = s.execute("embroidery.info", &json!({}))?;
    let mut out = json!({
        "what": what,
        "ids": ids,
        "stitches": info["stitches"],
        "colors": info["colors"],
        "widthMm": info["widthMm"],
        "heightMm": info["heightMm"],
        "fits": info["fits"],
        "time": info["time"],
    });
    if let Some(v) = photo {
        out["photo"] = json!(v);
    }
    if let Some(pct) = smaller {
        out["madeSmaller"] = json!(pct.round());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_base64() -> String {
        // two colour blocks on white: something to turn into stitches
        let mut png = vec![];
        image::RgbaImage::from_fn(120, 80, |x, y| {
            if (20..60).contains(&x) && (20..60).contains(&y) {
                image::Rgba([200, 30, 40, 255])
            } else if (70..110).contains(&x) && (10..70).contains(&y) {
                image::Rgba([20, 60, 160, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        })
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        vectorcraft_format::base64_encode(&png)
    }

    /// QA-01: words too long for the hoop are made smaller to fit, and the result says so;
    /// words that fit keep their size.
    #[test]
    fn words_too_long_for_the_hoop_are_made_smaller_to_fit() {
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": null})).unwrap();
        let r = s.execute("embroidery.makeDesign", &json!({"what": "words", "text": "Grandma", "height": 25})).unwrap();
        assert_eq!(r["fits"], true, "{r}");
        let pct = r["madeSmaller"].as_f64().unwrap();
        assert!((40.0..100.0).contains(&pct), "{r}");
        assert!(r["widthMm"].as_f64().unwrap() <= 130.0 * 0.9 + 0.5, "nine tenths of the 5×7's width: {r}");
        let r = s.execute("embroidery.makeDesign", &json!({"what": "words", "text": "Al", "height": 25})).unwrap();
        assert!(r.get("madeSmaller").is_none(), "{r}");
        assert!((r["heightMm"].as_f64().unwrap() - 25.0).abs() < 6.0, "{r}");
    }

    /// E3: each kind of design the guide offers is made in one step, ready to sew in the hoop.
    #[test]
    fn the_guide_makes_each_kind_of_design_ready_to_sew() {
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
        for q in [
            json!({"what": "words", "text": "Grace", "height": 20}),
            json!({"what": "shape", "shape": "Star"}),
            json!({"what": "applique", "shape": "Heart", "hoop": "Husqvarna 120 × 120"}),
            json!({"what": "picture", "name": "blocks.png", "dataBase64": png_base64(), "colors": 3}),
            json!({"what": "sample", "sample": "heart-patch"}),
            json!({"what": "ith", "project": "coaster", "text": "Tea"}),
            json!({"what": "quilt", "width": 4, "height": 4, "pattern": "stipple"}),
        ] {
            let r = s.execute("embroidery.makeDesign", &q).unwrap_or_else(|e| panic!("{q}: {e}"));
            assert!(r["stitches"].as_u64().unwrap() > 100, "{q}: {r}");
            assert_eq!(r["fits"], true, "{q}: {r}");
            assert!(!r["colors"].as_array().unwrap().is_empty(), "{q}: {r}");
        }
        // words in My Machine's first hoop, in its format
        let r = s.execute("embroidery.makeDesign", &json!({"what": "words", "text": "Al"})).unwrap();
        assert_eq!(r["what"], "words");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["machineFormat"], "vp3", "{info}");
        for bad in [json!({}), json!({"what": "quilt"}), json!({"what": "picture"}), json!({"what": "sample"}), json!({"what": "words", "text": " "})]
        {
            assert!(s.execute("embroidery.makeDesign", &bad).is_err(), "{bad}");
        }
    }

    /// PH-01: a photo is told from a drawing by its colours, even a drawing saved as a JPEG, and
    /// its subject is cut out of the background before it is digitized.
    #[test]
    fn a_photo_is_cut_out_and_a_drawing_is_not() {
        let encode = |img: image::RgbImage, format| {
            let mut out = vec![];
            img.write_to(&mut std::io::Cursor::new(&mut out), format).unwrap();
            vectorcraft_format::base64_encode(&out)
        };
        // a shaded orange ball in front of a green-to-blue background, with grain in each channel
        let photo = image::RgbImage::from_fn(240, 200, |x, y| {
            let grain = [(x * 7919 + y * 104_729) % 51, (x * 104_729 + y * 7919) % 47, (x * 31 + y * 17 * x) % 53].map(|g| f64::from(g) - 25.0);
            let (fx, fy) = (f64::from(x), f64::from(y));
            let (dx, dy) = (fx - 120.0, fy - 100.0);
            let c = if dx.hypot(dy) < 60.0 {
                let shade = 1.1 - 0.8 * (dx + dy + 85.0) / 170.0;
                [235.0 * shade, 140.0 * shade, 40.0 * shade]
            } else {
                [40.0 + fx * 0.8, 180.0 - fy * 0.6, 60.0 + (fx + fy) * 0.4]
            };
            image::Rgb([0, 1, 2].map(|i| (c[i] + grain[i]).clamp(0.0, 255.0) as u8))
        });
        // flat blocks, JPEG compressed
        let drawing = image::RgbImage::from_fn(240, 200, |x, y| {
            if (30..110).contains(&x) && (40..160).contains(&y) {
                image::Rgb([200, 30, 40])
            } else if (130..210).contains(&x) && (40..160).contains(&y) {
                image::Rgb([20, 60, 160])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        // a shaded orange ball on a soft blue gradient: a photo whose subject cuts out cleanly
        let ball = image::RgbImage::from_fn(240, 200, |x, y| {
            let (fx, fy) = (f64::from(x), f64::from(y));
            let (dx, dy) = (fx - 120.0, fy - 100.0);
            let c = if dx.hypot(dy) < 60.0 {
                let shade = 1.1 - 0.6 * (dx + dy + 85.0) / 170.0;
                [230.0 * shade, 120.0 * shade, 30.0 * shade]
            } else {
                [200.0 + fx * 0.1, 215.0 - fy * 0.05, 235.0]
            };
            image::Rgb(c.map(|v| v.clamp(0.0, 255.0) as u8))
        });
        let mut s = Session::new();
        let make = |s: &mut Session, data: &str, extra: Value| {
            let mut q = json!({"what": "picture", "name": "pic.jpg", "dataBase64": data, "hoop": "Brother 130 × 180 (5×7)"});
            if let (Some(q), Some(e)) = (q.as_object_mut(), extra.as_object()) {
                q.extend(e.clone());
            }
            s.execute("embroidery.makeDesign", &q).unwrap()
        };
        let size = |r: &Value| (r["widthMm"].as_f64().unwrap(), r["heightMm"].as_f64().unwrap());
        // told from a drawing by its colours
        let photo = encode(photo, image::ImageFormat::Jpeg);
        let r = make(&mut s, &photo, json!({"removeBackground": false}));
        assert_eq!(r["photo"], true, "{r}");
        assert!(r["stitches"].as_u64().unwrap() > 100 && r["fits"] == true, "{r}");
        assert_eq!(make(&mut s, &encode(drawing, image::ImageFormat::Jpeg), json!({}))["photo"], false);
        assert_eq!(make(&mut s, &photo, json!({"photo": false}))["photo"], false, "told otherwise");
        // the background cut away: the design is the round ball (it is scaled to the size asked, so
        // its shape tells), not the 6:5 picture
        let ball = encode(ball, image::ImageFormat::Png);
        let (w, h) = size(&make(&mut s, &ball, json!({"photo": true})));
        assert!((w / h - 1.0).abs() < 0.08, "{w} × {h} mm");
        // with Remove Background off the whole photo is stitched, the picture's shape
        let (w2, h2) = size(&make(&mut s, &ball, json!({"photo": true, "removeBackground": false})));
        assert!((w2 / h2 - 1.2).abs() < 0.08, "{w2} × {h2} mm");
        assert!(
            s.execute("embroidery.makeDesign", &json!({"what": "picture", "dataBase64": photo, "name": "p.jpg", "removeBackground": 1})).is_err()
        );
        let mut q = json!({"what": "picture", "name": "pic.jpg", "dataBase64": photo, "photo": "yes"});
        assert!(s.execute("embroidery.makeDesign", &q).is_err());
        q["photo"] = json!(null);
        q["colors"] = json!(20);
        assert!(s.execute("embroidery.makeDesign", &q).is_err());
    }
}
