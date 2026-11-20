# 7. Workspaces own opened checkout sessions

Status: Accepted

A terminal's current directory is navigation context, not ownership. Deriving the sidebar tree from it would hide running worktrees on `cd`, and merge independent workspaces that visit the same repository.

Each opened checkout is a terminal workspace with a persistent `parent` pointing to its owning top-level workspace. The sidebar shows opened checkouts directly beneath their workspace when they belong to one repository. Repository headings appear only when that workspace owns checkouts from multiple repositories. Repository headings use the checkout's pinned Git identity. A workspace can own checkouts from several repositories. A checkout opened twice under one owner is reused; opening it in another workspace creates independent sessions in the shared directory.

The focused pane supplies live Git context. A bounded daemon worker caches discovery by directory, refreshes it periodically, and publishes it with state. Git discovery never runs on the UI thread or while holding the daemon state lock. A directory change can update the workspace name, branch, diff counts, and worktree actions without changing ownership.

Existing implicit Git groups migrate once, preserving workspace and pane IDs. Orphaned checkout groups receive an empty parent workspace. Subsequent launches retain explicit membership instead of regrouping by repository. Closing a workspace closes its owned sessions; deleting a checkout is separate and refuses while another workspace has it open.

The additive `workspace_containers` capability gates commands that require this model. Remote parent IDs are scoped like workspace IDs. Upgrading a running daemon remains explicit because it ends its shells.
