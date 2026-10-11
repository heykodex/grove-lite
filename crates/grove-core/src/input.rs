//! Turning whatever the user typed into something a tab can load.

use std::net::{Ipv4Addr, Ipv6Addr};
use url::Url;

/// Search endpoint used when the input is not a URL.
pub const SEARCH_URL: &str = "https://duckduckgo.com/";

/// Result of interpreting the contents of the address bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Nothing was typed; the tab stays blank.
    Blank,
    /// Load this URL.
    Navigate(Url),
}

/// Build a search URL for `query`.
pub fn search_url(query: &str) -> Url {
    Url::parse_with_params(SEARCH_URL, [("q", query.trim())]).expect("SEARCH_URL is a valid base")
}

/// Interpret address bar input.
///
/// * explicit `http://` / `https://` URLs are used as typed
/// * `example.com/path`, `localhost:3000`, `192.168.0.1` are treated as hosts
///   (`https` for domains, `http` for localhost, IP addresses and bare
///   host:port pairs)
/// * everything else, including `javascript:` and `file:` strings, is a search
pub fn resolve(input: &str) -> Resolved {
    let text = input.trim();
    if text.is_empty() {
        return Resolved::Blank;
    }

    let lower = text.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return match Url::parse(text) {
            Ok(url) if url.host_str().is_some() => Resolved::Navigate(url),
            _ => Resolved::Navigate(search_url(text)),
        };
    }

    if text.chars().any(char::is_whitespace) {
        return Resolved::Navigate(search_url(text));
    }

    if let Some(scheme) = host_scheme(text) {
        if let Ok(url) = Url::parse(&format!("{scheme}://{text}")) {
            if url.host_str().is_some() {
                return Resolved::Navigate(url);
            }
        }
    }

    Resolved::Navigate(search_url(text))
}

/// Decide whether `text` (which has no scheme) starts with something that is
/// plausibly a host, and if so which scheme to give it.
fn host_scheme(text: &str) -> Option<&'static str> {
    if text.contains('@') {
        return None; // looks like an email address
    }
    let end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let authority = &text[..end];
    if authority.is_empty() {
        return None;
    }

    let (host, has_port) = if let Some(rest) = authority.strip_prefix('[') {
        let close = rest.find(']')?;
        let host = &rest[..close];
        let tail = &rest[close + 1..];
        if host.parse::<Ipv6Addr>().is_err() {
            return None;
        }
        match tail.strip_prefix(':') {
            Some(port) if valid_port(port) => (host, true),
            None if tail.is_empty() => (host, false),
            _ => return None,
        }
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if !valid_port(port) {
            return None;
        }
        (host, true)
    } else {
        (authority, false)
    };

    if host.parse::<Ipv6Addr>().is_ok() || host.parse::<Ipv4Addr>().is_ok() {
        return Some("http");
    }
    if host.eq_ignore_ascii_case("localhost") {
        return Some("http");
    }
    if is_domain(host) {
        return Some("https");
    }
    // `intranet:8080` style host with an explicit port.
    if has_port && is_label(host) {
        return Some("http");
    }
    None
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.len() <= 5
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u32>().is_ok_and(|p| (1..=65535).contains(&p))
}

fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn is_domain(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || !labels.iter().all(|l| is_label(l)) {
        return false;
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2 && (tld.bytes().all(|b| b.is_ascii_alphabetic()) || tld.to_ascii_lowercase().starts_with("xn--"))
}

/// Text shown in the address bar for a loaded URL: no `https://`, no lone
/// trailing slash. Plain `http://` is kept so insecure pages are obvious.
pub fn display_url(url: &Url) -> String {
    let s = url.as_str();
    let bare_root = url.path() == "/" && url.query().is_none() && url.fragment().is_none();
    let s = if bare_root { s.strip_suffix('/').unwrap_or(s) } else { s };
    s.strip_prefix("https://").unwrap_or(s).to_string()
}

/// Navigations a web tab may perform. Everything that could reach local files
/// or the app's own pages (and therefore its IPC) is refused.
pub fn is_allowed_navigation(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => {
            let host = url.host_str().unwrap_or("").to_ascii_lowercase();
            host != "tauri.localhost" && !host.ends_with(".tauri.localhost")
        }
        "about" | "blob" | "data" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(input: &str) -> String {
        match resolve(input) {
            Resolved::Navigate(u) => u.to_string(),
            Resolved::Blank => "BLANK".into(),
        }
    }

    #[test]
    fn empty_is_blank() {
        assert_eq!(resolve(""), Resolved::Blank);
        assert_eq!(resolve("   \t "), Resolved::Blank);
    }

    #[test]
    fn explicit_urls_are_kept() {
        assert_eq!(nav("https://example.com/a?b=1"), "https://example.com/a?b=1");
        assert_eq!(nav("HTTP://Example.com"), "http://example.com/");
        assert_eq!(nav("  https://example.com  "), "https://example.com/");
    }

    #[test]
    fn bare_domains_get_https() {
        assert_eq!(nav("example.com"), "https://example.com/");
        assert_eq!(nav("www.rust-lang.org/learn"), "https://www.rust-lang.org/learn");
        assert_eq!(nav("docs.rs/tauri?x=1#top"), "https://docs.rs/tauri?x=1#top");
        assert_eq!(nav("example.com:8443/x"), "https://example.com:8443/x");
    }

    #[test]
    fn local_hosts_get_http() {
        assert_eq!(nav("localhost"), "http://localhost/");
        assert_eq!(nav("localhost:3000/app"), "http://localhost:3000/app");
        assert_eq!(nav("127.0.0.1:8080"), "http://127.0.0.1:8080/");
        assert_eq!(nav("192.168.1.1"), "http://192.168.1.1/");
        assert_eq!(nav("[::1]:8000"), "http://[::1]:8000/");
        assert_eq!(nav("intranet:8080"), "http://intranet:8080/");
    }

    #[test]
    fn everything_else_is_a_search() {
        assert_eq!(nav("rust tauri"), "https://duckduckgo.com/?q=rust+tauri");
        assert_eq!(nav("cats"), "https://duckduckgo.com/?q=cats");
        assert_eq!(nav("3.14"), "https://duckduckgo.com/?q=3.14");
        assert_eq!(nav("me@example.com"), "https://duckduckgo.com/?q=me%40example.com");
        assert_eq!(nav("what is 1+1?"), "https://duckduckgo.com/?q=what+is+1%2B1%3F");
        assert_eq!(nav("foo:bar"), "https://duckduckgo.com/?q=foo%3Abar");
    }

    #[test]
    fn dangerous_schemes_are_searched_not_loaded() {
        assert!(nav("javascript:alert(1)").starts_with("https://duckduckgo.com/?q="));
        assert!(nav("file:///etc/passwd").starts_with("https://duckduckgo.com/?q="));
        assert!(nav("data:text/html,hi").starts_with("https://duckduckgo.com/?q="));
        assert!(nav("tauri://localhost").starts_with("https://duckduckgo.com/?q="));
    }

    #[test]
    fn broken_explicit_url_falls_back_to_search() {
        assert!(nav("http://").starts_with("https://duckduckgo.com/?q="));
    }

    #[test]
    fn ports_are_validated() {
        assert!(nav("localhost:0").starts_with("https://duckduckgo.com/"));
        assert!(nav("localhost:99999").starts_with("https://duckduckgo.com/"));
        assert_eq!(nav("localhost:65535"), "http://localhost:65535/");
    }

    #[test]
    fn display_hides_https_and_bare_slash() {
        let d = |s: &str| display_url(&Url::parse(s).unwrap());
        assert_eq!(d("https://example.com/"), "example.com");
        assert_eq!(d("https://example.com/a/"), "example.com/a/");
        assert_eq!(d("https://example.com/?q=1"), "example.com/?q=1");
        assert_eq!(d("https://example.com/#x"), "example.com/#x");
        assert_eq!(d("http://example.com/"), "http://example.com");
    }

    #[test]
    fn navigation_policy() {
        let ok = |s: &str| is_allowed_navigation(&Url::parse(s).unwrap());
        assert!(ok("https://example.com"));
        assert!(ok("http://localhost:3000"));
        assert!(ok("about:blank"));
        assert!(ok("blob:https://example.com/uuid"));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("tauri://localhost/index.html"));
        assert!(!ok("ipc://localhost/cmd"));
        assert!(!ok("http://tauri.localhost/history.html"));
        assert!(!ok("https://x.tauri.localhost/"));
        assert!(!ok("asset://localhost/x"));
    }
}
