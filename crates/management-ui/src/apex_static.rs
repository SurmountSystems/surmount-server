//! Serve a static public site for apex/www Hosts from a document root.
//!
//! When `SURMOUNT_APEX_PUBLIC_ROOT` points at a directory that contains
//! `index.html`, those files replace the hardcoded coming-soon page.
//! Path traversal is rejected. Directory listing is off.

use std::path::{Component, Path, PathBuf};

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// True when the document root exists and has an index.html to serve.
pub fn document_root_is_ready(root: &Path) -> bool {
    root.join("index.html").is_file()
}

/// Resolve a request path to a file under `root`.
///
/// `/` and empty become `index.html`. Query strings are stripped. Percent-
/// encoded paths are decoded, then `..` and other non-normal components are
/// rejected. A directory maps to `index.html` inside it. The result is
/// canonicalized and must stay under `root`.
pub fn apex_static_file(root: &Path, req_path: &str) -> Option<PathBuf> {
    resolve_apex_static_path(root, req_path)
}

/// Resolve a request path to a file under `root` (see [`apex_static_file`]).
pub fn resolve_apex_static_path(root: &Path, req_path: &str) -> Option<PathBuf> {
    let path_only = req_path.split('?').next().unwrap_or(req_path);
    let decoded = percent_decode_path(path_only)?;
    let rel = if decoded.is_empty() || decoded == "/" {
        "index.html".to_string()
    } else {
        decoded.trim_start_matches('/').to_string()
    };
    if rel.is_empty() {
        return None;
    }

    let mut joined = root.to_path_buf();
    for component in Path::new(&rel).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    if joined.is_dir() {
        joined.push("index.html");
    }
    if !joined.is_file() {
        return None;
    }
    let root_canon = root.canonicalize().ok()?;
    let file_canon = joined.canonicalize().ok()?;
    if !file_canon.starts_with(&root_canon) {
        return None;
    }
    Some(file_canon)
}

/// Guess a Content-Type from the file extension.
pub fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

/// Read and return a static file, or 404.
pub fn serve_apex_static_file(path: &Path) -> Response {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let ct = content_type_for_path(path);
    match HeaderValue::from_str(ct) {
        Ok(v) => (StatusCode::OK, [(header::CONTENT_TYPE, v)], bytes).into_response(),
        Err(_) => (StatusCode::OK, bytes).into_response(),
    }
}

fn percent_decode_path(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = hex_nibble(bytes[i + 1])?;
                let lo = hex_nibble(bytes[i + 2])?;
                let b = (hi << 4) | lo;
                if b == 0 {
                    return None;
                }
                out.push(b);
                i += 3;
            }
            0 => return None,
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// CSP for the public static site (inline script on support.html).
///
/// Services console keeps the nonce CSP. Apex HTML uses unsafe-inline so
/// the existing site does not need a rewrite.
pub fn public_site_csp() -> &'static str {
    "default-src 'self'; \
     script-src 'self' 'unsafe-inline'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data:; \
     connect-src 'self'; \
     font-src 'self'; \
     object-src 'none'; \
     base-uri 'self'; \
     form-action 'self'; \
     frame-ancestors 'none'"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "surmount-apex-static-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(dir.join("fonts")).unwrap();
        fs::write(
            dir.join("index.html"),
            "<html>SURMOUNT-PUBLIC-SITE-MARKER</html>",
        )
        .unwrap();
        fs::write(
            dir.join("philosophy.html"),
            "<html>philosophy-marker</html>",
        )
        .unwrap();
        fs::write(dir.join("styles.css"), "body{color:red}").unwrap();
        fs::write(dir.join("fonts").join("cinzel-regular.woff2"), b"w2").unwrap();
        dir
    }

    #[test]
    fn resolve_root_and_named_files() {
        let dir = scratch_dir();
        let index = resolve_apex_static_path(&dir, "/").expect("index");
        assert!(index.ends_with("index.html"));
        assert!(resolve_apex_static_path(&dir, "/philosophy.html").is_some());
        assert!(resolve_apex_static_path(&dir, "/styles.css").is_some());
        assert!(resolve_apex_static_path(&dir, "/fonts/cinzel-regular.woff2").is_some());
        assert!(resolve_apex_static_path(&dir, "/missing.html").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_rejects_parent_dir_escape() {
        let dir = scratch_dir();
        let secret = dir
            .parent()
            .unwrap()
            .join(format!("surmount-apex-outside-{}", std::process::id()));
        fs::write(&secret, "OUTSIDE-ROOT-SECRET").unwrap();
        let name = secret.file_name().unwrap().to_string_lossy();
        assert!(apex_static_file(&dir, "/../Cargo.toml").is_none());
        assert!(apex_static_file(&dir, &format!("/../{name}")).is_none());
        assert!(apex_static_file(&dir, "/fonts/../../Cargo.toml").is_none());
        assert!(apex_static_file(&dir, "/..%2f..%2fetc/passwd").is_none());
        assert!(apex_static_file(&dir, &format!("/%2e%2e/{name}")).is_none());
        assert!(apex_static_file(&dir, &format!("/fonts/%2e%2e/%2e%2e/{name}")).is_none());
        let _ = fs::remove_file(&secret);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn public_site_csp_allows_inline_script_not_third_party() {
        let csp = public_site_csp();
        assert!(csp.contains("script-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("style-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("font-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("nonce-"));
        assert!(!csp.contains("https://"));
    }

    #[test]
    fn document_root_ready_requires_index() {
        let dir = scratch_dir();
        assert!(document_root_is_ready(&dir));
        let empty = dir.join("empty-subdir");
        fs::create_dir_all(&empty).unwrap();
        assert!(!document_root_is_ready(&empty));
        let _ = fs::remove_dir_all(&dir);
    }
}
