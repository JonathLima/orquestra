use serde::{Deserialize, Serialize};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use url::{Host, Url};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub allow_external_brain: bool,

    #[serde(default = "default_allow_proxy")]
    pub allow_proxy: bool,

    #[serde(default = "default_redact_secrets")]
    pub redact_secrets: bool,

    #[serde(default)]
    pub allowed_write_roots: Vec<PathBuf>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allow_external_brain: false,
            allow_proxy: default_allow_proxy(),
            redact_secrets: default_redact_secrets(),
            allowed_write_roots: vec![],
        }
    }
}

pub fn redact_secrets(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < chars.len() {
        if let Some(key) = secret_key_at(&chars, index)
            && let Some((redacted, next_index)) = redact_assignment(&chars, index, key)
        {
            output.push_str(&redacted);
            index = next_index;
            continue;
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}

pub fn contains_secret_like(input: &str) -> bool {
    redact_secrets(input) != input
}

pub fn public_http_url_host(value: &str) -> Option<String> {
    public_url_host(value, false)
}

pub fn public_https_url_host(value: &str) -> Option<String> {
    public_url_host(value, true)
}

pub fn host_resolves_to_non_public_ip(host: &str) -> bool {
    (host, 443)
        .to_socket_addrs()
        .map(|addresses| {
            addresses
                .into_iter()
                .any(|address| is_non_public_ip(address.ip()))
        })
        .unwrap_or(false)
}

fn public_url_host(value: &str, https_only: bool) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if (https_only && url.scheme() != "https")
        || (!https_only && !matches!(url.scheme(), "http" | "https"))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = match url.host()? {
        Host::Domain(domain) => domain.trim_end_matches('.').to_ascii_lowercase(),
        Host::Ipv4(_) | Host::Ipv6(_) => return None,
    };
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return None;
    }
    Some(host)
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_multicast()
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (segments[0] & 0xfe00 == 0xfc00)
                || (segments[0] & 0xffc0 == 0xfe80)
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

fn default_allow_proxy() -> bool {
    true
}

fn default_redact_secrets() -> bool {
    true
}

fn secret_key_at(chars: &[char], index: usize) -> Option<&'static str> {
    [
        "authorization",
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
    ]
    .into_iter()
    .find(|key| starts_with_ignore_ascii_case(chars, index, key))
}

fn starts_with_ignore_ascii_case(chars: &[char], index: usize, needle: &str) -> bool {
    let needle_chars = needle.chars().collect::<Vec<_>>();
    if index + needle_chars.len() > chars.len() {
        return false;
    }
    needle_chars
        .iter()
        .enumerate()
        .all(|(offset, expected)| chars[index + offset].eq_ignore_ascii_case(expected))
}

fn redact_assignment(chars: &[char], key_start: usize, key: &str) -> Option<(String, usize)> {
    let mut cursor = key_start + key.chars().count();
    let mut output = chars[key_start..cursor].iter().collect::<String>();

    if chars.get(cursor) == Some(&'"') {
        output.push('"');
        cursor += 1;
    }
    while chars.get(cursor).is_some_and(|c| c.is_whitespace()) {
        output.push(chars[cursor]);
        cursor += 1;
    }
    if !matches!(chars.get(cursor), Some('=') | Some(':')) {
        return None;
    }
    output.push(chars[cursor]);
    cursor += 1;
    while chars.get(cursor).is_some_and(|c| c.is_whitespace()) {
        output.push(chars[cursor]);
        cursor += 1;
    }

    if key.eq_ignore_ascii_case("authorization") {
        return Some(redact_authorization(chars, cursor, output));
    }

    let quoted = chars.get(cursor) == Some(&'"');
    if quoted {
        output.push('"');
        cursor += 1;
    }
    output.push_str("[REDACTED]");
    if quoted {
        while cursor < chars.len() && chars[cursor] != '"' {
            cursor += 1;
        }
        if cursor < chars.len() {
            output.push('"');
            cursor += 1;
        }
    } else {
        while cursor < chars.len() && !is_secret_value_boundary(chars[cursor]) {
            cursor += 1;
        }
    }
    Some((output, cursor))
}

fn redact_authorization(chars: &[char], mut cursor: usize, mut output: String) -> (String, usize) {
    if starts_with_ignore_ascii_case(chars, cursor, "bearer") {
        output.push_str(&chars[cursor..cursor + 6].iter().collect::<String>());
        cursor += 6;
        while chars.get(cursor).is_some_and(|c| c.is_whitespace()) {
            output.push(chars[cursor]);
            cursor += 1;
        }
    }
    output.push_str("[REDACTED]");
    while cursor < chars.len() && !is_secret_value_boundary(chars[cursor]) {
        cursor += 1;
    }
    (output, cursor)
}

fn is_secret_value_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | '}' | ']')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_defaults_disable_external_brain() {
        let config = SecurityConfig::default();
        assert!(!config.allow_external_brain);
        assert!(config.allow_proxy);
        assert!(config.redact_secrets);
    }

    #[test]
    fn redact_common_secret_tokens() {
        let redacted = redact_secrets("run token=abc password=secret ok");
        assert_eq!(redacted, "run token=[REDACTED] password=[REDACTED] ok");
        assert!(contains_secret_like("api_key=abc"));
    }

    #[test]
    fn redact_json_headers_and_colon_secrets() {
        let redacted = redact_secrets(
            r#"{"apiKey":"abc123","password": "secret"} Authorization: Bearer token-value secret: value ok"#,
        );

        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("secret\""));
        assert!(!redacted.contains("token-value"));
        assert!(!redacted.contains("secret: value"));
        assert!(contains_secret_like("Authorization: Bearer abc"));
    }
}
