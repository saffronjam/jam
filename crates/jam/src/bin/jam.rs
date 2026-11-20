use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jam_core::*;
#[derive(Parser)]
#[command(version, about = "Persistent terminal workspaces")]
struct Args {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    remote: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Install the newest stable Jam release and restart managed sessions.
    #[command(alias = "update")]
    Upgrade {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long, conflicts_with = "check")]
        recover: bool,
    },
    /// Install the release CLI symlink in ~/.local/bin.
    InstallCli,
    #[command(name = "__upgrade-apply", hide = true)]
    UpgradeApply { journal: std::path::PathBuf },
    #[command(name = "__upgrade-recover", hide = true)]
    UpgradeRecover { journal: std::path::PathBuf },
    #[command(name = "__upgrade-restore", hide = true)]
    UpgradeRestore {
        journal: std::path::PathBuf,
        #[arg(long)]
        local_only: bool,
        #[arg(long)]
        preserve_intent: bool,
    },
    #[command(subcommand)]
    Coordinate(CoordinateCmd),
    #[command(subcommand)]
    Remote(RemoteCmd),
    #[command(subcommand)]
    Workspace(WorkspaceCmd),
    #[command(subcommand)]
    Worktree(WorktreeCmd),
    #[command(subcommand)]
    Tab(TabCmd),
    #[command(subcommand)]
    Pane(PaneCmd),
    #[command(subcommand)]
    Status(StatusCmd),
    #[command(subcommand)]
    Skill(SkillCmd),
    #[command(subcommand)]
    Daemon(DaemonCmd),
}
#[derive(Subcommand)]
enum CoordinateCmd {
    Run {
        agent: String,
    },
    List {
        /// Include archived tasks, agents, and messages.
        #[arg(long)]
        all: bool,
    },
    Models {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        refresh: bool,
    },
    Start {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value = "coordinator", value_parser = ["coordinator", "worker", "reviewer"])]
        role: String,
        #[arg(long)]
        complex: bool,
        #[arg(long)]
        tab_name: Option<String>,
        #[arg(long)]
        parent: Option<String>,
    },
    Task {
        #[arg(long)]
        owner: String,
        objective: String,
        #[arg(long)]
        review_of: Option<String>,
    },
    Assign {
        #[arg(long)]
        task: String,
        #[arg(long)]
        agent: String,
    },
    Result {
        #[arg(long)]
        task: String,
        #[arg(long)]
        agent: Option<String>,
        text: String,
    },
    Complete {
        #[arg(long)]
        task: String,
        #[arg(long)]
        agent: Option<String>,
        text: String,
    },
    Cancel {
        #[arg(long)]
        task: String,
        #[arg(long)]
        agent: Option<String>,
    },
    Send {
        #[arg(long)]
        to: String,
        #[arg(long)]
        agent: Option<String>,
        text: String,
    },
    Inbox {
        #[arg(long)]
        agent: Option<String>,
    },
    Ack {
        message: String,
        #[arg(long)]
        agent: Option<String>,
    },
    Instructions {
        #[arg(long)]
        agent: Option<String>,
    },
    Pause {
        agent: String,
    },
    Resume {
        agent: String,
    },
}
fn current_agent(a: Option<String>) -> Result<String> {
    a.or_else(|| std::env::var("JAM_AGENT_ID").ok())
        .or_else(|| {
            let pane = std::env::var("JAM_PANE_ID").ok()?;
            let Message::Coordination(board) = call(Request::Coordination(
                serde_json::json!({"action":"list","include_archived":true}),
            ))
            .ok()?
            else {
                return None;
            };
            board["agents"]
                .as_array()?
                .iter()
                .find(|a| a["pane"] == pane)?["id"]
                .as_str()
                .map(str::to_owned)
        })
        .context("Supply --agent or run inside a managed Jam agent")
}
#[derive(Subcommand)]
enum RemoteCmd {
    List,
    Add {
        name: String,
        destination: String,
    },
    Remove {
        id: String,
    },
    Install {
        id: String,
        archive: std::path::PathBuf,
    },
}
#[derive(clap::Args)]
struct InitialTabArgs {
    /// Start this agent as the first tab, without an extra shell.
    #[arg(long, value_parser = ["claude", "codex"])]
    agent: Option<String>,
    #[arg(long, requires = "agent", value_parser = ["worker", "coordinator", "reviewer"])]
    role: Option<String>,
    #[arg(long, requires = "agent")]
    complex: bool,
    #[arg(long, requires = "agent")]
    parent: Option<String>,
    /// Name the first terminal or agent tab.
    #[arg(long)]
    tab_name: Option<String>,
}
impl InitialTabArgs {
    fn into_tab(self) -> Option<InitialTab> {
        if let Some(provider) = self.agent {
            Some(InitialTab::Agent {
                provider,
                role: self.role.unwrap_or_else(|| "worker".into()),
                complex: self.complex,
                parent: self.parent,
                name: self.tab_name,
            })
        } else {
            self.tab_name
                .map(|name| InitialTab::Terminal { name: Some(name) })
        }
    }
}
#[derive(Subcommand)]
enum WorkspaceCmd {
    List,
    Close {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        group: bool,
    },
    Create {
        path: String,
        #[arg(long, default_value = "")]
        name: String,
        #[command(flatten)]
        initial: InitialTabArgs,
    },
}
#[derive(Subcommand)]
enum WorktreeCmd {
    List {
        #[arg(long)]
        workspace: String,
    },
    Create {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        branch: String,
        #[arg(long, default_value = "HEAD")]
        base: String,
        #[arg(long)]
        path: Option<String>,
        #[command(flatten)]
        initial: InitialTabArgs,
    },
    Open {
        path: String,
        #[arg(long)]
        workspace: Option<String>,
    },
    Remove {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        force: bool,
    },
}
#[derive(Subcommand)]
enum TabCmd {
    New {
        #[arg(long)]
        workspace: String,
    },
}
#[derive(Subcommand)]
enum PaneCmd {
    List,
    Split {
        #[arg(long)]
        pane: Option<String>,
        #[arg(long,default_value="right",value_parser=["right","down"])]
        direction: String,
    },
    Send {
        #[arg(long)]
        pane: Option<String>,
        #[arg(long)]
        text: String,
        #[arg(long)]
        enter: bool,
    },
    Capture {
        #[arg(long)]
        pane: Option<String>,
    },
    Close {
        #[arg(long)]
        pane: Option<String>,
    },
}
#[derive(Subcommand)]
enum StatusCmd {
    List,
    Set {
        #[arg(long)]
        pane: Option<String>,
        #[arg(long)]
        state: String,
    },
}
#[derive(Subcommand)]
enum SkillCmd {
    Show,
    Context {
        #[arg(long)]
        agent: Option<String>,
    },
    Setup {
        #[arg(long, value_parser = ["claude", "codex"])]
        agent: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Status {
        #[arg(long, value_parser = ["claude", "codex"])]
        agent: Vec<String>,
    },
    Remove {
        #[arg(long, value_parser = ["claude", "codex"])]
        agent: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
}
#[derive(Subcommand)]
enum DaemonCmd {
    Info,
    PrepareUpgrade {
        #[arg(long)]
        approved_json: Option<String>,
    },
    AbortUpgrade {
        token: String,
    },
    StopUpgrade {
        token: String,
    },
    CommitUpgrade {
        token: String,
    },
    Status,
    Stop,
    Run,
}
fn target(p: Option<String>) -> Result<String> {
    p.or_else(|| std::env::var("JAM_PANE_ID").ok())
        .context("supply --pane or run inside a Jam pane")
}
fn main() -> Result<()> {
    let a = Args::parse();
    if a.remote.is_some()
        && matches!(
            a.command,
            Command::Upgrade { .. }
                | Command::InstallCli
                | Command::UpgradeApply { .. }
                | Command::UpgradeRestore { .. }
                | Command::UpgradeRecover { .. }
        )
    {
        anyhow::bail!(
            "Run upgrade on the installation's host; --remote is not supported for this command"
        );
    }
    match &a.command {
        Command::Upgrade {
            check,
            yes,
            recover,
        } => {
            return if *recover {
                upgrade::recover_cli(*yes, a.json)
            } else {
                upgrade::cli(*check, *yes, a.json)
            };
        }
        Command::UpgradeRecover { journal } => return upgrade::crash_recovery::recover(journal),
        Command::InstallCli => {
            let path = upgrade::install_cli()?;
            if a.json {
                println!("{}", serde_json::json!({"path":path}));
            } else {
                println!("Installed {}", path.display());
            }
            return Ok(());
        }
        Command::UpgradeApply { journal } => return upgrade::apply(journal),
        Command::UpgradeRestore {
            journal,
            local_only,
            preserve_intent,
        } => {
            return if *preserve_intent {
                upgrade::restore_preserving_intent(journal)
            } else if *local_only {
                upgrade::restore_local(journal)
            } else {
                upgrade::restore(journal)
            };
        }
        _ => {}
    }
    if let Command::Coordinate(CoordinateCmd::Run { ref agent }) = a.command {
        return coordination::run(agent);
    }

    if let Command::Skill(command) = &a.command {
        if a.remote.is_some() {
            anyhow::bail!("Run skill setup in a terminal on the target machine");
        }
        match command {
            SkillCmd::Show => print!("{}", include_str!("../../../../skills/jam/SKILL.md")),
            SkillCmd::Context { agent } => {
                if !config::agent_enabled(agent.as_deref().unwrap_or("claude")) {
                    return Ok(());
                }
                let input = coordination::read_hook_input();
                let registered = coordination::hook(&input, agent.as_deref().unwrap_or("claude"));
                let base = agents::session_context(
                    std::env::var("JAM_PANE_ID").ok().as_deref(),
                    std::env::var("JAM_SOCKET").ok().as_deref(),
                );
                let text = [base, registered]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("\n");
                match input["hook_event_name"].as_str() {
                    Some("Stop") => println!("{}", serde_json::json!({"continue":true})),
                    Some("UserPromptSubmit") => println!(
                        "{}",
                        serde_json::json!({"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":text}})
                    ),
                    Some("SessionStart") | None => print!("{text}"),
                    _ => {}
                }
            }
            _ => {
                let (action, agent, dry_run) = match command {
                    SkillCmd::Setup { agent, dry_run } => (agents::Action::Setup, agent, *dry_run),
                    SkillCmd::Remove { agent, dry_run } => {
                        (agents::Action::Remove, agent, *dry_run)
                    }
                    SkillCmd::Status { agent } => (agents::Action::Status, agent, false),
                    _ => unreachable!(),
                };
                let selected: Vec<_> = agent
                    .iter()
                    .map(|name| {
                        if name == "claude" {
                            agents::Agent::Claude
                        } else {
                            agents::Agent::Codex
                        }
                    })
                    .collect();
                let reports = agents::manage(
                    action,
                    &selected,
                    &agents::Locations::environment()?,
                    dry_run,
                )?;
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&reports)?);
                } else {
                    for report in reports {
                        println!(
                            "{}: skill {}, hook {}{}\n  {}\n  {}",
                            report.agent.name(),
                            report.skill,
                            report.hook,
                            if report.changed {
                                if dry_run {
                                    " (would change)"
                                } else {
                                    " (updated)"
                                }
                            } else {
                                ""
                            },
                            report.skill_path.display(),
                            report.hook_path.display()
                        );
                    }
                    if action == agents::Action::Setup {
                        println!(
                            "Start a new agent session. Approve Jam's hook in Codex with /hooks; Claude Code requires a trusted folder."
                        );
                    }
                }
            }
        }
        return Ok(());
    }
    if let Command::Remote(command) = a.command {
        let result = match command {
            RemoteCmd::List => serde_json::to_value(remote::list()?)?,
            RemoteCmd::Add { name, destination } => {
                serde_json::to_value(remote::add(name, destination)?)?
            }
            RemoteCmd::Remove { id } => {
                remote::remove(&id)?;
                serde_json::json!({"ok":true})
            }
            RemoteCmd::Install { id, archive } => {
                remote::install(&id, &archive)?;
                serde_json::json!({"ok":true})
            }
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    struct Disconnect(Option<String>);
    impl Drop for Disconnect {
        fn drop(&mut self) {
            if let Some(id) = &self.0 {
                remote::disconnect(id);
            }
        }
    }
    let mut guard = Disconnect(None);
    let endpoint = if let Some(name) = a.remote {
        let entry = remote::list()?
            .into_iter()
            .find(|r| r.id == name || r.name == name)
            .context("Remote not found")?;
        guard.0 = Some(entry.id.clone());
        remote::connect(&entry.id)?
    } else {
        socket()
    };
    let r = match a.command {
        Command::Coordinate(CoordinateCmd::Models { provider, refresh }) => {
            Request::AgentModels { provider, refresh }
        }
        Command::Coordinate(cmd) => {
            use CoordinateCmd::*;
            let mut value = match cmd {
                Run { .. } | Models { .. } => unreachable!(),
                List { all } => serde_json::json!({"action":"list","include_archived":all}),
                Start {
                    workspace,
                    provider,
                    role,
                    parent,
                    complex,
                    tab_name,
                } => {
                    serde_json::json!({"action":"start","workspace":workspace,"provider":provider,"role":role,"parent":parent,"complex":complex,"tab_name":tab_name})
                }
                Task {
                    owner,
                    objective,
                    review_of,
                } => {
                    serde_json::json!({"action":"task","owner":owner,"objective":objective,"review_of":review_of})
                }
                Assign { task, agent } => {
                    serde_json::json!({"action":"assign","task":task,"agent":agent})
                }
                Result { task, agent, text } => {
                    serde_json::json!({"action":"result","task":task,"agent":current_agent(agent)?,"text":text})
                }
                Complete { task, agent, text } => {
                    serde_json::json!({"action":"complete","task":task,"agent":current_agent(agent)?,"text":text})
                }
                Cancel { task, agent } => {
                    serde_json::json!({"action":"cancel","task":task,"agent":current_agent(agent)?})
                }
                Send { to, agent, text } => {
                    serde_json::json!({"action":"send","to":to,"agent":current_agent(agent)?,"text":text})
                }
                Inbox { agent } => {
                    serde_json::json!({"action":"inbox","agent":current_agent(agent)?})
                }
                Ack { message, agent } => {
                    serde_json::json!({"action":"ack","message":message,"agent":current_agent(agent)?})
                }
                Instructions { agent } => {
                    serde_json::json!({"action":"instructions","agent":current_agent(agent)?})
                }
                Pause { agent } => serde_json::json!({"action":"pause","agent":agent}),
                Resume { agent } => serde_json::json!({"action":"resume","agent":agent}),
            };
            value["operation"] = serde_json::json!(id());
            Request::Coordination(value)
        }
        Command::Remote(_) => unreachable!(),
        Command::Workspace(WorkspaceCmd::Close { workspace, group }) => {
            Request::WorkspaceClose { workspace, group }
        }
        Command::Worktree(command) => match command {
            WorktreeCmd::List { workspace } => Request::Worktrees { workspace },
            WorktreeCmd::Create {
                workspace,
                branch,
                base,
                path,
                initial,
            } => match initial.into_tab() {
                Some(tab) => Request::CreateWorkspaceWithTab {
                    location: WorkspaceLocation::Worktree {
                        workspace,
                        branch,
                        base,
                        path,
                    },
                    tab,
                },
                None => Request::WorktreeCreate {
                    workspace,
                    branch,
                    base,
                    path,
                },
            },
            WorktreeCmd::Open { path, workspace } => Request::WorktreeOpen {
                path,
                workspace: workspace.or_else(|| std::env::var("JAM_WORKSPACE_ID").ok()),
            },
            WorktreeCmd::Remove { workspace, force } => {
                Request::WorktreeRemove { workspace, force }
            }
        },
        Command::Workspace(WorkspaceCmd::List)
        | Command::Pane(PaneCmd::List)
        | Command::Status(StatusCmd::List)
        | Command::Daemon(DaemonCmd::Status) => Request::State,
        Command::Workspace(WorkspaceCmd::Create {
            path,
            name,
            initial,
        }) => match initial.into_tab() {
            Some(tab) => Request::CreateWorkspaceWithTab {
                location: WorkspaceLocation::Directory { path, name },
                tab,
            },
            None => Request::CreateWorkspace { path, name },
        },
        Command::Tab(TabCmd::New { workspace }) => Request::NewTab { workspace },
        Command::Pane(PaneCmd::Split { pane, direction }) => Request::Split {
            pane: target(pane)?,
            vertical: direction == "down",
        },
        Command::Pane(PaneCmd::Send { pane, text, enter }) => {
            let mut data = text.into_bytes();
            if enter {
                data.push(b'\r')
            }
            Request::Send {
                pane: target(pane)?,
                data,
            }
        }
        Command::Pane(PaneCmd::Capture { pane }) => Request::Capture {
            pane: target(pane)?,
        },
        Command::Pane(PaneCmd::Close { pane }) => Request::Close {
            pane: target(pane)?,
        },
        Command::Status(StatusCmd::Set { pane, state }) => Request::Status {
            pane: target(pane)?,
            state,
        },
        Command::Daemon(DaemonCmd::Stop) => {
            let reply = if guard.0.is_none() {
                stop_local_at(&endpoint)?
            } else {
                stop_at(&endpoint)?
            };
            println!("{}", serde_json::to_string(&reply)?);
            return Ok(());
        }
        Command::Daemon(DaemonCmd::Info) => Request::HostInfo,
        Command::Daemon(DaemonCmd::PrepareUpgrade { approved_json }) => {
            if let Some(json) = approved_json {
                let request: Request = serde_json::from_str(&json)?;
                anyhow::ensure!(
                    matches!(request, Request::PrepareUpgradeApproved { .. }),
                    "Invalid approved upgrade request"
                );
                request
            } else {
                Request::PrepareUpgrade
            }
        }
        Command::Daemon(DaemonCmd::AbortUpgrade { token }) => Request::AbortUpgrade { token },
        Command::Daemon(DaemonCmd::StopUpgrade { token }) => Request::StopUpgrade { token },
        Command::Daemon(DaemonCmd::CommitUpgrade { token }) => Request::CommitUpgrade { token },
        Command::Daemon(DaemonCmd::Run) => {
            if guard.0.is_some() {
                anyhow::bail!("Use remote setup to start a remote daemon");
            }
            return jam_core::server::serve();
        }
        Command::Skill(_)
        | Command::Upgrade { .. }
        | Command::InstallCli
        | Command::UpgradeApply { .. }
        | Command::UpgradeRestore { .. }
        | Command::UpgradeRecover { .. } => unreachable!(),
    };
    let needs_models = match &r {
        Request::AgentModels { .. } => true,
        Request::Coordination(v) => v["role"] == "reviewer" || v["complex"] == true,
        Request::CreateWorkspaceWithTab {
            tab: InitialTab::Agent { role, complex, .. },
            ..
        } => role == "reviewer" || *complex,
        _ => false,
    };
    if needs_models {
        let supported = matches!(call_at(&endpoint, Request::Capabilities)?, Message::Capabilities(c) if c.iter().any(|v| v == "coordination_models"));
        if !supported {
            anyhow::bail!(
                "Update this machine's server to use coordination models and reviewer roles"
            );
        }
    }
    if matches!(&r, Request::CreateWorkspaceWithTab { .. }) {
        let Message::Capabilities(capabilities) = call_at(&endpoint, Request::Capabilities)? else {
            anyhow::bail!("Update this machine's daemon to create a workspace with its first tab");
        };
        if !capabilities.iter().any(|c| c == "workspace_initial_tab") {
            anyhow::bail!("Update this machine's daemon to create a workspace with its first tab");
        }
    }
    if matches!(
        &r,
        Request::WorktreeCreate { .. }
            | Request::WorktreeOpen {
                workspace: Some(_),
                ..
            }
    ) {
        let Message::Capabilities(capabilities) = call_at(&endpoint, Request::Capabilities)? else {
            anyhow::bail!("Daemon does not support workspace worktrees; update it first");
        };
        if !capabilities
            .iter()
            .any(|capability| capability == "workspace_containers")
        {
            anyhow::bail!("Update this machine's daemon to use workspace worktrees");
        }
    }
    let mut reply = call_at(&endpoint, r)?;
    while let Message::Operation {
        id, done: false, ..
    } = &reply
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
        reply = call_at(&endpoint, Request::Operation { id: id.clone() })?;
    }
    if let Message::Operation {
        error: Some(error), ..
    } = &reply
    {
        anyhow::bail!("{error}")
    }

    if !a.json
        && let Message::Capture(text) = reply
    {
        print!("{text}");
        return Ok(());
    }
    println!("{}", serde_json::to_string_pretty(&reply)?);
    Ok(())
}
