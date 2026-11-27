import SwiftUI

struct LayoutEdit {
  let id = UUID()
  let endpoint: String
  let request: [String: Any]
  let preview: (inout [Workspace]) -> Bool
  let complete: ([String: Any]) -> Void
}

extension Store {
  var projectedWorkspaces: [Workspace] {
    if let cachedProjectedWorkspaces { return cachedProjectedWorkspaces }
    var layout = pendingLayoutEdits.reduce(workspaces) { layouts, edit in
      var preview = layouts
      return edit.preview(&preview) ? preview : layouts
    }
    if let id = draggingTab, let index = tabDragIndex,
      let owner = layout.first(where: { $0.tabs.contains { $0.id == id } })
    {
      _ = OptimisticLayout.moveTab(id, into: owner.id, at: index, in: &layout)
    }
    cachedProjectedWorkspaces = layout
    return layout
  }

  @discardableResult
  func submitLayout(
    _ request: [String: Any], endpoint: String,
    preview: @escaping (inout [Workspace]) -> Bool,
    complete: @escaping ([String: Any]) -> Void = { _ in }
  ) -> Bool {
    guard !serverBusy.contains(endpoint),
      endpoint.isEmpty || remoteMessages[endpoint] == "Connected"
    else {
      error = "The machine is unavailable"
      return false
    }
    var layout = projectedWorkspaces
    guard preview(&layout) else { return false }
    let edit = LayoutEdit(
      endpoint: endpoint, request: request, preview: preview, complete: complete)
    InteractionTiming.event("Layout preview")
    withAnimation(WorkspaceFinder.animation) {
      pendingLayoutEdits.append(edit)
      // A drag preview is applied after all queued edits when projecting the tree.
      if draggingTab == nil { cachedProjectedWorkspaces = layout }
    }
    DispatchQueue.main.async { [weak self] in self?.sendNextLayoutEdit(endpoint) }
    return true
  }

  func sendNextLayoutEdit(_ endpoint: String) {
    guard !sendingLayoutEdits.contains(endpoint), !reportingFocus.contains(endpoint),
      let edit = pendingLayoutEdits.first(where: { $0.endpoint == endpoint })
    else { return }
    var layout = workspaces
    guard edit.preview(&layout), !serverBusy.contains(endpoint),
      endpoint.isEmpty || remoteMessages[endpoint] == "Connected"
    else {
      finishLayoutEdit(edit, reply: ["error": "The layout is no longer available"])
      return
    }
    sendingLayoutEdits.insert(endpoint)
    let revision =
      capabilitiesByEndpoint[endpoint]?.contains("layout_revision") == true
      ? stateRevisions[endpoint] : nil
    let incarnation = serverIncarnations[endpoint]
    let connection = remoteConnectionGenerations[endpoint, default: 0]
    endpointQueue(endpoint).async {
      let reply = InteractionTiming.measure("Layout RPC") {
        rpc(edit.request, endpoint: endpoint, revision: revision)
      }
      DispatchQueue.main.async {
        if self.serverIncarnations[endpoint] == incarnation
          && self.remoteConnectionGenerations[endpoint, default: 0] == connection
        {
          self.finishLayoutEdit(edit, reply: reply, advance: reply["State"] != nil)
        } else {
          self.finishLayoutEdit(
            edit, reply: ["error": "The machine changed while the edit was pending"], advance: false
          )
        }
      }
      if reply["State"] == nil,
        let state = rpc("State", endpoint: endpoint)["State"] as? [String: Any]
      {
        DispatchQueue.main.async {
          if self.serverIncarnations[endpoint] == incarnation
            && self.remoteConnectionGenerations[endpoint, default: 0] == connection
          {
            self.apply(state, endpoint: endpoint)
          }
        }
      }
      DispatchQueue.main.async { self.sendNextLayoutEdit(endpoint) }
    }
  }

  private func finishLayoutEdit(
    _ edit: LayoutEdit, reply: [String: Any], advance: Bool = true
  ) {
    let before = (selected, tab, focus)
    withAnimation(WorkspaceFinder.animation) {
      pendingLayoutEdits.removeAll { $0.id == edit.id }
      if let state = reply["State"] as? [String: Any] {
        apply(state, endpoint: edit.endpoint)
      }
      if let failure = reply["error"] as? String { error = failure }
    }
    if (selected, tab) == (before.0, before.1) && !visiblePaneIDs.contains(focus) {
      focus = visiblePaneIDs.first ?? ""
      focusTerminal()
    }
    edit.complete(reply)
    sendingLayoutEdits.remove(edit.endpoint)
    flushDeferredFocus()
    trimTerminalViews()
    if advance { sendNextLayoutEdit(edit.endpoint) }
  }

  func submitNewTab(workspace: Workspace, editor: [String: String]? = nil) {
    let endpoint = WorkspaceSelection.endpoint(of: workspace.id)
    let prefix = endpoint.isEmpty ? "" : endpoint + "|"
    let placeholder = prefix + "pending-tab-" + UUID().uuidString
    let leaf = prefix + "pending-pane-" + UUID().uuidString
    guard
      !pendingLayoutEdits.contains(where: {
        ($0.request["NewTab"] as? [String: String])?["workspace"] == workspace.id
          || ($0.endpoint == endpoint && $0.request["EditPreviewFile"] != nil)
      })
    else { return }
    let original = Set(workspace.tabs.map(\.id))
    let names = Set(workspace.tabs.map(\.name))
    let number = (1...(names.count + 1)).first { !names.contains("Tab \($0)") }!
    let pending = Tab(id: placeholder, name: "Tab \(number)", root: .leaf(leaf))
    let prior = tab
    let accepted = submitLayout(
      editor.map { ["EditPreviewFile": $0] } ?? ["NewTab": ["workspace": workspace.id]],
      endpoint: endpoint,
      preview: { workspaces in
        guard let index = workspaces.firstIndex(where: { $0.id == workspace.id }) else {
          return false
        }
        // A poll may see the created tab before NewTab replies. Publish newly
        // discovered siblings together once the reply identifies our own tab.
        workspaces[index].tabs.removeAll { !original.contains($0.id) }
        workspaces[index].tabs.append(pending)
        return true
      },
      complete: { [weak self] reply in
        guard let self else { return }
        // NewTab appends its tab under the daemon's layout lock. Its own reply
        // identifies it even when another client also opened a tab.
        let snapshot = reply["State"] as? [String: Any]
        let owner = (snapshot?["workspaces"] as? [[String: Any]])?.first {
          $0["id"] as? String == workspace.id
        }
        let createdID = (owner?["tabs"] as? [[String: Any]])?.last?["id"] as? String
        let created = self.workspaces.first { $0.id == workspace.id }?.tabs.first {
          $0.id == createdID && !original.contains($0.id)
        }
        if let created { self.tabCreationOrigins[created.id] = placeholder }
        guard self.selected == workspace.id, self.tab == placeholder else { return }
        self.select(workspace.id, tab: created?.id ?? prior)
        if let created {
          self.awaitingCreatedFrames.formUnion(created.root.ids)
        }
      })
    if accepted { select(workspace.id, tab: placeholder) }
  }

  static func layoutPlaceholder(_ id: String) -> Bool {
    id.split(separator: "|").last?.hasPrefix("pending-") == true
  }
}

enum OptimisticLayout {
  static func removePane(_ pane: String, from layout: inout [Workspace], removeEmptyTab: Bool)
    -> Bool
  {
    guard
      let workspace = layout.firstIndex(where: { $0.tabs.contains { $0.root.ids.contains(pane) } }),
      let tab = layout[workspace].tabs.firstIndex(where: { $0.root.ids.contains(pane) })
    else { return false }
    if let root = layout[workspace].tabs[tab].root.removing(pane) {
      layout[workspace].tabs[tab].root = root
    } else if removeEmptyTab {
      layout[workspace].tabs.remove(at: tab)
    } else {
      layout[workspace].tabs[tab].root = .empty
    }
    return true
  }
}

extension OptimisticLayout {
  static func movePane(
    _ pane: String, to destination: [String: Any], in layout: inout [Workspace],
    placeholder: String, removeEmptyTab: Bool
  ) -> Bool {
    guard let source = layout.lazy.flatMap(\.tabs).first(where: { $0.root.ids.contains(pane) })
    else { return false }
    let endpoint = WorkspaceSelection.endpoint(of: pane)
    let targetPane = destination["Pane"] as? [String: String]
    let targetTab = (destination["Tab"] as? [String: String])?["tab"]
    let targetWorkspace = (destination["Workspace"] as? [String: String])?["workspace"]
    let peer = targetPane?["pane"]
    guard let targetID = peer ?? targetTab ?? targetWorkspace,
      WorkspaceSelection.endpoint(of: targetID) == endpoint,
      peer != pane
    else { return false }

    if targetWorkspace == nil && targetTab == source.id && source.root.ids.count == 1 {
      return false
    }
    guard removePane(pane, from: &layout, removeEmptyTab: removeEmptyTab) else { return false }
    if let targetWorkspace,
      let workspace = layout.firstIndex(where: { $0.id == targetWorkspace })
    {
      layout[workspace].tabs.append(Tab(id: placeholder, name: source.name, root: .leaf(pane)))
      return true
    }
    for workspace in layout.indices {
      for tab in layout[workspace].tabs.indices {
        let target = layout[workspace].tabs[tab]
        guard target.id == targetTab || peer.map({ target.root.ids.contains($0) }) == true
        else { continue }
        if let leaf = peer ?? target.root.ids.first {
          let placement = targetPane?["placement"] ?? "Right"
          layout[workspace].tabs[tab].root = target.root.inserting(
            pane, nextTo: leaf, vertical: ["Above", "Below"].contains(placement),
            before: ["Left", "Above"].contains(placement))
        } else {
          layout[workspace].tabs[tab].root = .leaf(pane)
        }
        return true
      }
    }
    return false
  }
}

extension OptimisticLayout {
  static func moveTab(
    _ id: String, into workspace: String, at index: Int, in layout: inout [Workspace]
  ) -> Bool {
    guard let source = layout.firstIndex(where: { $0.tabs.contains { $0.id == id } }),
      let tab = layout[source].tabs.firstIndex(where: { $0.id == id }),
      let target = layout.firstIndex(where: { $0.id == workspace }),
      (0...max(0, layout[target].tabs.count - (source == target ? 1 : 0))).contains(index)
    else { return false }
    let moved = layout[source].tabs.remove(at: tab)
    layout[target].tabs.insert(moved, at: index)
    return true
  }
}

extension Store {
  func commitDivider(tab: String, path: [Bool], ratio: Double) {
    guard let current = currentTab, current.id == tab else { return }
    let expected = current.root.ids
    submitLayout(
      ["Ratio": ["tab": tab, "path": path, "ratio": ratio]],
      endpoint: WorkspaceSelection.endpoint(of: tab),
      preview: { layout in
        for workspace in layout.indices {
          guard let index = layout[workspace].tabs.firstIndex(where: { $0.id == tab }) else {
            continue
          }
          let root = layout[workspace].tabs[index].root
          guard root.ids == expected, let resized = root.resizing(path[...], ratio: ratio) else {
            return false
          }
          layout[workspace].tabs[index].root = resized
          return true
        }
        return false
      })
  }
}
