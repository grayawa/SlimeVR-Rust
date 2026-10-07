//! Native HTTP tools. Requests run outside the UI thread and retain original rollout rules.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, time::Duration};
pub fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("SlimeVR-GPUI")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}
pub fn http(method: &str, url: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut request = client()?.request(
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?,
        url,
    );
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("HTTP response exceeded 8 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn fnv_normalized(s: &str) -> f64 {
    let mut hash = 2166136261u32;
    for c in s.encode_utf16() {
        hash ^= c as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash as f64 / 4294967296.0
}
pub fn rollout(
    uuid: &str,
    version: &str,
    deploy: &Value,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let Some(map) = deploy.as_object() else {
        return false;
    };
    let mut rows = Vec::new();
    for (range, date) in map {
        let Some(range) = range.parse::<f64>().ok().filter(|r| *r > 0.0 && *r <= 1.0) else {
            return false;
        };
        let Some(date) = date
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        else {
            return false;
        };
        rows.push((range, date));
    }
    rows.sort_by(|(a, _), (b, _)| a.total_cmp(b));
    if rows.windows(2).any(|r| r[1].1 < r[0].1) {
        return false;
    }
    let threshold = rows
        .iter()
        .filter(|(_, date)| *date <= now)
        .map(|(r, _)| *r)
        .fold(0.0, f64::max);
    threshold > 0.0 && fnv_normalized(&format!("{uuid}-{version}")) <= threshold
}
pub fn official(uuid: &str) -> Result<Value, String> {
    let releases = http(
        "GET",
        "https://api.github.com/repos/SlimeVR/SlimeVR-Tracker-ESP/releases",
        None,
    )?;
    let mut candidates = Vec::new();
    for release in releases.as_array().ok_or("Invalid firmware release list")? {
        if release["prerelease"] == true || release["draft"] == true {
            continue;
        }
        let Some(assets) = release["assets"].as_array() else {
            continue;
        };
        let asset = |name: &str| {
            assets
                .iter()
                .find(|a| a["name"] == name)
                .cloned()
                .unwrap_or(Value::Null)
        };
        let deploy = asset("deploy.json");
        let classic = asset("BOARD_SLIMEVR-firmware.bin");
        let v12 = asset("BOARD_SLIMEVR_V1_2-firmware.bin");
        if deploy.is_null() || (classic.is_null() && v12.is_null()) {
            continue;
        }
        let version = release["tag_name"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('v');
        if semver::Version::parse(version).is_err() {
            continue;
        }
        let url = deploy["browser_download_url"]
            .as_str()
            .ok_or("Missing deployment URL")?;
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("github.com")
            || !parsed
                .path()
                .starts_with("/SlimeVR/SlimeVR-Tracker-ESP/releases/download/")
        {
            continue;
        }
        let allowed = http("GET", url, None)
            .map(|d| rollout(uuid, version, &d, chrono::Utc::now()))
            .unwrap_or(false);
        let candidate = json!({"name":release["name"],"version":version,"changelog":release["body"],"allowed":allowed,"files":{"9":classic,"22":v12}});
        if allowed {
            return Ok(candidate);
        }
        candidates.push(candidate);
        if candidates.len() >= 8 {
            break;
        }
    }
    candidates
        .into_iter()
        .next()
        .ok_or("No official firmware release is available".into())
}
pub fn digest_url(url: &str) -> Result<String, String> {
    let response = client()?
        .get(url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Firmware exceeds 32 MiB".into());
    }
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
pub fn tool_url(base: &str, path: &str, params: &[(&str, &str)]) -> Result<String, String> {
    let mut url = reqwest::Url::parse(base).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.password().is_some()
        || !url.username().is_empty()
    {
        return Err("Firmware service URL must use HTTP(S) without credentials".into());
    }
    url.set_path(&format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    ));
    if !params.is_empty() {
        url.query_pairs_mut().extend_pairs(params.iter().copied());
    }
    Ok(url.into())
}
