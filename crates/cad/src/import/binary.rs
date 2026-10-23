//! Binary DXF: the same group codes and values as an ASCII DXF, stored as bytes. Read into the
//! ASCII form so the rest of the import is shared. Layout from Autodesk's public DXF reference
//! ("About Binary DXF Files":
//! <https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-FC1C3C69-DBC2-49E4-893A-000D6538C0FE.htm>;
//! value types by group code, "Group Code Value Types Reference":
//! <https://help.autodesk.com/cloudhelp/2021/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm>):
//! the 22-byte sentinel, then each group code (2 bytes, least significant first; 1 byte before
//! Release 13, where 255 escapes a 2-byte code) and its value: strings ending in a NUL, doubles in
//! 8 bytes, integers in 2, 4 or 8, booleans in 1, and binary chunks as a length byte and the data.

use std::fmt::Write as _;

use super::reader::BINARY_SENTINEL;

/// How a value of a group code is stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    Text,
    Double,
    I16,
    I32,
    I64,
    Bool,
    Chunk,
}

/// The value type of group code `code` (the reference's table); `small` reads 280–289 as one
/// byte (the older reference's 8-bit integers) instead of two.
pub(super) fn kind(code: i32, small: bool) -> Option<Kind> {
    Some(match code {
        0..=9 | 100 | 102 | 105 | 300..=309 | 320..=369 | 390..=399 | 410..=419 | 430..=439 | 470..=481 | 999 | 1000..=1003 | 1005..=1009 => {
            Kind::Text
        }
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Kind::Double,
        280..=289 if small => Kind::Bool,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => Kind::I16,
        90..=99 | 420..=429 | 440..=459 | 1071 => Kind::I32,
        160..=169 => Kind::I64,
        290..=299 => Kind::Bool,
        310..=319 | 1004 => Kind::Chunk,
        _ => return None,
    })
}

/// Is `bytes` a binary DXF?
pub(crate) fn is_binary(bytes: &[u8]) -> bool {
    bytes.starts_with(BINARY_SENTINEL)
}

/// The drawing in `bytes` (a binary DXF, sentinel included) as ASCII DXF text.
pub(crate) fn to_text(bytes: &[u8]) -> Result<String, String> {
    let body = bytes.get(BINARY_SENTINEL.len()..).unwrap_or_default();
    // The first group is `0 SECTION`: a 2-byte code (two zero bytes) from Release 13 on, else one.
    let wide = body.get(1) == Some(&0);
    // 280–289 were 8-bit before; read them as 16-bit (the current reference) unless that loses
    // the way through the file
    match read(body, wide, false) {
        Ok((t, true)) => Ok(t),
        first => match (read(body, wide, true), first) {
            (Ok((t, true)), _) | (_, Ok((t, _))) => Ok(t),
            (Ok((t, false)), Err(_)) => Ok(t),
            (Err(_), Err(e)) => Err(e),
        },
    }
}

/// The text, and whether `0 EOF` was reached.
fn read(body: &[u8], wide: bool, small: bool) -> Result<(String, bool), String> {
    let damaged = |at: usize| format!("the binary DXF is damaged at byte {}", at + BINARY_SENTINEL.len());
    let mut out = String::with_capacity(body.len().saturating_mul(2));
    let mut i = 0usize;
    let take = |i: &mut usize, n: usize| -> Result<&[u8], String> {
        let s = body.get(*i..i.saturating_add(n)).ok_or_else(|| damaged(*i))?;
        *i += n;
        Ok(s)
    };
    while i < body.len() {
        let at = i;
        let code = if wide {
            let b = take(&mut i, 2)?;
            i32::from(i16::from_le_bytes([b[0], b[1]]))
        } else {
            match take(&mut i, 1)?[0] {
                255 => {
                    let b = take(&mut i, 2)?;
                    i32::from(i16::from_le_bytes([b[0], b[1]]))
                }
                c => i32::from(c),
            }
        };
        let k = kind(code, small).ok_or_else(|| damaged(at))?;
        let _ = writeln!(out, "{code}");
        match k {
            Kind::Text => {
                let rest = body.get(i..).ok_or_else(|| damaged(i))?;
                let end = rest.iter().position(|b| *b == 0).ok_or_else(|| damaged(i))?;
                let s = take(&mut i, end + 1)?;
                let s = s.get(..end).unwrap_or_default();
                // a line break in a value would break the pairs: none are written
                let text = super::reader::decode(s);
                out.extend(text.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }));
                if code == 0 && text == "EOF" {
                    out.push('\n');
                    return Ok((out, true));
                }
            }
            Kind::Double => {
                let b = take(&mut i, 8)?;
                let v = f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                // a value that isn't a number would end the reading of the text: kept as 0
                let _ = write!(out, "{}", if v.is_finite() { v } else { 0.0 });
            }
            Kind::I16 => {
                let b = take(&mut i, 2)?;
                let _ = write!(out, "{}", i16::from_le_bytes([b[0], b[1]]));
            }
            Kind::I32 => {
                let b = take(&mut i, 4)?;
                let _ = write!(out, "{}", i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
            Kind::I64 => {
                let b = take(&mut i, 8)?;
                let _ = write!(out, "{}", i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
            }
            Kind::Bool => {
                let _ = write!(out, "{}", take(&mut i, 1)?[0]);
            }
            Kind::Chunk => {
                let n = usize::from(take(&mut i, 1)?[0]);
                for b in take(&mut i, n)? {
                    let _ = write!(out, "{b:02X}");
                }
            }
        }
        out.push('\n');
    }
    // no `0 EOF`: what was read is kept, as a cut-off ASCII file's is
    if out.is_empty() { Err(damaged(0)) } else { Ok((out, false)) }
}
