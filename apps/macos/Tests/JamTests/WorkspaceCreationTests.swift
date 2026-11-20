import Foundation

struct WorkspaceCreationTests {
  static func run() throws {
    let pending = WorkspaceCreation(
      endpoint: "remote", home: "/home/edge", previous: "remote|old",
      knownWorkspaceIDs: ["remote|old", "remote|other"])
    let available = ["local", "remote|old", "remote|other"]
    assert(WorkspaceSelection.endpoint(of: pending.id) == "remote")
    assert(pending.includesDuringCreation("remote|old"))
    assert(!pending.includesDuringCreation("remote|new"))
    assert(pending.includesDuringCreation("local"))
    assert(pending.includesDuringCreation("another-remote|new"))
    // A poll cannot deselect a workspace while its creation is still in flight.
    assert(
      WorkspaceSelection.reconcile(
        selected: pending.id, machine: "remote", refreshed: "remote",
        workspaces: available + [pending.id]) == pending.id)
    assert(
      pending.selection(after: "remote|new", selected: pending.id, available: available)
        == "remote|new")
    assert(
      pending.selection(after: nil, selected: pending.id, available: available) == "remote|old")
    // Success and failure both preserve navigation performed during the network wait.
    for created in [nil, Optional("remote|new")] {
      assert(pending.selection(after: created, selected: "local", available: available) == "local")
      assert(
        pending.selection(after: created, selected: "remote|other", available: available)
          == "remote|other")
    }
    assert(
      pending.selection(after: nil, selected: pending.id, available: ["local", "remote|other"])
        == "remote|other")
    assert(pending.selection(after: nil, selected: pending.id, available: ["local"]) == "")
    let local = WorkspaceCreation(endpoint: "", home: "/Users/test", previous: "local")
    assert(WorkspaceSelection.endpoint(of: local.id).isEmpty)
    assert(local.id != WorkspaceCreation(endpoint: "", home: local.home, previous: "local").id)
    print("Workspace creation checks passed: polling, completion, failure, and navigation.")
  }
}
