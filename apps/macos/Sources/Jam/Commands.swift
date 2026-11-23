import AppKit
import CJam
import SwiftUI

struct JamCommand: Identifiable {
  let id: String
  let title: String
  static let all: [JamCommand] = [
    .init(id: "find", title: "Find in Tab History"),
    .init(id: "find_next", title: "Find Next"),
    .init(id: "find_previous", title: "Find Previous"),
    .init(id: "palette", title: "Command Palette"),
    .init(id: "new_workspace", title: "New Workspace"),
    .init(id: "open_workspace", title: "Open Workspace"),
    .init(id: "new_tab", title: "New Tab"), .init(id: "split_right", title: "Split Right"),
    .init(id: "split_down", title: "Split Down"),
    .init(id: "new_worktree", title: "New Worktree"),
    .init(id: "open_worktree", title: "Open Worktree"),
    .init(id: "close_pane", title: "Close Pane"),
    .init(id: "close_workspace", title: "Close Workspace"),
    .init(id: "settings", title: "Settings"),
    .init(id: "agent_integrations", title: "Agent Integrations"),
  ]
}
struct JamConfig: Codable, Equatable {
  enum CodingKeys: String, CodingKey {
    case worktreesDirectory = "worktrees_directory"
    case bindings
    case enabledAgents = "enabled_agents"
    case agentModels = "agent_models"
    case resumeInterruptedWork = "resume_interrupted_work"
    case maxAgentsPerCoordinator = "max_agents_per_coordinator"
    case appearance
    case closeEmptyWorkspaces = "close_empty_workspaces"
    case closeEmptyTabs = "close_empty_tabs"
    case allowWorkspaceDeletion = "allow_workspace_deletion"
    case copyOnSelect = "copy_on_select"
    case automaticCheckUpdates = "automatic_check_updates"
  }
  var worktreesDirectory: String
  var bindings: [String: [String]]
  var copyOnSelect: Bool = false
  var automaticCheckUpdates = true
  var enabledAgents = ["claude", "codex"]
  var agentModels: [String: AgentModelSettings] = [:]
  var maxAgentsPerCoordinator = 8
  var resumeInterruptedWork = true
  var appearance = "system"
  var closeEmptyWorkspaces = false
  var closeEmptyTabs = true
  var allowWorkspaceDeletion = true
  static var defaults: JamConfig {
    (try? load("defaults"))
      ?? JamConfig(worktreesDirectory: "~/wt/{{repo}}/{{worktree}}", bindings: [:])
  }
  static func load(_ request: String = "get") throws -> JamConfig {
    let raw = nativeString(jam_config(request))
    let data = Data(raw.utf8)
    if let error = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["error"]
      as? String
    {
      throw NSError(domain: error, code: 1)
    }
    return try JSONDecoder().decode(Self.self, from: data)
  }
  func save() throws {
    let text = String(decoding: try JSONEncoder().encode(self), as: UTF8.self)
    let raw = nativeString(jam_config(text))
    if let error = (try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any])?[
      "error"] as? String
    {
      throw NSError(domain: error, code: 1)
    }
  }
}
extension Store {
  func selectMachine(_ endpoint: String) {
    if let workspace = displayedWorkspaces.first(where: {
      WorkspaceSelection.endpoint(of: $0.id) == endpoint
    }) {
      select(workspace.id)
      return
    }
    if !selected.isEmpty && surface == .terminals { selections[selected] = (tab, focus) }
    surface = .terminals
    selectedMachine = endpoint
    selected = ""
    tab = ""
    focus = ""
    refreshCapabilities()
    trimTerminalViews()
  }
  func select(
    _ workspace: String, tab: String? = nil, pane: String? = nil,
    surface: WorkspaceSurface = .terminals
  ) {
    if self.surface == .terminals
      && (selections[selected]?.0 != self.tab || selections[selected]?.1 != focus)
    {
      selections[selected] = (self.tab, focus)
    }
    if selected != workspace { selected = workspace }
    self.surface = surface
    let saved = selections[workspace]
    let candidateTab = tab ?? saved?.0 ?? ""
    let targetTab =
      current?.tabs.contains(where: { $0.id == candidateTab }) == true
      ? candidateTab : current?.tabs.first?.id ?? ""
    if self.tab != targetTab { self.tab = targetTab }
    let candidate = pane ?? (tab == nil ? saved?.1 : nil) ?? ""
    let targetPane = visiblePaneIDs.contains(candidate) ? candidate : visiblePaneIDs.first ?? ""
    if focus != targetPane { focus = targetPane }
    refreshCapabilities()
    for id in visiblePaneIDs { restartPane(id, automatic: true) }
    focusTerminal()
    trimTerminalViews()
  }
  func trimTerminalViews() {
    terminalTrimWork?.cancel()
    let work = DispatchWorkItem { [weak self] in
      guard let self else { return }
      var retained = Set(self.visiblePaneIDs)
      if let lifted = self.liftedPane { retained.insert(lifted) }
      self.terminalViews.retain(live: Set(self.panes.keys), visible: retained)
    }
    terminalTrimWork = work
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: work)
  }
  func focusTerminal() {
    DispatchQueue.main.async { [weak self] in
      guard let self, self.editingWorkspace == nil, self.editingTab == nil,
        !TerminalInputGate.blocked,
        self.visiblePaneIDs.contains(self.focus),
        let terminal = self.terminalViews.existing(self.focus),
        let window = terminal.window, !terminal.isHiddenOrHasHiddenAncestor,
        NSApp.keyWindow == nil || NSApp.keyWindow === window
      else { return }
      // The inline editor's owning window can temporarily stop being key when
      // its field editor closes. Use the terminal's window for the handoff.
      if window.firstResponder === terminal {
        terminal.acknowledgeFocus()
      } else {
        window.makeFirstResponder(terminal)
      }
    }
  }
  func shortcut(_ action: String) -> KeyboardShortcut? {
    guard let chord = config?.bindings[action]?.first,
      let key = chord.split(separator: "+").last?.first
    else { return nil }
    let parts = chord.split(separator: "+")
    var mods: EventModifiers = []
    if parts.contains("ctrl") { mods.insert(.control) }
    if parts.contains("shift") { mods.insert(.shift) }
    if parts.contains("alt") { mods.insert(.option) }
    if parts.contains("cmd") { mods.insert(.command) }
    return KeyboardShortcut(KeyEquivalent(key), modifiers: mods)
  }
  func execute(_ action: String) {
    if showSettings && action != "settings" { return }
    switch action {
    case "find": finder.toggle()
    case "find_next": finder.next(1)
    case "find_previous": finder.next(-1)
    case "palette": showPalette = true
    case "new_workspace": newWorkspace()
    case "open_workspace": openFolder()
    case "new_tab": newTab()
    case "split_right": split(false)
    case "split_down": split(true)
    case "new_worktree":
      if enabled(action) { showWorktree = true }
    case "open_worktree": openWorktree()
    case "close_pane":
      if !focus.isEmpty { close(focus) }
    case "close_workspace": if let w = current { closeWorkspace(w, group: w.parent == nil) }
    case "settings": openSettings()
    case "agent_integrations": showAgentIntegrations = true
    default: break
    }
  }
  func enabled(_ action: String) -> Bool {
    guard !serverBusy.contains(currentEndpoint) else { return false }
    if action == "split_right" { return !busy && canSplit(false) }
    if action == "split_down" { return !busy && canSplit(true) }
    if ["find", "find_next", "find_previous"].contains(action) { return !visiblePaneIDs.isEmpty }
    if ["palette", "new_workspace", "open_workspace", "settings", "agent_integrations"].contains(
      action)
    {
      return true
    }
    if isCreatingWorkspace(selected) { return false }
    if action == "new_worktree" {
      return current.map { worktreeReason($0) == nil } == true && !busy
    }
    if action == "open_worktree" {
      return current.map {
        $0.activeRepository != nil
          && capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: $0.id)]?.contains(
            "workspace_containers") == true
      } == true && !busy
    }
    return current != nil && !busy
  }
  func worktreeReason(_ workspace: Workspace) -> String? {
    guard
      capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: workspace.id)]?.contains(
        "workspace_containers") == true
    else {
      return "Update this machine's daemon to use workspace worktrees"
    }
    guard let context = workspace.context, context.ready else { return "Checking repository…" }
    guard context.repository != nil else { return "This folder is not a Git repository" }
    return context.createDisabledReason
  }
  func operation(_ request: [String: Any], endpoint requestedEndpoint: String? = nil) {
    let endpoint = requestedEndpoint ?? requestEndpoint(request)
    guard !busy, !serverBusy.contains(endpoint) else { return }
    if surface == .terminals { selections[selected] = (tab, focus) }
    busy = true
    DispatchQueue.global().async {
      var reply = rpc(request, endpoint: endpoint)
      while let op = reply["Operation"] as? [String: Any], op["done"] as? Bool == false,
        let id = op["id"] as? String
      {
        Thread.sleep(forTimeInterval: 0.1)
        reply = rpc(["Operation": ["id": id]], endpoint: endpoint)
      }
      let latestState = rpc("State", endpoint: endpoint)["State"] as? [String: Any]
      DispatchQueue.main.async {
        if let latestState { self.apply(latestState, endpoint: endpoint) }
        self.busy = false
        if let error = reply["error"] as? String {
          self.error = error
        } else if let op = reply["Operation"] as? [String: Any] {
          if let error = op["error"] as? String {
            self.error = error
          } else if let id = op["workspace"] as? String {
            self.select(id)
          }
        }
        self.refresh()
      }
    }
  }
  func openCheckout(
    _ path: String, endpoint: String? = nil, workspace: String? = nil, branch: String? = nil
  ) {
    guard let source = workspace ?? current?.id else { return }
    beginCreation(.openWorktree(source: source, path: path), endpoint: endpoint, branch: branch)
  }
  func containerID(for workspace: String) -> String {
    workspaces.first { $0.id == workspace }?.parent ?? workspace
  }
  func openedCheckoutPaths(in workspace: String) -> Set<String> {
    let owner = containerID(for: workspace)
    var paths = Set(
      workspaces.filter { $0.parent == owner }.map { WorktreeChoices.normalized($0.path) })
    if let path = workspaces.first(where: { $0.id == owner })?.context?.checkoutPath {
      paths.insert(WorktreeChoices.normalized(path))
    }
    return paths
  }
  func openWorktree() {
    guard let w = current, w.activeRepository != nil,
      capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: w.id)]?.contains(
        "workspace_containers") == true
    else { return }
    worktreePicker = WorktreePickerContext(
      workspace: w.id, name: w.name, path: w.context?.path ?? w.path,
      endpoint: WorkspaceSelection.endpoint(of: w.id))
  }
  func workspaceCloseNeedsConfirmation(_ workspace: Workspace, group: Bool) -> Bool {
    var affected: Set<String> = [workspace.id]
    if group {
      while true {
        let children = workspaces.filter { $0.parent.map(affected.contains) == true }.map(\.id)
        let previousCount = affected.count
        affected.formUnion(children)
        if affected.count == previousCount { break }
      }
    }
    let entries = workspaces.filter { affected.contains($0.id) }
    let root = entries.first { $0.id == workspace.id } ?? workspace
    let targets = [root] + entries.filter { $0.id != root.id }
    return targets.contains { target in
      var ids = target.tabs.flatMap { $0.root.ids }
      if let coordinator = target.coordinator { ids.append(coordinator.pane) }
      // Missing metadata is not evidence that a terminal has stopped.
      return ids.contains { panes[$0]?.exited != true }
    }
  }

  func closeWorkspace(_ w: Workspace, group: Bool) {
    guard !isCreatingWorkspace(w.id) else { return }
    if !workspaceCloseNeedsConfirmation(w, group: group) {
      request(["WorkspaceClose": ["workspace": w.id, "group": group]])
      return
    }
    guard let window = NSApp.keyWindow ?? NSApp.mainWindow,
      window.attachedSheet == nil
    else { return }
    let alert = NSAlert()
    alert.messageText = group ? "Close project and its open worktrees?" : "Close workspace?"
    alert.informativeText =
      "Running terminals will stop. Checkout files and branches will be preserved."
    alert.addButton(withTitle: "Close")
    alert.addButton(withTitle: "Cancel")
    alert.beginSheetModal(for: window) { response in
      guard response == .alertFirstButtonReturn else { return }
      self.request(["WorkspaceClose": ["workspace": w.id, "group": group]])
    }
  }
  func removeWorktree(_ w: Workspace, force: Bool = false) {
    guard config?.allowWorkspaceDeletion != false else { return }
    guard let window = NSApp.keyWindow ?? NSApp.mainWindow else { return }
    let alert = NSAlert()
    alert.alertStyle = .warning
    alert.messageText = force ? "Force delete checkout?" : "Delete checkout?"
    alert.informativeText =
      "\(w.path)\n\nThis stops its terminals and deletes checkout files. Branches are preserved."
      + (force ? "\nUncommitted and untracked files will be permanently lost." : "")
    alert.addButton(withTitle: force ? "Force Delete" : "Delete")
    alert.addButton(withTitle: "Cancel")
    // A sheet belongs to the Jam window, unlike a standalone runModal alert.
    alert.beginSheetModal(for: window) { response in
      guard response == .alertFirstButtonReturn else { return }
      if force {
        self.operation(["WorktreeRemove": ["workspace": w.id, "force": true]])
        return
      }
      DispatchQueue.global().async {
        var result = rpc(["WorktreeRemove": ["workspace": w.id, "force": false]])
        while let op = result["Operation"] as? [String: Any], op["done"] as? Bool == false,
          let id = op["id"] as? String
        {
          Thread.sleep(forTimeInterval: 0.1)
          result = rpc(["Operation": ["id": id]])
        }
        let final = result
        DispatchQueue.main.async {
          let error =
            (final["Operation"] as? [String: Any])?["error"] as? String
            ?? final["error"] as? String
          if let error {
            if error.contains("local changes") {
              self.removeWorktree(w, force: true)
            } else {
              self.error = error
            }
          }
          self.refresh()
        }
      }
    }
  }
  func reloadConfig() {
    do {
      let c = try JamConfig.load()
      if c != config {
        config = c
        appUpgrade.configure(automatic: c.automaticCheckUpdates)
        NSApp.appearance =
          c.appearance == "dark"
          ? NSAppearance(named: .darkAqua)
          : c.appearance == "light" ? NSAppearance(named: .aqua) : nil
      }
    } catch {
      self.error = "Could not load settings: \(error)"
      if config == nil { config = JamConfig.defaults }
    }
  }
  func installShortcuts() {
    monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self, self.editingWorkspace == nil, self.editingTab == nil, self.draggingTab == nil,
        !self.busy,
        !self.showSettings, !self.showAgentIntegrations, !self.showPalette,
        !self.showWorktree, self.worktreePicker == nil,
        !(event.window is NSColorPanel), event.window?.sheetParent == nil,
        event.window?.attachedSheet == nil, event.window == NSApp.keyWindow
      else { return event }
      let flags = event.modifierFlags.intersection([.control, .shift, .option, .command])
      var parts: [String] = []
      if flags.contains(.control) { parts.append("ctrl") }
      if flags.contains(.shift) { parts.append("shift") }
      if flags.contains(.option) { parts.append("alt") }
      if flags.contains(.command) { parts.append("cmd") }
      guard let key = event.charactersIgnoringModifiers?.lowercased() else { return event }
      parts.append(key)
      let keyString = parts.joined(separator: "+")
      if self.showFind {
        if event.keyCode == 53 {
          self.finder.close()
          return nil
        }
        if event.keyCode == 36 || event.keyCode == 76 {
          self.finder.next(flags.contains(.shift) ? -1 : 1)
          return nil
        }
        if let action = self.config?.bindings.first(where: { $0.value.contains(keyString) })?.key,
          ["find", "find_next", "find_previous"].contains(action)
        {
          self.execute(action)
          return nil
        }
        return event
      }
      if let action = self.config?.bindings.first(where: { $0.value.contains(keyString) })?.key {
        if self.enabled(action) { self.execute(action) }
        return nil
      }
      return event
    }
  }
}
struct CommandPalette: View {
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) var dismiss
  @State private var query = ""
  @State private var selected = 0
  @FocusState private var searching: Bool
  struct Entry: Identifiable {
    let id: String
    let title: String
    let detail: String
    let action: () -> Void
    var enabled: Bool = true
  }
  var entries: [Entry] {
    var all = JamCommand.all.filter { $0.id != "palette" }.map { c in
      Entry(
        id: c.id, title: c.title,
        detail: store.config?.bindings[c.id]?.joined(separator: ", ") ?? "",
        action: { store.execute(c.id) }, enabled: store.enabled(c.id))
    }
    for w in store.workspaces {
      all.append(Entry(id: w.id, title: w.name, detail: w.path, action: { store.select(w.id) }))
      if let coordinator = w.coordinator {
        all.append(
          Entry(
            id: coordinator.id, title: "Coordinator", detail: w.name,
            action: { store.openCoordinator(w.id) }))
      }
      for t in w.tabs {
        all.append(
          Entry(id: t.id, title: t.name, detail: w.name, action: { store.select(w.id, tab: t.id) }))
        for id in t.root.ids {
          all.append(
            Entry(
              id: id, title: store.panes[id]?.title ?? "Terminal", detail: "\(w.name) / \(t.name)",
              action: { store.select(w.id, tab: t.id, pane: id) }))
        }
      }
    }
    func matches(_ needle: String, _ haystack: String) -> Bool {
      var chars = needle.makeIterator()
      var next = chars.next()
      for c in haystack where c == next { next = chars.next() }
      return next == nil
    }
    let words = query.lowercased().split(separator: " ")
    return all.filter { e in
      words.allSatisfy { matches(String($0), (e.title + " " + e.detail).lowercased()) }
    }
  }
  func run(_ e: Entry) {
    guard e.enabled else { return }
    dismiss()
    DispatchQueue.main.async { e.action() }
  }
  var body: some View {
    VStack(spacing: 12) {
      TextField("Search commands, workspaces, tabs, and panes", text: $query).textFieldStyle(
        .roundedBorder
      ).focused($searching).onSubmit {
        if !entries.isEmpty { run(entries[min(selected, entries.count - 1)]) }
      }
      ScrollView {
        LazyVStack(spacing: 2) {
          ForEach(Array(entries.enumerated()), id: \.element.id) { i, e in
            Button {
              run(e)
            } label: {
              HStack {
                Text(e.title)
                Spacer()
                Text(e.detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
              }.padding(9).contentShape(Rectangle()).background(
                i == selected ? Color.accentColor.opacity(0.15) : .clear,
                in: RoundedRectangle(cornerRadius: 6))
            }.buttonStyle(.plain).disabled(!e.enabled)
          }
        }
      }
      if entries.isEmpty { Text("No matching commands").foregroundStyle(.secondary) }
    }.padding(18).frame(width: 640, height: 420).onAppear { searching = true }.onChange(of: query) {
      _, _ in selected = 0
    }.onKeyPress(.downArrow) {
      selected = min(max(0, entries.count - 1), selected + 1)
      return .handled
    }.onKeyPress(.upArrow) {
      selected = max(0, selected - 1)
      return .handled
    }.onMoveCommand { direction in
      if direction == .down {
        selected = min(max(0, entries.count - 1), selected + 1)
      } else if direction == .up {
        selected = max(0, selected - 1)
      }
    }.onExitCommand { dismiss() }
  }
}
struct WorktreeForm: View {
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) var dismiss
  @State private var branch = ""
  @State private var base = "HEAD"
  @State private var path = ""
  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Text("New Worktree").font(.headline)
      TextField("Branch name", text: $branch)
      TextField("Base revision", text: $base)
      TextField("Checkout path (optional)", text: $path)
      HStack {
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        Spacer()
        if let workspace = store.current, let reason = store.worktreeReason(workspace) {
          Text(reason).font(.caption).foregroundStyle(.secondary)
        }
        Button("Create") {
          guard let w = store.current else { return }
          store.beginCreation(
            .createWorktree(
              source: w.id, branch: branch, base: base, path: path.isEmpty ? nil : path))
          dismiss()
        }.keyboardShortcut(.defaultAction).disabled(
          branch.isEmpty || !store.enabled("new_worktree"))
      }
    }.textFieldStyle(.roundedBorder).padding(24).frame(width: 480)
  }
}
