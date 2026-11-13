//! Custom URI schemes:
//! - `thumb://localhost/<id>`  → cached JPEG thumbnail (generated on demand for visible items)
//! - `media://localhost/<id>` → the displayable file for the viewer, with HTTP Range support
//!   so video scrubbing seeks instantly without loading whole files.
//! Only catalog ids are accepted, never paths, so the webview cannot read arbitrary files.

use crate::AppState;
use std::io::{Read, Seek, SeekFrom};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

fn id_from(req: &Request<Vec<u8>>) -> Option<i64> {
    req.uri().path().trim_matches('/').split('/').next()?.parse().ok()
}

fn status(code: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder().status(code).header(header::CONTENT_TYPE, "text/plain").body(msg.as_bytes().to_vec()).unwrap()
}

pub fn thumbnail<R: Runtime>(ctx: UriSchemeContext<'_, R>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().state::<AppState>().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let resp = match id_from(&req) {
            None => status(StatusCode::BAD_REQUEST, "bad id"),
            // thumb://localhost/<id>/strip → timeline filmstrip for videos
            Some(id) if req.uri().path().trim_end_matches('/').ends_with("/strip") => match app.filmstrip(id) {
                Ok(path) => match std::fs::read(&path) {
                    Ok(bytes) => Response::builder().header(header::CONTENT_TYPE, "image/jpeg").header(header::CACHE_CONTROL, "max-age=31536000").body(bytes).unwrap(),
                    Err(_) => status(StatusCode::NOT_FOUND, "missing"),
                },
                Err(e) => status(StatusCode::NOT_FOUND, &e.to_string()),
            },
            Some(id) => match app.thumbnail(id) {
                Ok(Some(path)) => match std::fs::read(&path) {
                    Ok(bytes) => Response::builder()
                        .header(header::CONTENT_TYPE, "image/jpeg")
                        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                        .body(bytes)
                        .unwrap(),
                    Err(_) => status(StatusCode::NOT_FOUND, "missing"),
                },
                Ok(None) => status(StatusCode::NOT_FOUND, "no thumbnail"),
                Err(e) => status(StatusCode::NOT_FOUND, &e.to_string()),
            },
        };
        responder.respond(resp);
    });
}

const MAX_CHUNK: u64 = 8 * 1024 * 1024;

pub fn media<R: Runtime>(ctx: UriSchemeContext<'_, R>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().state::<AppState>().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let resp = (|| -> Result<Response<Vec<u8>>, String> {
            let id = id_from(&req).ok_or("bad id")?;
            let path = app.display_file(id).map_err(|e| e.to_string())?;
            let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let len = file.metadata().map_err(|e| e.to_string())?.len();
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let mime = curator_core::media::media_types::mime_for(&ext).unwrap_or("application/octet-stream");
            // QuickTime: WebKit plays H.264/HEVC .mov fine when labelled as quicktime.
            let range = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|r| parse_range(r, len));
            tracing::debug!(id, range = ?req.headers().get(header::RANGE), len, mime, "media request");
            match range {
                Some((start, end)) => {
                    let end = end.min(start + MAX_CHUNK - 1).min(len.saturating_sub(1));
                    let mut buf = vec![0u8; (end - start + 1) as usize];
                    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
                    file.read_exact(&mut buf).map_err(|e| e.to_string())?;
                    Ok(Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(header::CONTENT_TYPE, mime)
                        .header(header::ACCEPT_RANGES, "bytes")
                        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                        .header(header::CONTENT_LENGTH, buf.len().to_string())
                        .body(buf)
                        .unwrap())
                }
                None => {
                    // Images are read whole; large videos without a Range get the first chunk.
                    if len > MAX_CHUNK * 4 && mime.starts_with("video") {
                        let end = MAX_CHUNK - 1;
                        let mut buf = vec![0u8; MAX_CHUNK as usize];
                        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
                        return Ok(Response::builder()
                            .status(StatusCode::PARTIAL_CONTENT)
                            .header(header::CONTENT_TYPE, mime)
                            .header(header::ACCEPT_RANGES, "bytes")
                            .header(header::CONTENT_RANGE, format!("bytes 0-{end}/{len}"))
                            .body(buf)
                            .unwrap());
                    }
                    let mut buf = Vec::with_capacity(len as usize);
                    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                    Ok(Response::builder()
                        .header(header::CONTENT_TYPE, mime)
                        .header(header::ACCEPT_RANGES, "bytes")
                        .header(header::CONTENT_LENGTH, len.to_string())
                        .body(buf)
                        .unwrap())
                }
            }
        })();
        let resp = resp.unwrap_or_else(|e| {
            tracing::warn!(error = %e, uri = %req.uri(), "media request failed");
            status(StatusCode::NOT_FOUND, &e)
        });
        tracing::debug!(status = %resp.status(), bytes = resp.body().len(), "media response");
        responder.respond(resp);
    });
}

/// Parse a single "bytes=start-end" / "bytes=start-" / "bytes=-suffix" range.
fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let spec = h.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if len == 0 {
        return None;
    }
    if a.is_empty() {
        let suffix: u64 = b.parse().ok()?;
        return Some((len.saturating_sub(suffix), len - 1));
    }
    let start: u64 = a.parse().ok()?;
    if start >= len {
        return None;
    }
    let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
    (end >= start).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::parse_range;
    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(parse_range("bytes=50-", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=10-5", 100), None);
    }
}
