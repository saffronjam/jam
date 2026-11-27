<p align="center"><img src="assets/jam.svg" width="112" alt="Jam"></p>
<h1 align="center">Jam</h1>
<p align="center">A native macOS terminal for working with AI agents across projects and machines.</p>

![Jam with split terminals, a coordinator, and a worktree](docs/images/hero.png)

<sub>Claude implements terminal search while Codex reviews it and runs the tests. Quota values use sample data. Regenerate with `python3 docs/images/capture.py`.</sub>

Jam keeps terminals, Git worktrees, and agent conversations together. Work locally or connect to a Linux machine over SSH. Give a workspace coordinator a task, follow its workers, and keep the terminals within reach.

- Native macOS interface with GPU-rendered terminals, tabs, and split panes.
- Workspaces and worktrees with Git summaries and individual activity indicators.
- Claude and Codex integrations, usage reporting, and workspace or worktree coordinators.
- Durable task assignments and agent inboxes, with saved conversation recovery.
- Remote terminals backed by a user-owned Linux daemon.

## Install

Initial releases target **Apple Silicon Macs with macOS 14 or later**. Remote binaries target Debian 12 or a compatible Linux system with glibc 2.36+, on x86_64 or aarch64. SSH, Python 3, and systemd user services are required.

Download the app from [GitHub Releases](https://github.com/saffronjam/jam/releases), unzip it, and place `Jam.app` in `~/Applications`. Initial builds are ad-hoc signed, not Apple-notarized. macOS may require explicitly approving the downloaded app in **System Settings → Privacy & Security**.

Install the CLI with `~/Applications/Jam.app/Contents/Resources/bin/jam install-cli`, then ensure `~/.local/bin` is on your PATH.

## Upgrades

When an update is found, select **Upgrade available** in the top toolbar. Use **Jam → Check for Updates…** to check manually, or turn automatic checks off in **Settings → General**.

Run `jam upgrade` to check, download, and apply the latest stable release. Jam asks before interrupting active sessions. `jam update` is an alias; `jam upgrade --check --json` is available for scripts. See [upgrade behavior](docs/upgrading.md).

## Agents

Install and sign in to Claude Code or Codex separately. Jam detects integrations on each machine; enable the ones you want in Settings. Ordinary terminal commands remain available regardless of integration settings.

Right-click a workspace or worktree to add a coordinator. Coordinators have a dedicated sidebar entry and can delegate work into separate worktrees. New features use a worktree unless you explicitly request the existing checkout. Publishing, pushing, and merging follow your instructions.

Read [coordinator behavior](docs/coordinators.md) for roles, instructions, task delivery, and recovery. Agent usage may require granting Jam access to the provider's credentials in Keychain.

## File previews

Control-click a file path in terminal output to preview it beside the terminal. The expand button opens a larger view. Markdown opens formatted with tables and Mermaid diagrams. Choose **Raw** to read the source. A link to a specific line opens in Raw view, as do Markdown files over 2 MiB.

MP4 and MOV files play with macOS controls and keep their playback position when expanded. Jam reads the requested ranges from the machine that owns the pane. **Settings → Integrations** shows which file previews are available for each machine.

## Remote machines

Add a machine through the sidebar. Jam uses SSH and installs a server under your Linux user account. No root daemon is required. Session persistence between logins depends on systemd lingering; Jam explains when it needs enabling.

Saved layouts and agent conversations can be restored after a server restart. Ordinary running shell processes do not survive that restart. See [remote workspaces](docs/remote-workspaces.md).

## Build

Install Python 3.11 or later, Xcode 26.5 (or Command Line Tools with macOS SDK 26.5), Rust through rustup, and [just](https://github.com/casey/just). The repository pins Rust and downloads a checksum-verified Zig toolchain into `.cache`.

Local and CI builds share the SDK pin in [`scripts/macos-toolchain.json`](scripts/macos-toolchain.json). Both check and release workflows select that Xcode explicitly. Builds verify the SDK before compiling and check the app executable before packaging, so an older SDK cannot silently restore the legacy sidebar and colors. The deployment target remains macOS 14; the build SDK does not raise the minimum supported OS.

```sh
just dev           # Build and launch the macOS app
just test core     # Rust, Swift logic, installer and snapshot checks
just check         # Full local checks, including native UI tests
just remote-build  # Build both Linux server architectures using Docker
just bundle        # Package the app with its remote servers
```

Developer binaries and caches stay inside the checkout. Release builds must include both Linux server archives and all dependency notices.

The standalone [desktop acceptance suite](e2e/README.md) drives installed applications through native UI automation on macOS and Linux. Both platforms collect the same scenarios; a missing Linux client remains an ordinary test failure.

## Architecture

A Rust daemon owns terminal sessions. A shared renderer draws terminal contents; Swift provides the macOS interface. The CLI and app use the same daemon protocol.

[Architecture](docs/architecture.md) · [Protocol](docs/protocol.md) · [Dependency licenses](docs/dependencies.md)

## License

Jam's original code and artwork are [MIT licensed](LICENSE). Bundled components retain their own licenses. Claude, Codex, and other third-party names and marks belong to their respective owners.
