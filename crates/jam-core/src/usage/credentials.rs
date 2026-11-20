use super::{Profile, Provider, digest};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;
use std::{io::Read, path::Path};

// Credentials never implement Debug or Serialize and never cross the daemon socket.
pub(super) struct Credentials {
    pub token: String,
    pub account_header: Option<String>,
    pub account_key: Option<String>,
    pub label: Option<String>,
}

fn read(path: &Path) -> Result<Value, String> {
    let file = std::fs::File::open(path)
        .map_err(|_| "Sign in to this agent profile to read usage".to_string())?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read this agent login".to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Agent login file is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Cannot read this agent login".into())
}

fn claims(token: &str) -> Value {
    token
        .split('.')
        .nth(1)
        .and_then(|part| URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

pub(super) fn load(profile: &Profile, allow_prompt: bool) -> Result<Credentials, String> {
    #[cfg(not(target_os = "macos"))]
    let _ = allow_prompt;
    if profile.api_key {
        return Err("Usage windows are unavailable for API-key authentication".into());
    }
    match profile.provider {
        Provider::Codex => codex(read(&profile.directory.join("auth.json"))?),
        Provider::Claude => {
            let token = if let Some(token) = &profile.token {
                token.clone()
            } else {
                let path = profile.directory.join(".credentials.json");
                let value = if path.is_file() {
                    read(&path)?
                } else {
                    #[cfg(target_os = "macos")]
                    {
                        let default = profile.home.join(".claude");
                        let service = if profile.directory == default {
                            "Claude Code-credentials".into()
                        } else {
                            format!(
                                "Claude Code-credentials-{}",
                                &digest(profile.directory.to_string_lossy().trim_end_matches('/'))
                                    [..8]
                            )
                        };
                        let bytes = keychain(&service, allow_prompt)?;
                        serde_json::from_slice(&bytes)
                            .map_err(|_| "Cannot read Claude login".to_string())?
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        return Err("Sign in to Claude on this machine to read usage".into());
                    }
                };
                value["claudeAiOauth"]["accessToken"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Sign in to Claude on this machine to read usage")?
                    .to_owned()
            };
            Ok(Credentials {
                token,
                account_header: None,
                account_key: None,
                label: None,
            })
        }
    }
}

fn codex(value: Value) -> Result<Credentials, String> {
    if value["auth_mode"].as_str().is_some_and(|m| m != "chatgpt") {
        return Err("Usage windows are unavailable for API-key authentication".into());
    }
    let token = value["tokens"]["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Sign in to Codex with ChatGPT to read usage")?
        .to_owned();
    let access = claims(&token);
    let id = value["tokens"]["id_token"]
        .as_str()
        .map(claims)
        .unwrap_or(Value::Null);
    let account = value["tokens"]["account_id"]
        .as_str()
        .or_else(|| access["https://api.openai.com/auth"]["chatgpt_account_id"].as_str());
    let subject = access["sub"].as_str().or_else(|| id["sub"].as_str());
    let key = account
        .zip(subject)
        .map(|(a, s)| digest(&format!("codex:{a}:{s}")));
    let label = id["email"]
        .as_str()
        .or_else(|| access["email"].as_str())
        .or_else(|| access["https://api.openai.com/profile"]["email"].as_str())
        .map(str::to_owned);
    Ok(Credentials {
        token,
        account_header: account.map(str::to_owned),
        account_key: key,
        label,
    })
}

#[cfg(target_os = "macos")]
fn keychain(service: &str, allow_prompt: bool) -> Result<Vec<u8>, String> {
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        data::CFData,
        dictionary::CFDictionary,
        string::CFString,
    };
    use security_framework_sys::{item::*, keychain_item::SecItemCopyMatching};
    unsafe {
        let string = |p| CFString::wrap_under_get_rule(p).as_CFType();
        let mut pairs = vec![
            (string(kSecClass), string(kSecClassGenericPassword)),
            (string(kSecAttrService), CFString::new(service).as_CFType()),
            (string(kSecReturnData), CFBoolean::true_value().as_CFType()),
        ];
        if !allow_prompt {
            pairs.push((
                string(kSecUseAuthenticationUI),
                string(kSecUseAuthenticationUISkip),
            ));
        }
        let query = CFDictionary::from_CFType_pairs(&pairs);
        let mut result = std::ptr::null();
        let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result);
        if status != 0 || result.is_null() {
            return Err("Claude login is missing or Keychain access is unavailable".into());
        }
        let result = CFType::wrap_under_create_rule(result);
        result
            .downcast::<CFData>()
            .map(|data| data.bytes().to_vec())
            .ok_or_else(|| "Cannot read Claude login".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn token(value: Value) -> String {
        format!(
            "header.{}.signature",
            URL_SAFE_NO_PAD.encode(value.to_string())
        )
    }
    #[test]
    fn account_identity_does_not_follow_token_rotation() {
        let auth = |nonce| json!({"auth_mode":"chatgpt","tokens":{"account_id":"team","access_token":token(json!({"sub":"user","nonce":nonce})),"id_token":token(json!({"sub":"user","email":"a@example.invalid"}))}});
        let one = codex(auth(1)).unwrap();
        let two = codex(auth(2)).unwrap();
        assert_eq!(one.account_key, two.account_key);
        assert_eq!(one.label.as_deref(), Some("a@example.invalid"));
        assert!(codex(json!({"auth_mode":"apikey"})).is_err());
        assert!(
            codex(json!({"tokens":{"access_token":"opaque"}}))
                .unwrap()
                .account_key
                .is_none()
        );
    }
    #[test]
    fn explicit_profile_is_read_without_falling_back_to_default_login() {
        let temp = tempfile::tempdir().unwrap();
        let profile = Profile::new(
            Provider::Codex,
            temp.path().join("missing"),
            temp.path().into(),
            None,
            false,
        );
        assert!(load(&profile, false).is_err());
        let path = temp.path().join("work");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("auth.json"),
            json!({"auth_mode":"chatgpt","tokens":{"access_token":"test-only"}}).to_string(),
        )
        .unwrap();
        let profile = Profile::new(Provider::Codex, path, temp.path().into(), None, false);
        assert_eq!(load(&profile, false).unwrap().token, "test-only");
    }
}
