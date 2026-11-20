# Coordinator design

Status: initial implementation present. Dedicated coordinator sidebar entries, durable tasks/inboxes, role instructions, provider-hook registration, and conversation resume are implemented. Real-provider end-to-end delivery and provider-version compatibility remain validation requirements; do not treat transport acceptance as acknowledgement.

A coordinator is a role on an agent session, presented as a **Coordinator** child under its workspace or worktree. Selecting that child shows one dedicated pane without a tab bar. Ordinary workspace tabs and their saved selection remain independent. Workspace coordinators delegate to worktree coordinators where present, or directly to assigned agents otherwise. They can create worktrees and subordinate coordinators for larger tasks. Each task and MR has one responsible coordinator; reviews, implementation, and video work can be delegated without transferring that responsibility.

Instruction files are personal, additive context:

| File | Applies to |
| --- | --- |
| `~/.config/jam/instructions/all.md` | All of the user's Jam agents |
| `~/.config/jam/instructions/coordinator.md` | The user's coordinators |
| `<git-common-dir>/jam/instructions/all.md` | Jam agents in this repository |
| `<git-common-dir>/jam/instructions/coordinator.md` | Coordinators in this repository |

Resolve `git-common-dir` with `git rev-parse --path-format=absolute --git-common-dir` so linked worktrees share repository instructions. Files in this directory stay local to the clone; they are not transferred by Git or automatically shared with a remote clone. There is no tracked `.jam/` directory in this design.

Keep reusable repository procedures in existing skills. Coordinator instructions describe when and to whom to delegate those procedures; they do not duplicate the procedures or add Jam requirements to the repository's general agent guidance. Existing repository authorization rules still apply. Task authorization must state whether delivery includes publishing an MR, starting environments, or merging.

Provide **Show effective instructions** for each agent. Show every contributing file, its scope, and the task-specific context, so users can inspect why a coordinator knows a workflow and an ordinary worker does not. Role changes must update context explicitly; changing directory must not silently turn a worker into a coordinator.

Use a durable task inbox with sender/recipient identity and reply relationships. Do not deliver delegation by injecting keystrokes into a terminal. Unavailable coordinators retain queued work. Closing an agent session does not silently replay its previous commands.
