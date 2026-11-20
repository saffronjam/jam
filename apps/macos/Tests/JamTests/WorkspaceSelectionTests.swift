import Foundation

struct WorkspaceSelectionTests {
  static func run() throws {
    let remote = "remote"
    let local = "local-workspace"
    let remoteWorkspace = "remote|workspace"
    // Selecting an empty/offline remote must survive repeated local polls.
    var selected = ""
    for _ in 0..<20 {
      selected = WorkspaceSelection.reconcile(
        selected: selected, machine: remote, refreshed: "", workspaces: [local])
      assert(selected.isEmpty)
    }
    selected = WorkspaceSelection.reconcile(
      selected: selected, machine: remote, refreshed: remote, workspaces: [local])
    assert(selected.isEmpty)
    selected = WorkspaceSelection.reconcile(
      selected: selected, machine: remote, refreshed: remote, workspaces: [local, remoteWorkspace])
    assert(selected == remoteWorkspace)
    assert(
      WorkspaceSelection.reconcile(
        selected: selected, machine: remote, refreshed: "", workspaces: [remoteWorkspace])
        == selected)
    // Removing the last workspace keeps the selected machine, not a workspace on another host.
    assert(
      WorkspaceSelection.reconcile(
        selected: selected, machine: remote, refreshed: remote, workspaces: [local]
      ).isEmpty)
    assert(WorkspaceSelection.restoredMachine(workspace: "", machine: remote) == remote)
    assert(WorkspaceSelection.restoredMachine(workspace: local, machine: remote).isEmpty)
    // Local and remote refreshes produce the same sidebar order.
    for ids in [[remoteWorkspace, local], [local, remoteWorkspace]] {
      assert(
        WorkspaceSelection.ordered(ids, machines: ["", remote], id: { $0 }) == [
          local, remoteWorkspace,
        ])
    }
    print(
      "Workspace selection tests passed: empty remotes, refresh isolation, restoration, and stable ordering."
    )
  }
}
