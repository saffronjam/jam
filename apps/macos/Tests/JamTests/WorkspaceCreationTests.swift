import Foundation

struct WorkspaceCreationTests {
  static func run() throws {
    let pending = WorkspaceCreation(
      endpoint: "remote", previous: "remote|old", action: .new, name: "~", path: "/home/test")
    assert(WorkspaceSelection.endpoint(of: pending.id) == "remote")
    assert(WorkspaceSelection.endpoint(of: pending.proposedID) == "remote")
    assert(pending.proposedID != pending.id)
    assert(pending.action.request(home: nil) == nil)
    assert(
      (pending.action.request(home: "/home/test")?["NewWorkspace"] as? [String: String])?["path"]
        == "/home/test")
    let action = WorkspaceCreationAction.createWorktree(
      source: "remote|old", branch: "topic", base: "main", path: nil)
    let fields = action.request(home: nil)?["WorktreeCreate"] as? [String: Any]
    assert(fields?["workspace"] as? String == "remote|old" && fields?["path"] is NSNull)
    assert(
      WorkspaceSelection.reconcile(
        selected: pending.id, machine: "remote", refreshed: "remote",
        workspaces: ["remote|old", pending.id]) == pending.id)
    print("Workspace creation requests and pending selection passed.")
  }
}
