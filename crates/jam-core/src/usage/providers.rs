use super::{Extra, Profile, Provider, Report, Window, cache, credentials, digest, unix_now};
use serde_json::Value;
use std::sync::Mutex;

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

#[derive(Debug)]
enum RequestError {
    RateLimited(Option<i64>),
    Other(&'static str),
}

fn retry_at(value: &str, now: i64) -> Option<i64> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(now.saturating_add(seconds.min(i64::MAX as u64) as i64));
    }
    let format = time::format_description::parse_borrowed::<2>(
        "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT",
    )
    .ok()?;
    time::PrimitiveDateTime::parse(value, &format)
        .ok()
        .map(|date| date.assume_utc().unix_timestamp())
}

fn get(
    agent: &ureq::Agent,
    url: &str,
    auth: &credentials::Credentials,
    provider: Provider,
) -> Result<Value, RequestError> {
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
    let mut response = request
        .call()
        .map_err(|_| RequestError::Other("Usage service is temporarily unavailable"))?;
    match response.status().as_u16() {
        200 => (),
        401 | 403 => return Err(RequestError::Other("Sign in again to read usage")),
        429 => {
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| retry_at(value, unix_now()));
            return Err(RequestError::RateLimited(retry));
        }
        _ => {
            return Err(RequestError::Other(
                "Usage service is temporarily unavailable",
            ));
        }
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .map_err(|_| RequestError::Other("Could not read usage response"))?;
    serde_json::from_slice(&bytes).map_err(|_| RequestError::Other("Could not read usage response"))
}

pub(super) fn fetch(profile: &Profile, allow_prompt: bool, cache: &Mutex<cache::Cache>) -> Report {
    let mut report = Report::new(profile, 0);
    let auth = match credentials::load(profile, allow_prompt) {
        Ok(auth) => auth,
        Err(error) => {
            report.checked_at = Some(unix_now());
            report.error = Some(error);
            return report;
        }
    };
    // Prefer an account and subject key when credentials supply one. Opaque
    // Claude tokens use a digest so changing logins never displays the old quota.
    let credential = auth
        .account_key
        .clone()
        .unwrap_or_else(|| digest(&format!("usage:{}", auth.token)));
    let now = unix_now();
    let previous = cache.lock().unwrap().get(&profile.key, &credential, now);
    if let Some(entry) = &previous
        && entry.retry_at > now
    {
        let mut report = entry.report.clone();
        entry.expire_success(&mut report, now);
        return report;
    }
    report.account_key = auth
        .account_key
        .clone()
        .or_else(|| previous.as_ref()?.report.account_key.clone());
    report.account_label = auth
        .label
        .clone()
        .or_else(|| previous.as_ref()?.report.account_label.clone());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    let url = match profile.provider {
        Provider::Codex => "https://chatgpt.com/backend-api/codex/usage",
        Provider::Claude => "https://api.anthropic.com/api/oauth/usage",
    };
    let result = get(&agent, url, &auth, profile.provider).and_then(|value| {
        let (windows, extra) = parse(profile.provider, &value);
        if !windows.iter().any(|window| window.percent.is_some()) {
            return Err(RequestError::Other("Usage response contained no limits"));
        }
        report.windows = windows;
        report.extra = extra;
        if profile.provider == Provider::Claude
            && report.account_key.is_none()
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
    });
    let now = unix_now();
    let entry = record_reading(credential, report, previous.as_ref(), result, now);
    let report = entry.report.clone();
    let _ = cache.lock().unwrap().store(entry, now);
    report
}

fn record_reading(
    credential: String,
    mut report: Report,
    previous: Option<&cache::Reading>,
    result: Result<(), RequestError>,
    now: i64,
) -> cache::Reading {
    report.checked_at = Some(now);
    report.checking = false;
    let mut retry_at = now + cache::REFRESH_INTERVAL;
    let mut rate_limits = 0;
    match result {
        Ok(()) => report.last_success_at = Some(now),
        Err(error) => {
            let message = match error {
                RequestError::RateLimited(server_retry) => {
                    rate_limits = previous.map_or(1, |entry| entry.rate_limits.saturating_add(1));
                    let delay = cache::REFRESH_INTERVAL * (1 << (rate_limits - 1).min(4));
                    retry_at = (now + delay.min(3600)).max(server_retry.unwrap_or(now));
                    "Usage service rate limited. Retrying later"
                }
                RequestError::Other(message) => message,
            };
            if let Some(previous) = previous {
                previous.keep_last_success(&mut report, now);
            }
            report.error = Some(message.into());
        }
    }
    cache::Reading {
        credential,
        report,
        retry_at,
        rate_limits,
    }
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
            .http_status_as_error(false)
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
        assert!(matches!(
            error,
            RequestError::Other("Sign in again to read usage")
        ));
        server.join().unwrap();
    }
    #[test]
    fn rate_limited_usage_survives_daemon_restart_for_the_same_login() {
        let directory = tempfile::tempdir().unwrap();
        let profile = Profile::new(
            Provider::Claude,
            directory.path().into(),
            directory.path().into(),
            Some("fixture-token".into()),
            false,
        );
        let path = directory.path().join("state/usage-cache.json");
        let mut cache = cache::Cache::open(path.clone());
        let now = unix_now();
        let mut report = Report::new(&profile, 1);
        report
            .windows
            .push(window("five_hour", "5h", &json!(42), &Value::Null));
        let credential = digest("usage:fixture-token");
        let success = record_reading(credential.clone(), report, None, Ok(()), now - 301);
        let next = retry_at("1800", now).unwrap();
        assert_eq!(next, now + 1800);
        assert_eq!(
            retry_at("Fri, 20 Nov 2026 12:00:00 GMT", now),
            Some(1795176000)
        );
        let throttled = record_reading(
            credential.clone(),
            Report::new(&profile, 1),
            Some(&success),
            Err(RequestError::RateLimited(Some(next))),
            now,
        );
        assert_eq!(throttled.retry_at, next);
        assert_eq!(throttled.report.last_success_at, Some(now - 301));
        assert_eq!(throttled.report.windows[0].percent, Some(42.0));
        let retry = record_reading(
            credential.clone(),
            Report::new(&profile, 1),
            Some(&throttled),
            Err(RequestError::RateLimited(None)),
            next,
        );
        assert_eq!(retry.retry_at, next + 600);
        cache.store(throttled, now).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("fixture-token")
        );
        let restarted = Mutex::new(cache::Cache::open(path));
        // No network request should occur while the persisted cooldown is active.
        let displayed = fetch(&profile, false, &restarted);
        assert_eq!(
            displayed.error.as_deref(),
            Some("Usage service rate limited. Retrying later")
        );
        assert_eq!(displayed.windows[0].percent, Some(42.0));
        assert_eq!(displayed.last_success_at, Some(now - 301));
        assert!(
            restarted
                .lock()
                .unwrap()
                .get(&profile.key, "new-login", now)
                .is_none()
        );
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
