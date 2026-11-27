import AppKit

enum WorkspaceRemovalAction: Equatable {
  case close(group: Bool)
  case delete(force: Bool)

  var label: String {
    switch self {
    case .close: "Closing workspace"
    case .delete: "Deleting checkout"
    }
  }

  func request(workspace: String) -> [String: Any] {
    switch self {
    case .close(let group): ["WorkspaceClose": ["workspace": workspace, "group": group]]
    case .delete(let force): ["WorktreeRemove": ["workspace": workspace, "force": force]]
    }
  }
}

struct WorkspaceRemoval {
  let token = UUID()
  let action: WorkspaceRemovalAction
  var showsSpinner = false
}

extension Store {
  var currentRemoval: WorkspaceRemoval? {
    guard !workspaceRemovals.isEmpty else { return nil }
    return current.flatMap { workspaceRemovals[$0.id] }
  }

  func isWorkspacePending(_ id: String) -> Bool {
    isCreatingWorkspace(id) || workspaceRemovals[id] != nil
  }

  func beginWorkspaceRemoval(
    _ workspace: Workspace, action: WorkspaceRemovalAction,
    call: @escaping (Any, String) -> [String: Any] = { rpc($0, endpoint: $1) }
  ) {
    let endpoint = WorkspaceSelection.endpoint(of: workspace.id)
    guard !reorderingWorkspaces, !serverBusy.contains(endpoint),
      workspaces.contains(where: { $0.id == workspace.id })
    else { return }
    guard endpoint.isEmpty || remoteMessages[endpoint] == "Connected" else {
      error = "Remote is unavailable"
      return
    }
    let ids = workspaces.filter {
      $0.id == workspace.id || (action == .close(group: true) && $0.parent == workspace.id)
    }.map(\.id)
    guard !ids.contains(where: isWorkspacePending) else { return }
    let incarnation = serverIncarnations[endpoint]
    let removal = WorkspaceRemoval(action: action)
    workspaceRemovals.merge(ids.map { ($0, removal) }) { _, new in new }
    DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(300)) { [weak self] in
      guard let self, self.workspaceRemovals[workspace.id]?.token == removal.token else { return }
      var loading = removal
      loading.showsSpinner = true
      self.workspaceRemovals.merge(ids.map { ($0, loading) }) { _, new in new }
    }

    DispatchQueue.global(qos: .userInitiated).async {
      var reply = call(action.request(workspace: workspace.id), endpoint)
      while let op = reply["Operation"] as? [String: Any], op["done"] as? Bool == false,
        let id = op["id"] as? String
      {
        Thread.sleep(forTimeInterval: 0.1)
        reply = call(["Operation": ["id": id]], endpoint)
      }
      let op = reply["Operation"] as? [String: Any]
      let failure = reply["error"] as? String ?? op?["error"] as? String
      let state = reply["State"] as? [String: Any]
      let latest = state ?? (call("State", endpoint)["State"] as? [String: Any])
      let confirmed = state != nil || (op?["done"] as? Bool == true && failure == nil)
      DispatchQueue.main.async {
        guard self.workspaceRemovals[workspace.id]?.token == removal.token else { return }
        if let incarnation, self.serverIncarnations[endpoint] != incarnation {
          self.workspaceRemovals = self.workspaceRemovals.filter { $0.value.token != removal.token }
          self.refresh()
          return
        }
        if let latest { self.apply(latest, endpoint: endpoint) }
        // A successful removal can also reach us first in a state poll. If its
        // snapshot is still in flight, keep the panes hidden until it arrives.
        if self.workspaces.contains(where: { $0.id == workspace.id }), !confirmed {
          self.workspaceRemovals = self.workspaceRemovals.filter { $0.value.token != removal.token }
          if ids.contains(self.selected) {
            if !self.visiblePaneIDs.contains(self.focus) {
              self.focus = self.visiblePaneIDs.first ?? ""
            }
            self.focusTerminal()
          }
          if action == .delete(force: false), failure?.contains("local changes") == true {
            self.removeWorktree(workspace, force: true)
          } else {
            self.error = failure ?? "Could not confirm workspace removal"
          }
        }
        self.trimTerminalViews()
        self.refresh()
      }
    }
  }
}
