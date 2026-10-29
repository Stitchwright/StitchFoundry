//! DWG drawings as DXF text, for the DXF import to read. The DWG is read by `acadrust`
//! (<https://github.com/hakanaktt/acadrust>, MPL-2.0, used unmodified as a dependency) in its
//! failsafe mode, which skips object records it can't decode, and written back out as ASCII DXF.
//! Nothing outside this crate sees acadrust's types. The reading runs under `catch_unwind` and
//! within size limits, as it parses untrusted files. A damaged file can still make it ask for an
//! impossible amount of memory, which aborts a process rather than panicking, or keep it busy: so
//! the apps run the conversion in a helper process (a copy of themselves, [`use_helper`] and
//! [`serve_if_helper`]) with a time limit, and a failed helper only fails the open. Native only: on
//! the web [`dwg_to_dxf`] answers an error.

#![forbid(unsafe_code)]
/// Does `bytes` start as a DWG drawing does (`AC10` and two digits: `AC1015` is 2000, `AC1032`
/// 2018)?
pub fn is_dwg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"AC10") && bytes.get(4..6).is_some_and(|v| v.iter().all(u8::is_ascii_digit))
}

/// The largest DWG read (bytes).
pub const MAX_DWG: usize = 256 << 20;
/// The largest DXF a DWG may turn into (bytes): a damaged file can make the reader produce far
/// more drawing than the file holds.
pub const MAX_DXF: usize = 1 << 30;

/// The longest a conversion may take in the helper process.
pub const HELPER_TIME: std::time::Duration = std::time::Duration::from_secs(120);
/// The environment variable that makes a process the helper ([`serve_if_helper`]).
pub const HELPER_ENV: &str = "SF_DWG_HELPER";

/// The program (and its arguments) to run as the helper process.
#[derive(Clone, Debug)]
pub struct Helper {
    pub exe: std::path::PathBuf,
    pub args: Vec<String>,
}

static HELPER: std::sync::OnceLock<Helper> = std::sync::OnceLock::new();

/// Convert DWG drawings in a helper process from now on: `helper` is run with [`HELPER_ENV`] set,
/// and calls [`serve_if_helper`] first thing. Without it, conversion runs in this process (tests).
/// Set once; later calls are ignored.
pub fn use_helper(helper: Helper) {
    // only the first helper counts: the app sets it once, at start
    let _ = HELPER.set(helper);
}

/// A conversion: the DXF text and notes on what couldn't be read.
pub type Converted = (Vec<u8>, Vec<String>);

/// The DXF text of a DWG drawing, and notes on what couldn't be read.
pub fn dwg_to_dxf(bytes: &[u8]) -> Result<(Vec<u8>, Vec<String>), String> {
    let Some(h) = HELPER.get() else {
        return native::dwg_to_dxf(bytes, MAX_DWG, MAX_DXF);
    };
    // the last drawing converted: opening one asks for it more than once (its layouts and
    // layers, the preview, the import)
    static LAST: std::sync::Mutex<Option<(Vec<u8>, Converted)>> = std::sync::Mutex::new(None);
    if let Ok(last) = LAST.lock()
        && let Some((key, answer)) = last.as_ref()
        && key.as_slice() == bytes
    {
        return Ok(answer.clone());
    }
    let answer = native::in_helper(h, bytes)?;
    if let Ok(mut last) = LAST.lock() {
        *last = Some((bytes.to_vec(), answer.clone()));
    }
    Ok(answer)
}

/// When this process was started as the helper ([`HELPER_ENV`] set): read a DWG on standard input,
/// write its conversion to standard output and exit. Otherwise return at once. The apps call it
/// first thing in `main`.
pub fn serve_if_helper() {
    if std::env::var_os(HELPER_ENV).is_some_and(|v| v == "1") {
        native::serve();
    }
}

/// The marker before the helper's answer (anything printed before it is skipped).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MARK: &[u8] = b"\0SF-DWG-ANSWER\0";

#[cfg(target_arch = "wasm32")]
mod native {
    const WEB: &str = "DWG drawings open in the desktop app; on the web, save the drawing as DXF in the CAD app and open that";

    pub fn dwg_to_dxf(_: &[u8], _: usize, _: usize) -> Result<(Vec<u8>, Vec<String>), String> {
        Err(WEB.into())
    }

    pub fn in_helper(_: &super::Helper, _: &[u8]) -> Result<(Vec<u8>, Vec<String>), String> {
        Err(WEB.into())
    }

    pub fn serve() {}
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::{Cursor, Read, Write};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    use super::{HELPER_ENV, HELPER_TIME, Helper, MARK, MAX_DWG, MAX_DXF};

    const FAILED: &str = "the DWG drawing couldn't be read (the reader failed on it)";

    /// Convert in the helper process: the DWG on its standard input, the answer on its output.
    pub fn in_helper(h: &Helper, bytes: &[u8]) -> Result<(Vec<u8>, Vec<String>), String> {
        if !super::is_dwg(bytes) {
            return Err("not a DWG drawing".into());
        }
        if bytes.len() > MAX_DWG {
            return Err(format!("the DWG drawing is larger than the {} MB that can be opened", MAX_DWG >> 20));
        }
        let mut child = Command::new(&h.exe)
            .args(&h.args)
            .env(HELPER_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("the DWG reader couldn't be started: {e}"))?;
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("the DWG reader couldn't be started".into());
        };
        let input = bytes.to_vec();
        // a helper that stops reading (it failed) ends the writing: nothing to report
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let reader = std::thread::spawn(move || {
            let mut out = Vec::new();
            // at most the largest answer: the reading ends there
            let _ = stdout.take((MAX_DXF + (1 << 20)) as u64).read_to_end(&mut out);
            out
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) if start.elapsed() > HELPER_TIME => {
                    // killed for taking too long: its exit is not needed
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => break None,
            }
        };
        let _ = writer.join();
        let out = reader.join().unwrap_or_default();
        let Some(status) = status else {
            return Err(format!("the DWG drawing took longer than {} seconds to read: it is probably damaged", HELPER_TIME.as_secs()));
        };
        let at = out.windows(MARK.len()).position(|w| w == MARK).ok_or(FAILED)?;
        let answer = out.get(at + MARK.len()..).unwrap_or_default();
        if !status.success() {
            return Err(FAILED.into());
        }
        decode(answer)
    }

    /// The helper: read the DWG, convert it, write the answer, exit.
    pub fn serve() {
        let mut bytes = Vec::new();
        // a read that fails leaves too few bytes, which the conversion refuses
        let _ = std::io::stdin().take((MAX_DWG + 1) as u64).read_to_end(&mut bytes);
        let answer = encode(dwg_to_dxf(&bytes, MAX_DWG, MAX_DXF));
        let mut out = std::io::stdout().lock();
        let ok = out.write_all(MARK).and_then(|_| out.write_all(&answer)).and_then(|_| out.flush()).is_ok();
        std::process::exit(if ok { 0 } else { 1 });
    }

    /// The answer: `ok`, the notes, a blank line and the DXF; or `err` and the message.
    fn encode(r: Result<(Vec<u8>, Vec<String>), String>) -> Vec<u8> {
        match r {
            Ok((dxf, notes)) => {
                let mut v = b"ok\n".to_vec();
                for n in notes {
                    v.extend(n.replace('\n', " ").as_bytes());
                    v.push(b'\n');
                }
                v.push(b'\n');
                v.extend(dxf);
                v
            }
            Err(e) => format!("err\n{}", e.replace('\n', " ")).into_bytes(),
        }
    }

    fn decode(answer: &[u8]) -> Result<(Vec<u8>, Vec<String>), String> {
        if let Some(e) = answer.strip_prefix(b"err\n") {
            return Err(String::from_utf8_lossy(e).into_owned());
        }
        let body = answer.strip_prefix(b"ok\n").ok_or(FAILED)?;
        // the notes end at the first blank line (at once when there are none)
        let end = if body.first() == Some(&b'\n') { 0 } else { body.windows(2).position(|w| w == b"\n\n").map(|i| i + 1).ok_or(FAILED)? };
        let notes = String::from_utf8_lossy(body.get(..end).unwrap_or_default()).lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
        Ok((body.get(end + 1..).unwrap_or_default().to_vec(), notes))
    }

    pub fn dwg_to_dxf(bytes: &[u8], max_dwg: usize, max_dxf: usize) -> Result<(Vec<u8>, Vec<String>), String> {
        if !super::is_dwg(bytes) {
            return Err("not a DWG drawing".into());
        }
        if bytes.len() > max_dwg {
            return Err(format!("the DWG drawing is larger than the {} MB that can be opened", max_dwg >> 20));
        }
        let read = catch_unwind(|| {
            acadrust::DwgReader::from_stream_with_options(Cursor::new(bytes), acadrust::DwgReadOptions::failsafe()).read_with_stats()
        })
        .map_err(|_| "the DWG drawing couldn't be read (the reader failed on it)".to_string())?
        .map_err(|e| format!("the DWG drawing couldn't be read: {e}"))?;
        if !read.stats.record_stream_read {
            return Err("the DWG drawing couldn't be read: its objects are damaged or in a version that isn't read".into());
        }
        let mut notes = Vec::new();
        let skipped = read.stats.skipped_source_records;
        if skipped > 0 {
            notes.push(format!(
                "{skipped} of the DWG drawing's {} objects couldn't be read and were left out",
                read.stats.source_records.max(skipped)
            ));
        }
        let mut out = Capped { buf: Vec::new(), limit: max_dxf, over: false };
        let doc = read.document;
        let written = catch_unwind(AssertUnwindSafe(|| acadrust::DxfWriter::new(&doc).write_to_writer(&mut out)))
            .map_err(|_| "the DWG drawing couldn't be converted (the converter failed on it)".to_string())?;
        if out.over {
            return Err("the DWG drawing turned into more drawing data than can be opened: it is probably damaged".into());
        }
        written.map_err(|e| format!("the DWG drawing couldn't be converted: {e}"))?;
        Ok((out.buf, notes))
    }

    /// A writer that stops at `limit` bytes.
    struct Capped {
        buf: Vec<u8>,
        limit: usize,
        over: bool,
    }

    impl Write for Capped {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.buf.len().saturating_add(b.len()) > self.limit {
                self.over = true;
                return Err(std::io::Error::other("too large"));
            }
            self.buf.extend_from_slice(b);
            Ok(b.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run as the helper by [`the_helper_process_answers_and_its_failures_stay_in_it`].
    #[test]
    fn helper_entry() {
        serve_if_helper();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_helper_process_answers_and_its_failures_stay_in_it() {
        let h = Helper {
            exe: std::env::current_exe().unwrap(),
            args: ["tests::helper_entry", "--exact", "--quiet", "--test-threads=1"].map(String::from).to_vec(),
        };
        // a drawing: its DXF comes back, the same as converting here
        let dwg = acadrust::DwgWriter::write_to_vec(&acadrust::CadDocument::new()).unwrap();
        let (dxf, notes) = native::in_helper(&h, &dwg).unwrap();
        assert_eq!((dxf.clone(), notes), dwg_to_dxf(&dwg).unwrap());
        assert!(String::from_utf8_lossy(&dxf).contains("SECTION"));
        // a damaged drawing: the helper's error comes back as it is
        let e = native::in_helper(&h, b"AC1032 not really a drawing").unwrap_err();
        assert!(e.contains("couldn't be read"), "{e}");
        assert!(native::in_helper(&h, b"hello").is_err());
        // a helper that dies without answering is a failed read, not a crash here
        let dead = Helper { exe: std::env::current_exe().unwrap(), args: vec!["--list".into()] };
        let e = native::in_helper(&dead, b"AC1032\0\0\0\0").unwrap_err();
        assert!(e.contains("reader failed"), "{e}");
        let missing = Helper { exe: "/nonexistent/helper".into(), args: vec![] };
        assert!(native::in_helper(&missing, b"AC1032\0\0\0\0").unwrap_err().contains("started"));
    }

    #[test]
    fn garbage_and_cut_files_are_refused_without_a_crash() {
        assert!(is_dwg(b"AC1032\0\0") && !is_dwg(b"AC10xx") && !is_dwg(b"0\nSECTION"));
        for junk in
            [&b"AC1015"[..], b"AC1018\0\0\0\0\0\0\0\0\0\0", b"AC1032 not really a drawing", &[b'A', b'C', b'1', b'0', b'2', b'4', 0xff, 0xff, 0xff]]
        {
            assert!(dwg_to_dxf(junk).is_err(), "{junk:?}");
        }
        assert!(dwg_to_dxf(b"hello").is_err());
    }
}
