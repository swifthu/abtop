//! minimax (`mmx`) account-level quota collector.
//!
//! Calls the local `mmx quota` CLI to retrieve a 5h + 7d window snapshot
//! for the first model in the response. Reuses the canonical
//! `RateLimitInfo` model so the existing quota panel can render mmx
//! alongside Claude without any UI changes beyond the `SOURCES` list.

use crate::model::RateLimitInfo;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// CLI invocation timeout. Anything slower is treated as a hard failure
/// (returns `None`); the TUI degrades to a "—" placeholder.
const MMX_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct MmxQuotaResponse {
    #[serde(default)]
    model_remains: Vec<MmxModelRemain>,
    #[serde(default)]
    base_resp: Option<MmxBaseResp>,
}

#[derive(Debug, Deserialize)]
struct MmxModelRemain {
    #[serde(default)]
    end_time: u64,
    #[serde(default)]
    current_interval_remaining_percent: f64,
    #[serde(default)]
    weekly_end_time: u64,
    #[serde(default)]
    current_weekly_remaining_percent: f64,
    #[serde(default)]
    #[allow(dead_code)]
    model_name: String,
}

#[derive(Debug, Deserialize)]
struct MmxBaseResp {
    #[serde(default)]
    status_code: i32,
}

/// Public entry point. Calls `mmx quota` and returns the first model's
/// quota, or `None` on any error.
pub fn read_mmx_quota() -> Option<RateLimitInfo> {
    run_mmx("mmx")
}

fn run_mmx(bin: &str) -> Option<RateLimitInfo> {
    let mut child = Command::new(bin)
        .arg("quota")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_status) => break,
            None => {
                if start.elapsed() >= MMX_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_response(&output.stdout)
}

/// Pure JSON → RateLimitInfo mapper. Exposed for unit testing without
/// spawning a subprocess. Takes the raw stdout bytes from `mmx quota`.
fn parse_response(stdout: &[u8]) -> Option<RateLimitInfo> {
    let text = std::str::from_utf8(stdout).ok()?;
    let resp: MmxQuotaResponse = serde_json::from_str(text).ok()?;

    if let Some(b) = &resp.base_resp {
        if b.status_code != 0 {
            return None;
        }
    }
    let first = resp.model_remains.first()?;

    // mmx returns *remaining*; RateLimitInfo / quota panel expects *used*.
    let used_5h = (100.0 - first.current_interval_remaining_percent).clamp(0.0, 100.0);
    let used_7d = (100.0 - first.current_weekly_remaining_percent).clamp(0.0, 100.0);

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Some(RateLimitInfo {
        source: "mmx".to_string(),
        five_hour_pct: Some(used_5h),
        five_hour_resets_at: Some(first.end_time / 1000),
        seven_day_pct: Some(used_7d),
        seven_day_resets_at: Some(first.weekly_end_time / 1000),
        updated_at: Some(now_secs),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_first_model_remaining_to_used() {
        let json = br#"{
            "model_remains": [
                {
                    "model_name": "general",
                    "end_time": 1781402400000,
                    "current_interval_remaining_percent": 90.0,
                    "weekly_end_time": 1781452800000,
                    "current_weekly_remaining_percent": 100.0
                },
                {
                    "model_name": "video",
                    "end_time": 1781452800000,
                    "current_interval_remaining_percent": 50.0,
                    "weekly_end_time": 1781452800000,
                    "current_weekly_remaining_percent": 0.0
                }
            ],
            "base_resp": {"status_code": 0}
        }"#;
        let info = parse_response(json).expect("should parse");
        assert_eq!(info.source, "mmx");
        assert_eq!(info.five_hour_pct, Some(10.0));
        assert_eq!(info.seven_day_pct, Some(0.0));
        assert_eq!(info.five_hour_resets_at, Some(1781402400));
        assert_eq!(info.seven_day_resets_at, Some(1781452800));
        assert!(info.updated_at.is_some());
    }

    #[test]
    fn returns_none_on_base_resp_error() {
        let json = br#"{
            "model_remains": [{"end_time": 0, "current_interval_remaining_percent": 50.0, "weekly_end_time": 0, "current_weekly_remaining_percent": 50.0}],
            "base_resp": {"status_code": 1, "status_msg": "fail"}
        }"#;
        assert!(parse_response(json).is_none());
    }

    #[test]
    fn returns_none_on_empty_model_remains() {
        let json = br#"{"model_remains": [], "base_resp": {"status_code": 0}}"#;
        assert!(parse_response(json).is_none());
    }

    #[test]
    fn returns_none_on_invalid_json() {
        assert!(parse_response(b"not json").is_none());
    }

    #[test]
    fn returns_none_on_non_utf8() {
        assert!(parse_response(&[0xff, 0xfe, 0xfd]).is_none());
    }

    #[test]
    fn tolerates_missing_base_resp() {
        let json = br#"{
            "model_remains": [{
                "end_time": 1000, "current_interval_remaining_percent": 0.0,
                "weekly_end_time": 2000, "current_weekly_remaining_percent": 50.0
            }]
        }"#;
        let info = parse_response(json).expect("missing base_resp OK");
        assert_eq!(info.five_hour_pct, Some(100.0));
        assert_eq!(info.seven_day_pct, Some(50.0));
    }

    /// Write a fake `mmx` shim to a temp dir, return the temp dir guard
    /// and the absolute path to the shim. The shim's body is a POSIX
    /// shell snippet that runs as the fake `mmx quota` command.
    fn fake_mmx(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mmx");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    #[test]
    fn run_mmx_spawns_subprocess_and_parses() {
        let (_dir, path) = fake_mmx(r#"echo '{"model_remains":[{"end_time":1000,"current_interval_remaining_percent":75.0,"weekly_end_time":2000,"current_weekly_remaining_percent":25.0}],"base_resp":{"status_code":0}}'"#);
        let info = run_mmx(path.to_str().unwrap()).expect("should parse");
        assert_eq!(info.source, "mmx");
        assert_eq!(info.five_hour_pct, Some(25.0));
        assert_eq!(info.seven_day_pct, Some(75.0));
    }

    #[test]
    fn run_mmx_returns_none_on_nonzero_exit() {
        let (_dir, path) = fake_mmx("exit 1");
        assert!(run_mmx(path.to_str().unwrap()).is_none());
    }

    #[test]
    fn run_mmx_returns_none_on_missing_binary() {
        assert!(run_mmx("/nonexistent/path/mmx").is_none());
    }

    #[test]
    fn run_mmx_returns_none_on_timeout() {
        let (_dir, path) = fake_mmx("sleep 5");
        let start = Instant::now();
        let result = run_mmx(path.to_str().unwrap());
        let elapsed = start.elapsed();
        assert!(result.is_none(), "expected None on timeout");
        assert!(
            elapsed < Duration::from_secs(4),
            "should not wait full 5s, got {elapsed:?}"
        );
    }
}
