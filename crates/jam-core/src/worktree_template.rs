//! Worktree paths are templates, never shell expressions.
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

pub const DEFAULT: &str = "~/wt/{{repo}}/{{worktree}}";

fn substitute(template: &str, repo: &str, worktree: &str) -> Result<String> {
    let mut rest = template;
    let mut rendered = String::new();
    while let Some(start) = rest.find("{{") {
        let literal = &rest[..start];
        if literal.contains("}}") {
            bail!("Unexpected }} in worktree path template")
        }
        rendered.push_str(literal);
        rest = &rest[start + 2..];
        let end = rest
            .find("}}")
            .ok_or_else(|| anyhow::anyhow!("Unclosed variable in worktree path template"))?;
        let key = rest[..end].trim();
        rendered.push_str(match key {
            "repo" => repo,
            "worktree" | "wt" => worktree,
            _ => bail!(
                "Unknown worktree variable {{{{{key}}}}}. Use {{{{repo}}}} or {{{{worktree}}}}"
            ),
        });
        rest = &rest[end + 2..];
    }
    if rest.contains("}}") {
        bail!("Unexpected }} in worktree path template")
    }
    rendered.push_str(rest);
    Ok(rendered)
}
/// Old values specified a base directory. Expand those without changing their destinations.
pub fn normalize(template: &str) -> Result<String> {
    if template.is_empty() || template.len() > 4096 || template.chars().any(char::is_control) {
        bail!("Worktree path must be a nonempty, single-line path")
    }
    if !template.starts_with('/') && template != "~" && !template.starts_with("~/") {
        bail!("Worktree path must be absolute or start with ~/")
    }
    let template = if template.contains("{{") || template.contains("}}") {
        template.to_owned()
    } else {
        format!(
            "{}/{{{{repo}}}}/{{{{worktree}}}}",
            template.trim_end_matches('/')
        )
    };
    substitute(&template, "repository", "worktree")?;
    Ok(template)
}
pub fn render(template: &str, repo: &str, branch: &str, home: &Path) -> Result<PathBuf> {
    let worktree = branch.replace('/', "-");
    for (label, value) in [("repository", repo), ("worktree", worktree.as_str())] {
        if value.is_empty()
            || [".", ".."].contains(&value)
            || value.contains('/')
            || value.chars().any(char::is_control)
        {
            bail!("Invalid {label} name for a worktree path")
        }
    }
    let rendered = substitute(&normalize(template)?, repo, &worktree)?;
    let path = rendered
        .strip_prefix("~/")
        .map(|s| home.join(s))
        .unwrap_or_else(|| PathBuf::from(&rendered));
    if !path.is_absolute() {
        bail!("Worktree home directory must be absolute")
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands_names_once_and_preserves_legacy_paths() {
        let home = Path::new("/home/example");
        assert_eq!(
            render("~/wt", "my repo", "feature/login", home).unwrap(),
            home.join("wt/my repo/feature-login")
        );
        assert_eq!(
            render(
                "~/trees/{{worktree}}-{{repo}}",
                "jam",
                "feature/login",
                home
            )
            .unwrap(),
            home.join("trees/feature-login-jam")
        );
        assert_eq!(
            render(
                "/checkouts/{{ repo }}/{{wt}}",
                "{{worktree}}",
                "topic",
                home
            )
            .unwrap(),
            PathBuf::from("/checkouts/{{worktree}}/topic")
        );
        assert_eq!(
            normalize("/custom/").unwrap(),
            "/custom/{{repo}}/{{worktree}}"
        );
    }
    #[test]
    fn rejects_malformed_and_unknown_variables() {
        for value in [
            "",
            "relative/{{repo}}",
            "~/{{repo",
            "~/oops}}",
            "~/{{unknown}}",
            "~/{{}}",
            "~/{{repo}}\n",
        ] {
            assert!(normalize(value).is_err(), "accepted {value:?}");
        }
        assert!(render(DEFAULT, "..", "topic", Path::new("/home/test")).is_err());
    }
}
