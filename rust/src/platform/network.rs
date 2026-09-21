use std::io::Read;
use std::time::Duration;

/// Read a proxy URL from config or environment.
///
/// Priority:
///   1. /data/adb/tricky_store/proxy.txt   (single line)
///   2. ALL_PROXY / all_proxy / HTTPS_PROXY / https_proxy env vars
///
/// The value may include a scheme (socks5://host:port, http://host:port).
/// If no scheme is present, socks5:// is assumed.
fn proxy_url() -> Option<String> {
    let raw = std::fs::read_to_string("/data/adb/tricky_store/proxy.txt")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            for k in ["ALL_PROXY", "all_proxy", "HTTPS_PROXY", "https_proxy"] {
                if let Ok(v) = std::env::var(k) {
                    let v = v.trim().to_string();
                    if !v.is_empty() {
                        return Some(v);
                    }
                }
            }
            None
        })?;

    // Allow bare "host:port" and default it to socks5.
    if raw.contains("://") {
        Some(raw)
    } else {
        Some(format!("socks5://{raw}"))
    }
}

/// Build a ureq agent, wiring in the proxy when one is configured.
fn agent() -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new();
    if let Some(p) = proxy_url() {
        match ureq::Proxy::new(&p) {
            Ok(proxy) => {
                b = b.proxy(proxy);
            }
            Err(e) => {
                tracing::warn!("invalid proxy '{p}': {e}; falling back to direct");
            }
        }
    }
    b.build()
}

pub fn is_online() -> bool {
    // ICMP ping cannot traverse an HTTP/SOCKS proxy, and GitHub is often
    // unreachable directly, so anchor the online check on domestic hosts
    // that answer without the proxy (Ali DNS / DNSPod over TCP 443).
    use std::net::TcpStream;
    for addr in ["223.5.5.5:443", "119.29.29.29:443", "1.1.1.1:443"] {
        if let Ok(sa) = addr.parse() {
            if TcpStream::connect_timeout(&sa, Duration::from_secs(5)).is_ok() {
                return true;
            }
        }
    }
    false
}

pub fn wait_for_network(max_attempts: u32) -> bool {
    for i in 0..max_attempts {
        if is_online() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(1 << i.min(4)));
    }
    false
}

pub fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    let resp = agent()
        .get(url)
        .timeout(Duration::from_secs(30))
        .call()?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(10 * 1024 * 1024)
        .read_to_end(&mut body)?;
    Ok(body)
}

pub fn download_text(url: &str) -> anyhow::Result<String> {
    let bytes = download(url)?;
    Ok(String::from_utf8(bytes)?)
}
