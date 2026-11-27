import SwiftUI

extension Store {
  func beginCreation(
    _ action: WorkspaceCreationAction, endpoint requested: String? = nil, branch: String? = nil
  ) {
    let endpoint = requested ?? currentEndpoint
    guard !serverBusy.contains(endpoint) else { return }
    if let existing = workspaceCreations.values.first(where: {
      $0.endpoint == endpoint && $0.action == action
    }) {
      select(existing.id)
      return
    }
    let home = endpoint.isEmpty ? NSHomeDirectory() : remoteHomes[endpoint]
    var parent: Workspace?
    var pendingBranch = branch
    let path: String
    switch action {
    case .new: path = home ?? "~"
    case .open(let directory): path = directory
    case .openWorktree(let source, let directory):
      parent = workspaces.first { $0.id == source }
      path = directory
    case .createWorktree(let source, let branch, _, let explicit):
      parent = workspaces.first { $0.id == source }
      pendingBranch = branch
      let repo = parent?.context?.mainPath ?? parent?.mainPath ?? parent?.path ?? ""
      path =
        explicit ?? WorktreePathPreview(
          template: config?.worktreesDirectory ?? "~/wt/{{repo}}/{{worktree}}",
          repo: (repo as NSString).lastPathComponent, worktree: branch, home: home ?? ""
        ).path
        ?? branch.replacingOccurrences(of: "/", with: "-")
    }
    let owner = parent.map { containerID(for: $0.id) }
    if action.opensExisting,
      let existing = workspaces.first(where: {
        WorkspaceSelection.endpoint(of: $0.id) == endpoint && $0.path == path
          && (owner == nil || $0.parent == owner || $0.id == owner)
      })
    {
      select(existing.id)
      return
    }
    let name = WorkspaceName.display(
      (path as NSString).lastPathComponent, automatic: true, directory: path, home: home)
    var creation = WorkspaceCreation(
      endpoint: endpoint, previous: selected, action: action, name: name, path: path)
    creation.home = home
    creation.parent = owner
    creation.branch = pendingBranch
    creation.repository = parent?.activeRepository
    workspaceCreationEpochs[endpoint, default: 0] &+= 1
    workspaceCreations[creation.id] = creation
    select(creation.id)
    runCreation(creation)
  }

  func canRetryCreation(_ creation: WorkspaceCreation) -> Bool {
    creation.failure != nil && !serverBusy.contains(creation.endpoint)
      && (creation.endpoint.isEmpty || remoteMessages[creation.endpoint] == "Connected")
  }

  func retryCreation(_ original: WorkspaceCreation) {
    guard canRetryCreation(original) else { return }
    var creation = original
    if creation.canResubmit && workspaces.contains(where: { $0.id == creation.proposedID }) {
      creation.result = creation.proposedID
    } else if creation.canResubmit {
      creation.token = UUID().uuidString.lowercased()
      creation.operation = nil
      creation.incarnation = nil
    }
    creation.failure = nil
    creation.canResubmit = false
    workspaceCreations[creation.id] = creation
    runCreation(creation)
  }

  private func runCreation(_ original: WorkspaceCreation) {
    DispatchQueue.global(qos: .userInitiated).async {
      var creation = original
      let state = creation.perform()
      DispatchQueue.main.async { [creation] in self.finishWorkspaceCreation(creation, state: state)
      }
    }
  }

  func finishWorkspaceCreation(_ creation: WorkspaceCreation, state: [String: Any]?) {
    guard workspaceCreations[creation.id]?.token == creation.token else { return }
    let wasSelected = selected == creation.id
    if creation.failure != nil {
      workspaceCreations[creation.id] = creation
      return
    }
    guard let created = creation.result, let state else { return }
    if reorderingWorkspaces {
      deferredWorkspaceCreations[creation.id] = (creation, state)
      return
    }
    workspaceCreationEpochs[creation.endpoint, default: 0] &+= 1
    workspaceCreationOrigins[created] = creation.id
    apply(state, endpoint: creation.endpoint)
    guard workspaces.contains(where: { $0.id == created }) else {
      var closed = creation
      closed.failure = "The workspace is no longer open"
      workspaceCreations[creation.id] = closed
      return
    }
    workspaceCreations.removeValue(forKey: creation.id)
    if wasSelected {
      select(created)
      if !focus.isEmpty && terminalViews.existing(focus)?.meta["ready"] as? Bool != true {
        awaitingCreatedFrames.insert(focus)
      }
    }
    selections.removeValue(forKey: creation.id)
  }

  func dismissCreation(_ creation: WorkspaceCreation) {
    guard creation.failure != nil else { return }
    workspaceCreations.removeValue(forKey: creation.id)
    if selected == creation.id {
      if displayedWorkspaces.contains(where: { $0.id == creation.previous }) {
        select(creation.previous)
      } else {
        selectMachine(creation.endpoint)
      }
    }
    selections.removeValue(forKey: creation.id)
  }
}

extension WorkspaceCreation {
  // Every retry polls a known operation or reuses its receipt on the same server.
  mutating func perform() -> [String: Any]? {
    let endpoint = self.endpoint
    func call(_ request: Any) -> [String: Any] { rpc(request, endpoint: endpoint) }
    func layout() -> [String: Any] { call("State") }
    let hostReply = call("HostInfo")
    guard let host = hostReply["HostInfo"] as? [String: Any],
      let generation = host["incarnation"] as? String
    else {
      failure = hostReply["error"] as? String ?? "Could not reach the server"
      return nil
    }
    home = home ?? host["home"] as? String
    if let incarnation, incarnation != generation {
      let reply = layout()
      if let state = reply["State"] as? [String: Any],
        let rows = state["workspaces"] as? [[String: Any]],
        rows.contains(where: { $0["id"] as? String == (result ?? proposedID) })
      {
        result = result ?? proposedID
        return state
      }
      failure =
        reply["error"] as? String
        ?? "The server restarted. Check the workspace list before starting again."
      return nil
    }
    var reply: [String: Any] = [:]
    if result == nil {
      if let operation {
        reply = call(["Operation": ["id": operation]])
      } else {
        guard let request = action.request(home: home) else {
          failure = "Could not determine the home directory"
          return nil
        }
        let capabilities = call("Capabilities")
        guard let features = capabilities["Capabilities"] as? [String] else {
          failure = capabilities["error"] as? String ?? "Could not read server capabilities"
          return nil
        }
        if features.contains("workspace_creation_v1") {
          incarnation = generation
          reply = call([
            "WorkspaceCreation": ["id": proposedID, "incarnation": generation, "request": request]
          ])
        } else {
          // Older servers return IDs for worktree operations and for the initial-tab API.
          // A lost initial reply on those servers cannot safely be submitted twice.
          guard incarnation == nil else {
            failure = "Could not confirm creation. Check the workspace list."
            return nil
          }
          incarnation = generation
          if case .new = action, features.contains("workspace_initial_tab"), let home {
            reply = call([
              "CreateWorkspaceWithTab": [
                "location": ["Directory": ["path": home, "name": ""]],
                "tab": ["Terminal": ["name": "Tab 1"]],
              ]
            ])
          } else {
            reply = call(request)
          }
          if let state = reply["State"] as? [String: Any],
            let rows = state["workspaces"] as? [[String: Any]]
          {
            // NewWorkspace in old daemons appends under the same lock as its reply.
            result =
              action == .new
              ? rows.last?["id"] as? String
              : rows.first { $0["path"] as? String == path }?["id"] as? String
            if result != nil { return state }
          }
        }
      }
      while let op = reply["Operation"] as? [String: Any], let id = op["id"] as? String {
        operation = id
        if op["done"] as? Bool == true {
          if let error = op["error"] as? String {
            failure = error
            canResubmit = true
            return nil
          }
          result = op["workspace"] as? String
          break
        }
        Thread.sleep(forTimeInterval: 0.1)
        reply = call(["Operation": ["id": id]])
      }
      if let created = reply["CreatedWorkspace"] as? [String: Any] {
        result = created["workspace"] as? String
      }
      guard result != nil else {
        failure = reply["error"] as? String ?? "Could not confirm workspace creation"
        return nil
      }
    }
    let latest = layout()
    guard let state = latest["State"] as? [String: Any],
      (state["workspaces"] as? [[String: Any]])?.contains(where: { $0["id"] as? String == result })
        == true
    else {
      failure = latest["error"] as? String ?? "The workspace is no longer open"
      return nil
    }
    return state
  }
}

struct WorkspaceCreationView: View {
  let creation: WorkspaceCreation
  @EnvironmentObject var store: Store
  var body: some View {
    if let failure = creation.failure {
      VStack(spacing: 12) {
        ErrorNotice(message: failure)
        HStack {
          Button("Dismiss") { store.dismissCreation(creation) }
          Button("Retry") { store.retryCreation(creation) }
            .disabled(!store.canRetryCreation(creation))
        }
      }.frame(maxWidth: 320)
    } else {
      ProgressView().accessibilityLabel("Opening workspace")
    }
  }
}
