use super::{Extra, Profile, Provider, Report, Window, credentials, digest, unix_now};
use serde_json::Value;

fn percent(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}
fn reset(value: &Value) -> Option<i64> {
    let n = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()));
    if let Some(n) = n.filter(|n| *n > 0) {
        return Some(if n > 1_000_000_000_000 { n / 1000 } else { n });
    }
    time::OffsetDateTime::parse(
        value.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
    .map(|t| t.unix_timestamp())
}
fn window(key: &str, label: &str, pct: &Value, at: &Value) -> Window {
    Window {
        key: key.into(),
        label: label.into(),
        percent: percent(pct),
        resets_at: reset(at),
    }
}
fn codex_windows(value: &Value, scope: &str) -> Vec<Window> {
    ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|name| {
            let w = value.get(name)?.as_object()?;
            let seconds = w.get("limit_window_seconds")?.as_u64()?;
            let (key, label) = match seconds {
                18000 => ("five_hour".into(), "5h".into()),
                604800 => ("weekly".into(), "7d".into()),
                s if s % 3600 == 0 => (format!("window_{s}"), format!("{}h", s / 3600)),
                s => (format!("window_{s}"), format!("{}m", s / 60)),
            };
            let (key, label) = if scope.is_empty() {
                (key, label)
            } else {
                (format!("{key}:{scope}"), format!("{label} {scope}"))
            };
            Some(window(
                &key,
                &label,
                value[name].get("used_percent").unwrap_or(&Value::Null),
                &value[name]["reset_at"],
            ))
        })
        .collect()
}
pub(super) fn parse(provider: Provider, value: &Value) -> (Vec<Window>, Option<Extra>) {
    let mut windows = Vec::new();
    let extra = match provider {
        Provider::Codex => {
            windows.extend(codex_windows(&value["rate_limit"], ""));
            for additional in value["additional_rate_limits"]
                .as_array()
                .into_iter()
                .flatten()
            {
                windows.extend(codex_windows(
                    &additional["rate_limit"],
                    additional["limit_name"].as_str().unwrap_or("Model"),
                ));
            }
            value["credits"].as_object().map(|_| Extra {
                label: "Credits".into(),
                unit: "credits".into(),
                used: None,
                limit: None,
                balance: value["credits"]["balance"].as_f64().or_else(|| {
                    value["credits"]["balance"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                }),
            })
        }
        Provider::Claude => {
            if let Some(limits) = value["limits"].as_array().filter(|v| !v.is_empty()) {
                for limit in limits {
                    let kind = limit["kind"].as_str().unwrap_or("unknown");
                    let (key, label) = match kind {
                        "session" => ("five_hour", "5h"),
                        "weekly_all" | "weekly_scoped" => ("weekly", "7d"),
                        other => (other, other),
                    };
                    let target = limit["scope"]
                        .get("model")
                        .filter(|v| v.is_object())
                        .or_else(|| limit["scope"].get("surface"));
                    let scope = target
                        .and_then(|t| t["display_name"].as_str().or_else(|| t["id"].as_str()));
                    let (key, label) = scope
                        .map(|s| (format!("{key}:{s}"), format!("{label} {s}")))
                        .unwrap_or((key.into(), label.into()));
                    windows.push(window(&key, &label, &limit["percent"], &limit["resets_at"]));
                }
            } else if let Some(fields) = value.as_object() {
                for (key, item) in fields {
                    if key != "five_hour" && !key.starts_with("seven_day") {
                        continue;
                    }
                    if !item.is_object() {
                        continue;
                    }
                    let (id, label) = if key == "five_hour" {
                        ("five_hour".into(), "5h".into())
                    } else if key == "seven_day" {
                        ("weekly".into(), "7d".into())
                    } else {
                        let name = key.trim_start_matches("seven_day_").replace('_', " ");
                        (format!("weekly:{name}"), format!("7d {name}"))
                    };
                    windows.push(window(
                        &id,
                        &label,
                        &item["utilization"],
                        &item["resets_at"],
                    ));
                }
            }
            value["extra_usage"]
                .as_object()
                .filter(|_| value["extra_usage"]["is_enabled"] == true)
                .map(|_| {
                    let e = &value["extra_usage"];
                    let scale =
                        10f64.powi(e["decimal_places"].as_i64().unwrap_or(2).clamp(0, 6) as i32);
                    Extra {
                        label: "Extra usage".into(),
                        unit: e["currency"].as_str().unwrap_or("").into(),
                        used: e["used_credits"].as_f64().map(|n| n / scale),
                        limit: e["monthly_limit"].as_f64().map(|n| n / scale),
                        balance: None,
                    }
                })
        }
    };
    windows.sort_by_key(|w| match w.key.as_str() {
        "five_hour" => 0,
        "weekly" => 1,
        _ => 2,
    });
    (windows, extra)
}

fn get(
    agent: &ureq::Agent,
    url: &str,
    auth: &credentials::Credentials,
    provider: Provider,
) -> Result<Value, String> {
    let mut request = agent
        .get(url)
        .header("Authorization", &format!("Bearer {}", auth.token))
        .header("Accept", "application/json");
    match provider {
        Provider::Claude => {
            request = request.header("anthropic-beta", "oauth-2025-04-20");
        }
        Provider::Codex => {
            request = request.header("User-Agent", "codex-cli/0.130.0");
            if let Some(id) = &auth.account_header {
                request = request.header("chatgpt-account-id", id);
            }
        }
    }
    let mut response = request.call().map_err(|error| -> String {
        match error {
            ureq::Error::StatusCode(401 | 403) => "Sign in again to read usage".into(),
            ureq::Error::StatusCode(429) => "Usage service rate limited. Retrying later".into(),
            _ => "Usage service is temporarily unavailable".into(),
        }
    })?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .map_err(|_| "Could not read usage response".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Could not read usage response".into())
}

pub(super) fn fetch(profile: &Profile, allow_prompt: bool) -> Report {
    let mut report = Report::new(profile, 0);
    let result = (|| {
        let auth = credentials::load(profile, allow_prompt)?;
        report.account_key = auth.account_key.clone();
        report.account_label = auth.label.clone();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .into();
        let url = match profile.provider {
            Provider::Codex => "https://chatgpt.com/backend-api/codex/usage",
            Provider::Claude => "https://api.anthropic.com/api/oauth/usage",
        };
        let value = get(&agent, url, &auth, profile.provider)?;
        (report.windows, report.extra) = parse(profile.provider, &value);
        if !report.windows.iter().any(|w| w.percent.is_some()) {
            return Err("Usage response contained no limits".into());
        }
        if profile.provider == Provider::Claude
            && let Ok(identity) = get(
                &agent,
                "https://api.anthropic.com/api/oauth/profile",
                &auth,
                profile.provider,
            )
        {
            let account = identity["account"]["uuid"].as_str();
            let org = identity["organization"]["uuid"].as_str();
            report.account_key = account
                .zip(org)
                .map(|(a, o)| digest(&format!("claude:{a}:{o}")));
            report.account_label = identity["account"]["email"]
                .as_str()
                .or_else(|| identity["account"]["email_address"].as_str())
                .map(str::to_owned);
        }
        Ok(())
    })();
    report.checked_at = Some(unix_now());
    report.error = result.err();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_transport_only_gets_and_never_exposes_error_bodies() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_lowercase();
            assert!(request.starts_with("get /usage "));
            assert!(request.contains("authorization: bearer fixture-token"));
            socket.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\nConnection: close\r\n\r\nprivate-error").unwrap();
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(2)))
            .build()
            .into();
        let auth = credentials::Credentials {
            token: "fixture-token".into(),
            account_header: None,
            account_key: None,
            label: None,
        };
        let error = get(
            &agent,
            &format!("http://{address}/usage"),
            &auth,
            Provider::Codex,
        )
        .unwrap_err();
        assert_eq!(error, "Sign in again to read usage");
        assert!(!error.contains("fixture-token") && !error.contains("private-error"));
        server.join().unwrap();
    }
    #[test]
    fn handles_provider_windows_scopes_unknowns_and_resets() {
        let (windows, _) = parse(
            Provider::Codex,
            &json!({"rate_limit":{"primary_window":{"limit_window_seconds":18000,"used_percent":25.5,"reset_at":1700000000000i64},"secondary_window":{"limit_window_seconds":604800,"used_percent":null}},"additional_rate_limits":[{"limit_name":"special","rate_limit":{"primary_window":{"limit_window_seconds":3600,"used_percent":1}}}]}),
        );
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].percent, Some(25.5));
        assert_eq!(windows[0].resets_at, Some(1700000000));
        assert_eq!(windows[1].percent, None);
        assert!(windows.iter().any(|w| w.label == "1h special"));
        let (windows, _) = parse(
            Provider::Claude,
            &json!({"five_hour":{"utilization":80,"resets_at":"2026-11-20T12:00:00Z"},"seven_day":{"utilization":40},"seven_day_model":{"utilization":null}}),
        );
        assert_eq!(windows.len(), 3);
        assert!(windows[0].resets_at.is_some());
        let (windows, _) = parse(
            Provider::Claude,
            &json!({"limits":[{"kind":"weekly_scoped","percent":30,"scope":{"model":{"display_name":"Model"}}}]}),
        );
        assert_eq!(windows[0].label, "7d Model");
        assert!(parse(Provider::Claude, &json!({})).0.is_empty());
    }
}
