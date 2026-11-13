//! FFprobe inspection of video and audio files.

use crate::core::errors::{CuratorError, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Raw ffprobe JSON (`-show_format -show_streams`).
pub fn probe(ffprobe: &Path, file: &Path) -> Result<Value> {
    let out = run_with_timeout(
        Command::new(ffprobe).args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(file),
        Duration::from_secs(60),
    )?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(CuratorError::ToolFailed(format!("ffprobe could not read the file: {}", err.lines().last().unwrap_or("unknown error"))));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

/// Run a command, killing it if it exceeds `timeout` (protects workers from hung decoders).
pub fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<std::process::Output> {
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_h = std::thread::spawn(move || {
        let mut v = vec![];
        if let Some(mut s) = stdout {
            let _ = std::io::Read::read_to_end(&mut s, &mut v);
        }
        v
    });
    let err_h = std::thread::spawn(move || {
        let mut v = vec![];
        if let Some(mut s) = stderr {
            let _ = std::io::Read::read_to_end(&mut s, &mut v);
        }
        v
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CuratorError::ToolFailed(format!("timed out after {}s", timeout.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    Ok(std::process::Output { status, stdout: out_h.join().unwrap_or_default(), stderr: err_h.join().unwrap_or_default() })
}

/// Parse an ffprobe rational like "30000/1001".
pub fn parse_rational(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let n: f64 = n.parse().ok()?;
    let d: f64 = d.parse().ok()?;
    if d == 0.0 || n == 0.0 {
        None
    } else {
        Some(n / d)
    }
}

/// Parse ISO 6709 location strings, e.g. "+40.4329-073.9876+005.000/".
pub fn parse_iso6709(s: &str) -> Option<(f64, f64, Option<f64>)> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = vec![];
    let mut cur = String::new();
    for ch in s.chars() {
        if (ch == '+' || ch == '-') && !cur.is_empty() {
            parts.push(cur.clone());
            cur.clear();
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let lat: f64 = parts.first()?.parse().ok()?;
    let lon: f64 = parts.get(1)?.parse().ok()?;
    let alt = parts.get(2).and_then(|a| a.parse().ok());
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some((lat, lon, alt))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rationals() {
        assert!((parse_rational("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert!(parse_rational("0/0").is_none());
    }
    #[test]
    fn iso6709() {
        let (lat, lon, alt) = parse_iso6709("+40.4329-073.9876+005.000/").unwrap();
        assert!((lat - 40.4329).abs() < 1e-6);
        assert!((lon + 73.9876).abs() < 1e-6);
        assert_eq!(alt, Some(5.0));
        let (lat, lon, alt) = parse_iso6709("-27.5954-048.5480/").unwrap();
        assert!(lat < 0.0 && lon < 0.0 && alt.is_none());
    }
}
