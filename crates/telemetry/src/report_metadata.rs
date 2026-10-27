//! Machine-readable metadata for benchmark / report output (issue #6).
//!
//! Benchmark numbers are only comparable across runs when the
//! environment that produced them travels with them. This block is
//! local-only — nothing is uploaded anywhere — and it never carries
//! secrets: URLs are reduced to a `scheme://host` identifier, which
//! strips userinfo, paths, and query strings (where RPC and Jupiter
//! API keys commonly hide). Field names are deterministic; the schema
//! is versioned so consumers can rely on them.

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub const METADATA_SCHEMA: &str = "mobius-report-metadata/1";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReportMetadata {
    pub schema: &'static str,
    /// Build-time commit, from `MOBIUS_COMMIT_SHA` (or `GIT_SHA`) when
    /// the build environment provides it; `"unknown"` otherwise.
    pub commit_sha: String,
    pub mobius_version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    /// Scheduler mode label (e.g. `"Event"`, `"RoundRobin"`).
    pub scheduler_mode: String,
    pub duration_s: f64,
    /// `scheme://host` of the configured RPC endpoint, secrets stripped.
    pub rpc_host: String,
    /// Jupiter tier derived from the configured base URL's host, when
    /// one is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jupiter_tier: Option<String>,
    pub timestamp_unix_s: u64,
}

impl ReportMetadata {
    pub fn capture(
        scheduler_mode: impl Into<String>,
        duration_s: f64,
        rpc_url: &str,
        jupiter_base_url: Option<&str>,
    ) -> Self {
        Self {
            schema: METADATA_SCHEMA,
            commit_sha: commit_sha(),
            mobius_version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            scheduler_mode: scheduler_mode.into(),
            duration_s,
            rpc_host: Self::host_identifier(rpc_url),
            jupiter_tier: jupiter_base_url.and_then(Self::jupiter_tier),
            timestamp_unix_s: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Reduce a URL to `scheme://host` (port preserved). Userinfo,
    /// path, query and fragment — the places credentials hide — are
    /// dropped. Input without a scheme is treated as a bare host.
    /// Empty input yields `"unknown"`.
    pub fn host_identifier(url: &str) -> String {
        let url = url.trim();
        if url.is_empty() {
            return "unknown".to_string();
        }
        let (scheme, rest) = match url.split_once("://") {
            Some((s, r)) => (s, r),
            None => return url.split('/').next().unwrap_or("unknown").to_string(),
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // Drop userinfo (`user:pass@host`).
        let host = authority.rsplit('@').next().unwrap_or("");
        if host.is_empty() {
            return "unknown".to_string();
        }
        format!("{scheme}://{host}")
    }

    /// Jupiter tier from the base URL's host: `lite-api.jup.ag` is the
    /// lite tier, `api.jup.ag` the standard API; any other host is
    /// reported verbatim (minus scheme) so self-hosted gateways stay
    /// identifiable without inventing a tier name.
    pub fn jupiter_tier(base_url: &str) -> Option<String> {
        let id = Self::host_identifier(base_url);
        if id == "unknown" {
            return None;
        }
        let host = id.split("://").nth(1).unwrap_or(&id);
        let host = host.split(':').next().unwrap_or(host);
        Some(match host {
            "lite-api.jup.ag" => "lite".to_string(),
            "api.jup.ag" => "api".to_string(),
            other => other.to_string(),
        })
    }
}

fn commit_sha() -> String {
    option_env!("MOBIUS_COMMIT_SHA")
        .or(option_env!("GIT_SHA"))
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_has_deterministic_fields() {
        let m = ReportMetadata::capture(
            "Event",
            12.5,
            "https://rpc.example.com",
            Some("https://lite-api.jup.ag"),
        );
        let v: serde_json::Value = serde_json::to_value(&m).unwrap();
        let obj = v.as_object().unwrap();
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "schema",
                "commit_sha",
                "mobius_version",
                "os",
                "arch",
                "scheduler_mode",
                "duration_s",
                "rpc_host",
                "jupiter_tier",
                "timestamp_unix_s"
            ]
        );
        assert_eq!(obj["schema"], METADATA_SCHEMA);
        assert_eq!(obj["mobius_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(obj["scheduler_mode"], "Event");
        assert_eq!(obj["duration_s"], 12.5);
        assert_eq!(obj["jupiter_tier"], "lite");
        assert!(!obj["os"].as_str().unwrap().is_empty());
        assert!(!obj["arch"].as_str().unwrap().is_empty());
    }

    #[test]
    fn host_identifier_strips_secrets() {
        // API key in the query string.
        assert_eq!(
            ReportMetadata::host_identifier("https://mainnet.helius-rpc.com/?api-key=SECRET"),
            "https://mainnet.helius-rpc.com"
        );
        // Credentials in userinfo, key-like path segment.
        assert_eq!(
            ReportMetadata::host_identifier("https://user:pass@rpc.example.com:8899/SECRETKEY"),
            "https://rpc.example.com:8899"
        );
        assert_eq!(ReportMetadata::host_identifier(""), "unknown");
        assert_eq!(ReportMetadata::host_identifier("rpc.example.com"), "rpc.example.com");
    }

    #[test]
    fn jupiter_tier_mapping() {
        assert_eq!(
            ReportMetadata::jupiter_tier("https://lite-api.jup.ag/v6"),
            Some("lite".to_string())
        );
        assert_eq!(ReportMetadata::jupiter_tier("https://api.jup.ag"), Some("api".to_string()));
        assert_eq!(ReportMetadata::jupiter_tier(""), None);
    }
}
