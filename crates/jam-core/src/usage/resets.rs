use super::{MONITOR, Profile, Provider, credentials, process, providers, unix_now};
use crate::codex_session::Rpc;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResetCredits {
    available: u64,
    credits: Option<Vec<ResetCredit>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ResetCredit {
    credit_id: String,
    title: Option<String>,
    expires_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetOutcome {
    Reset,
    AlreadyRedeemed,
    NoCredit,
    NothingToReset,
    Cooldown,
    Ineligible,
    Unavailable,
}

fn available(reading: &Value, account_id: &str) -> Result<ResetCredits> {
    ensure!(
        reading["accountId"].as_str() == Some(account_id),
        "Codex could not verify the selected account"
    );
    let summary = &reading["rateLimitResetCredits"];
    let available = summary["availableCount"]
        .as_u64()
        .context("Codex did not report reset availability")?;
    let credits = summary["credits"].as_array().map(|rows| {
        rows.iter()
            .filter_map(|credit| {
                let expires_at = credit["expiresAt"].as_i64();
                if credit["status"] != "available"
                    || credit["resetType"] != "codexRateLimits"
                    || expires_at.is_some_and(|at| at <= unix_now())
                {
                    return None;
                }
                Some(ResetCredit {
                    credit_id: credit["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())?
                        .to_owned(),
                    title: credit["title"].as_str().map(str::to_owned),
                    expires_at,
                })
            })
            .collect()
    });
    Ok(ResetCredits { available, credits })
}

fn tracked_profile(profile_key: &str, account: &str) -> Result<Profile> {
    MONITOR
        .state
        .lock()
        .unwrap()
        .entries
        .get(profile_key)
        .filter(|e| e.report.account_key.as_deref() == Some(account))
        .map(|e| e.profile.clone())
        .context("The selected usage account is no longer available")
}

enum Session {
    Codex(Rpc),
    Claude {
        client: ureq::Agent,
        token: String,
        organization: uuid::Uuid,
    },
}

const CLAUDE_API: &str = "https://api.anthropic.com";

fn claude_grants(reading: &Value) -> Result<ResetCredits> {
    let block = &reading["cedar_ember"];
    let grants = block["grants"]
        .as_array()
        .context("Claude did not report free reset availability")?;
    let next = block["next_grant_id"].as_str();
    let mut available: u64 = 0;
    let mut credits = Vec::new();
    for grant in grants {
        let Some(id) = grant["id"].as_str() else {
            continue;
        };
        let count = grant["resets_left"].as_u64().unwrap_or_default();
        let expires_at = providers::reset(&grant["ends_at"]);
        if count == 0
            || !(1..=40).contains(&id.len())
            || !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
            || (grant["paused"] == true && Some(id) != next)
            || expires_at.is_some_and(|at| at <= unix_now())
            || block["eligible"] == false
        {
            continue;
        }
        available = available.saturating_add(count);
        credits.push(ResetCredit {
            credit_id: id.into(),
            title: grant["title"].as_str().map(str::to_owned),
            expires_at,
        });
    }
    // Multiple grants can offer identical resets. Follow the provider's preferred grant.
    credits.sort_by_key(|grant| Some(grant.credit_id.as_str()) != next);
    credits.truncate(1);
    Ok(ResetCredits {
        available,
        credits: Some(credits),
    })
}

fn read_claude(
    client: &ureq::Agent,
    base: &str,
    auth: &credentials::Credentials,
    account: &str,
) -> Result<(uuid::Uuid, ResetCredits)> {
    let identity = providers::get(
        client,
        &format!("{base}/api/oauth/profile"),
        auth,
        Provider::Claude,
    )?;
    ensure!(
        providers::claude_account_key(&identity).as_deref() == Some(account),
        "The Claude login does not match the selected account"
    );
    let org = identity["organization"]["uuid"]
        .as_str()
        .and_then(|v| v.parse().ok())
        .context("Invalid Claude organization")?;
    let usage = providers::get(
        client,
        &format!("{base}/api/oauth/usage?cedar_ember=1"),
        auth,
        Provider::Claude,
    )?;
    Ok((org, claude_grants(&usage)?))
}

fn read(profile: &Profile, account: &str) -> Result<(Session, ResetCredits)> {
    ensure!(
        crate::config::agent_enabled(profile.provider.name()),
        "This integration is disabled in Settings"
    );
    let auth = credentials::load(profile, false).map_err(anyhow::Error::msg)?;
    if profile.provider == Provider::Claude {
        let client = providers::http_client();
        let (org, credits) = read_claude(&client, CLAUDE_API, &auth, account)?;
        return Ok((
            Session::Claude {
                client,
                token: auth.token,
                organization: org,
            },
            credits,
        ));
    }
    ensure!(
        auth.account_key.as_deref() == Some(account),
        "The Codex login does not match the selected account"
    );
    let account_id = auth
        .account_header
        .context("Codex account identity unavailable")?;
    let mut rpc =
        Rpc::start(&json!({"path":profile.directory,"config_directory":profile.directory}))
            .map_err(|_| anyhow::anyhow!("Could not start Codex to check free resets"))?;
    let reading = rpc
        .call("account/rateLimits/read", json!({}))
        .map_err(|_| anyhow::anyhow!("Could not read free resets from Codex"))?;
    Ok((Session::Codex(rpc), available(&reading, &account_id)?))
}

pub fn reset_credits(profile: &str, account: &str) -> Result<ResetCredits> {
    Ok(read(&tracked_profile(profile, account)?, account)?.1)
}

fn redeem(
    credits: &ResetCredits,
    credit: Option<&str>,
    attempt: uuid::Uuid,
    consume: impl FnOnce(Value) -> Result<Value>,
) -> Result<ResetOutcome> {
    ensure!(
        credits.available > 0
            && match (&credits.credits, credit) {
                (Some(rows), Some(id)) => rows.iter().any(|c| c.credit_id == id),
                (None, None) => true,
                _ => false,
            },
        "This free reset is no longer available. Reopen usage to check this account."
    );
    let reply =
        consume(json!({"creditId":credit,"idempotencyKey":attempt.to_string()})).map_err(|_| {
            anyhow::anyhow!("Codex did not confirm this reset. Check usage before trying again.")
        })?;
    serde_json::from_value(reply["outcome"].clone())
        .context("Codex returned an unknown reset outcome")
}

pub fn redeem_reset(
    profile: &str,
    account: &str,
    credit: Option<&str>,
    attempt: &str,
) -> Result<ResetOutcome> {
    let outcome = consume(
        &tracked_profile(profile, account)?,
        account,
        credit,
        attempt,
    )?;
    if matches!(outcome, ResetOutcome::Reset | ResetOutcome::AlreadyRedeemed) {
        for entry in MONITOR
            .state
            .lock()
            .unwrap()
            .entries
            .values_mut()
            .filter(|entry| entry.report.account_key.as_deref() == Some(account))
        {
            entry.attempted = None;
            entry.refresh_after_reset = true;
        }
        let _ = MONITOR.wake.try_send(());
    }
    Ok(outcome)
}

fn consume(
    profile: &Profile,
    account: &str,
    credit: Option<&str>,
    attempt: &str,
) -> Result<ResetOutcome> {
    let attempt = uuid::Uuid::parse_str(attempt)?;
    let (session, credits) = read(profile, account)?;
    match session {
        Session::Codex(mut rpc) => redeem(&credits, credit, attempt, |params| {
            rpc.call("account/rateLimitResetCredit/consume", params)
        }),
        Session::Claude {
            client,
            token,
            organization,
        } => {
            let grant = credit.context("A Claude reset grant is required")?;
            ensure!(
                credits
                    .credits
                    .as_ref()
                    .is_some_and(|items| items.iter().any(|g| g.credit_id == grant)),
                "The selected Claude reset is no longer available"
            );
            claim_claude(&client, CLAUDE_API, &token, organization, grant, attempt)
        }
    }
}

fn claim_claude(
    client: &ureq::Agent,
    base: &str,
    token: &str,
    org: uuid::Uuid,
    grant: &str,
    attempt: uuid::Uuid,
) -> Result<ResetOutcome> {
    let request = client.post(format!("{base}/api/organizations/{org}/reset_rate_limits"));
    let body = json!({"program":"cedar_ember","grant_id":grant,"request_id":attempt.to_string()});
    let reply =
        providers::response_json(providers::claude_request(request, token).send(body.to_string()))?;
    match reply["result"].as_str() {
        Some("reset") => Ok(ResetOutcome::Reset),
        Some("already_used") => Ok(ResetOutcome::AlreadyRedeemed),
        Some("not_limited") => Ok(ResetOutcome::NothingToReset),
        Some("cooldown") => Ok(ResetOutcome::Cooldown),
        Some("ineligible") => Ok(ResetOutcome::Ineligible),
        Some("unavailable") => Ok(ResetOutcome::Unavailable),
        _ => anyhow::bail!("Claude did not confirm this reset"),
    }
}

/// Check the desktop's configured login when an older daemon cannot handle resets.
/// A matching verified provider account is required by read() before any redemption.
pub fn configured_reset(provider: Provider, request: crate::Request) -> Result<crate::Message> {
    let profile = process::configured_profiles().into_iter().find(|p| p.provider == provider)
        .context("No local login is available for this provider. Update Jam on the account’s machine to manage its resets.")?;
    match request {
        crate::Request::UsageResetCredits { account, .. } => Ok(crate::Message::UsageResetCredits(
            read(&profile, &account)?.1,
        )),
        crate::Request::RedeemUsageReset {
            account,
            credit,
            attempt,
            ..
        } => Ok(crate::Message::UsageResetOutcome(consume(
            &profile,
            &account,
            credit.as_deref(),
            &attempt,
        )?)),
        _ => anyhow::bail!("Expected a usage reset request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_uses_the_verified_organization_and_preferred_grant() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let org = uuid::Uuid::new_v4();
        let attempt = uuid::Uuid::new_v4();
        let identity =
            json!({"account":{"uuid":"fixture"},"organization":{"uuid":org.to_string()}});
        let account = providers::claude_account_key(&identity).unwrap();
        let usage = json!({"cedar_ember":{"next_grant_id":"preferred","grants":[
            {"id":"used","resets_left":0}, {"id":"expired","resets_left":1,"ends_at":1},
            {"id":"open","resets_left":1}, {"id":"paused","resets_left":5,"paused":true},
            {"id":"preferred","resets_left":2,"paused":true}, {"id":"../bad","resets_left":4}
        ]}});
        let server = std::thread::spawn(move || {
            let claim = format!("POST /api/organizations/{org}/reset_rate_limits");
            for (expected, reply) in [
                ("GET /api/oauth/profile", &identity),
                ("GET /api/oauth/profile", &identity),
                ("GET /api/oauth/usage?cedar_ember=1", &usage),
                (claim.as_str(), &json!({"result":"not_limited"})),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line.trim_end(), format!("{expected} HTTP/1.1"));
                let mut headers = String::new();
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line.to_ascii_lowercase());
                }
                assert!(headers.contains("authorization: bearer fixture"));
                assert!(headers.contains("anthropic-beta: oauth-2025-04-20"));
                if expected == claim {
                    let length = headers
                        .lines()
                        .find_map(|h| h.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    let mut body = vec![0u8; length];
                    reader.read_exact(&mut body).unwrap();
                    assert_eq!(
                        serde_json::from_slice::<Value>(&body).unwrap(),
                        json!({"grant_id":"preferred","program":"cedar_ember","request_id":attempt.to_string()})
                    );
                }
                let reply = reply.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
            }
        });
        let client = ureq::Agent::new_with_defaults();
        let auth = credentials::Credentials {
            token: "fixture".into(),
            account_header: None,
            account_key: None,
            label: None,
        };
        assert!(read_claude(&client, &base, &auth, "different-account").is_err());
        let (verified, credits) = read_claude(&client, &base, &auth, &account).unwrap();
        assert_eq!(verified, org);
        assert_eq!(credits.available, 3);
        let credits = credits.credits.unwrap();
        assert_eq!(credits.len(), 1);
        assert_eq!(credits[0].credit_id, "preferred");
        let result = claim_claude(
            &client,
            &base,
            &auth.token,
            verified,
            &credits[0].credit_id,
            attempt,
        )
        .unwrap();
        assert!(matches!(result, ResetOutcome::NothingToReset));
        server.join().unwrap();
    }

    #[test]
    fn only_verified_available_credits_can_be_spent() {
        let reading = json!({"accountId":"A","rateLimitResetCredits":{"availableCount":2,"credits":[
            {"id":"first","status":"available","resetType":"codexRateLimits"},
            {"id":"second","status":"available","resetType":"codexRateLimits"},
            {"id":"expired","status":"available","resetType":"codexRateLimits","expiresAt":1},
            {"id":"used","status":"redeemed","resetType":"codexRateLimits"}
        ]}});
        assert!(available(&reading, "B").is_err());
        let credits = available(&reading, "A").unwrap();
        let attempt = uuid::Uuid::new_v4();
        for id in ["expired", "used", "other", ""] {
            assert!(
                redeem(&credits, Some(id), attempt, |_| panic!(
                    "unexpected redemption"
                ))
                .is_err()
            );
        }
        let result = redeem(&credits, Some("second"), attempt, |params| {
            assert_eq!(
                params,
                json!({"creditId":"second","idempotencyKey":attempt.to_string()})
            );
            Ok(json!({"outcome":"nothingToReset"}))
        })
        .unwrap();
        assert!(matches!(result, ResetOutcome::NothingToReset));
        assert!(redeem(&credits, None, attempt, |_| panic!("missing named credit")).is_err());
        let count_only = available(
            &json!({"accountId":"A","rateLimitResetCredits":{"availableCount":1}}),
            "A",
        )
        .unwrap();
        let result = redeem(&count_only, None, attempt, |params| {
            assert!(params["creditId"].is_null());
            assert_eq!(params["idempotencyKey"], attempt.to_string());
            Ok(json!({"outcome":"reset"}))
        })
        .unwrap();
        assert!(matches!(result, ResetOutcome::Reset));
        assert!(available(&json!({"accountId":"A"}), "A").is_err());
    }
}
