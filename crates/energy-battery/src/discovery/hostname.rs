//! The mDNS service type and the `zendure-<product>-<sn>.local` hostname pattern.

/// Service advertised by every SolarFlow that runs the local API.
pub const SERVICE_TYPE: &str = "_zendure._tcp.local.";
const PREFIX: &str = "zendure";

/// Split `zendure-<product>-<sn>.local` into `(product, sn)`.
///
/// Both `-` and `_` are accepted as separators (the third-party registry regex does the same), the
/// `.local` suffix is optional, and the serial is the last token. The serial must look like a
/// battery serial (8+ alphanumeric characters), which rejects the meters that advertise the same
/// service under short suffixes such as `zendure-meter-3ct`.
pub fn parse_hostname(host: &str) -> Option<(String, String)> {
    let trimmed = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let base = trimmed.strip_suffix(".local").unwrap_or(&trimmed);
    let mut tokens = base
        .split(['-', '_'])
        .map(str::trim)
        .filter(|token| !token.is_empty());
    if tokens.next()? != PREFIX {
        return None;
    }
    let product = tokens.next()?.to_owned();
    let sn = tokens.next()?.to_ascii_uppercase();
    if sn.len() < 8 || !sn.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }
    Some((product, sn))
}

/// Build the hostname the mDNS record advertises.
pub fn build_hostname(product: &str, sn: &str) -> String {
    let product: String = product
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    format!("zendure-{product}-{}.local", sn.trim())
}
