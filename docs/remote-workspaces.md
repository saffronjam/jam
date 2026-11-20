# Remote workspaces

Jam saves SSH destinations in the compact machine list at the bottom of the sidebar. Workspaces stay above it, grouped under collapsible machine names when at least two machines are connected. Connection lights belong to machines; workspace lights report agent activity. Add a machine with the plus above the machine list. Connect explicitly to permit native SSH authentication/host-key prompts; background retries never prompt. SSH configuration, agents and jump hosts are handled by system OpenSSH.

`just bundle` builds the app with both server architectures. `just remote-build` produces Linux x86-64 and ARM64 server archives. `just build` includes available archives in the development app. The server requires Linux with glibc 2.36 or newer, systemd user services, Python 3, tar, and Git for worktrees. The remote does not download dependencies or compile anything.

First connection installs a versioned server under `$XDG_DATA_HOME/jam/servers` (default `~/.local/share/jam/servers`) and enables `jamd.service` for the current user. A machine with lingering disabled can connect and run workspaces normally. The warning triangle opens an anchored explanation with an **Enable Persistence** action, which keeps its user service alive after the last logout. If host policy rejects that, an administrator must enable lingering. Jam does not use sudo or change shell startup files.

The SSH connection forwards a private local Unix socket to the remote user's private daemon socket. OpenSSH multiplexes terminal and control channels within that encrypted connection; Jam retains its length-prefixed MessagePack framing. GUI loss or SSH loss does not terminate the service or its children. No public TCP listener is created. The server CLI and skill path are exposed to shells through their environment.

The protocol compatibility tag pins both the Jam generation and Ghostty revision. Incompatible active servers are preserved. Idle upgrades must acquire the daemon's drain guard before stopping it; the guard rejects new mutations. Reboot/server failure retains layout only, not running processes or scrollback. Opening a stopped tab starts fresh shells, never previous commands. On a changed server incarnation, the already visible tab does the same after fresh state arrives; hidden tabs wait until opened. A transport reconnect to the same daemon does not relaunch deliberately exited shells.

Remote object references are qualified by the saved endpoint ID. Paths and names are never rewritten. Cross-machine tab/pane transfers are rejected. Pane sizes follow the last client to focus the pane; background mirrors cannot resize a pane owned by another client. Input is never replayed after reconnect.

CLI examples:

```sh
jam remote add lab dev@dev-server
jam remote list
jam remote install REMOTE_ID build/remote/x86_64.tar.gz
jam --remote lab workspace list
jam --remote lab daemon info
just test remote lab
```

The registry is stored alongside local Jam state as `remotes.json`. Removing an entry leaves the remote service and workspaces intact.

## Server actions

Right-click a machine in the bottom machine list for **Check for Updates**, **Update Server…**, and **Restart Server…**. Local has the same actions. Update detection compares server build IDs, including compatible older builds, and requires a matching bundled server artifact. The connection light continues to report connectivity rather than update status.

The attached confirmation sheet counts running terminals. **Update When Sessions End** waits until all terminal processes have exited, then obtains the server's atomic drain guard. It never treats an idle agent or shell prompt as a stopped terminal. The schedule is saved locally and checked while Jam is running, resuming when the app is reopened. It can be cancelled from the machine menu. **Update and Restart** and **Restart Server** explicitly end running terminals; layout is preserved, but commands are not replayed.

Remote updates verify the requested build and file checksums before stopping anything, activate an immutable build through an atomic symlink, restart the user service, and verify its response. A startup or health-check failure restores the previous executable. Restarting a remote uses its installed service; restarting Local launches the server bundled with the current app. No action enables lingering or changes SSH authentication policy.

## Binary compatibility

Release servers are built on Debian 12 for x86_64 and aarch64. Use Debian 12 or a compatible glibc 2.36+ system with systemd user services. Alpine/musl and non-systemd installations are outside the initial release targets.
