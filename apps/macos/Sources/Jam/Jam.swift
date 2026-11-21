import AppKit
import CJam
import Darwin
import SwiftUI

func nativeString(_ p: UnsafeMutablePointer<CChar>?) -> String {
  guard let p else { return "{\"error\":\"No native response\"}" }
  defer { jam_free(p) }
  return String(cString: p)
}
func rpc(_ request: Any, endpoint: String = "", revision: UInt64? = nil) -> [String: Any] {
  do {
    var payload = request
    if let revision, var object = request as? [String: Any] {
      object["_revision"] = revision
      payload = object
    }
    let data = try JSONSerialization.data(withJSONObject: payload, options: .fragmentsAllowed)
    let text = String(decoding: data, as: UTF8.self)
    let value = try JSONSerialization.jsonObject(
      with: Data(nativeString(jam_request_endpoint(endpoint, text)).utf8),
      options: .fragmentsAllowed)
    return value as? [String: Any] ?? ["response": value]
  } catch { return ["error": error.localizedDescription] }
}
struct Pane: Identifiable {
  var id: String
  var title: String
  var cwd: String
  var status: String
  var exited: Bool
  var reports: [[String: Any]] = []
  var launchError: String? = nil
  var agentProvider: String? = nil
  var agentRole: String? = nil
  var isAIPane: Bool { agentProvider != nil || agentRole != nil }
  var activityStatus: ActivityStatus { exited ? .stopped : ActivityStatus(status) }
}
extension Store {
  func activityStatus(for tab: Tab) -> ActivityStatus? {
    let agents = tab.root.ids.compactMap { panes[$0] }.filter(\.isAIPane)
    return agents.isEmpty ? nil : ActivityStatus.aggregate(agents.map(\.activityStatus))
  }
  func activityStatus(for workspace: Workspace) -> ActivityStatus {
    ActivityStatus.aggregate(workspace.tabs.compactMap { activityStatus(for: $0) })
  }
}
struct Tab: Identifiable {
  var agentRole: String? = nil
  var id: String
  var name: String
  var root: Node
}
struct WorkspaceCoordinator {
  let id: String
  let pane: String
  let provider: String
  init(id: String, pane: String, provider: String) {
    self.id = id
    self.pane = pane
    self.provider = provider
  }
  init?(_ value: [String: Any]?) {
    guard let value, let id = value["id"] as? String,
      let pane = value["pane"] as? String, let provider = value["provider"] as? String
    else { return nil }
    self.init(id: id, pane: pane, provider: provider)
  }
}
enum WorkspaceSurface { case terminals, coordinator }
struct Workspace: Identifiable {
  var id: String
  var name: String
  var path: String
  var tabs: [Tab]
  var coordinator: WorkspaceCoordinator? = nil
  var hasCoordinator: Bool { coordinator != nil }
  var repository: String? = nil
  var mainPath: String? = nil
  var branch: String? = nil
  var parent: String? = nil
  var context: RepositoryContext? = nil
  var activeRepository: String? { context?.repository ?? (context == nil ? repository : nil) }
  var sidebarBranch: String? {
    if parent != nil { return context?.repository == repository ? context?.branch : branch }
    return context?.branch ?? (context == nil ? branch : nil)
  }
}
struct RepositoryContext: Decodable {
  let path: String
  let checkoutPath: String?
  let ready: Bool
  let repository: String?
  let branch: String?
  let createDisabledReason: String?
  enum CodingKeys: String, CodingKey {
    case path, ready, repository, branch
    case createDisabledReason = "create_disabled_reason"
    case checkoutPath = "checkout_path"
  }
}
enum TerminalInputGate { static var blocked = false }
final class Store: ObservableObject {
  let appUpgrade = AppUpgradeController()
  @Published var remotes: [RemoteMachine] = []
  @Published var editingRemote: RemoteMachine?
  var pendingRenames: [String: PendingRename] = [:]
  var pausedRemotes = Set<String>()
  @Published var startingCoordinators = Set<String>()
  @Published var surface: WorkspaceSurface =
    JamDefaults.shared.bool(forKey: "coordinatorSurface") ? .coordinator : .terminals
  {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(surface == .coordinator, forKey: "coordinatorSurface")
      }
    }
  }
  @Published var coordinatorRecovery: CoordinatorRecovery? { didSet { updateInputGate() } }
  @Published var showRemote = false { didSet { updateInputGate() } }
  @Published var serverAction: ServerAction? { didSet { updateInputGate() } }
  @Published var serverStatuses: [String: ServerStatus] = [:]
  @Published var agentUsage: [String: [AgentUsageReport]] = [:]
  @Published var usageErrors: [String: String] = [:]
  var usageLoading = Set<String>()
  var usagePolled: [String: Date] = [:]
  @Published var serverBusy = Set<String>() { didSet { updateInputGate() } }
  @Published var checkingServers = Set<String>()
  @Published var scheduledServerUpdates = Set(
    JamDefaults.shared.stringArray(forKey: "scheduledServerUpdates") ?? [])
  {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(Array(scheduledServerUpdates), forKey: "scheduledServerUpdates")
      }
    }
  }
  var serverCheckDates: [String: Date] = [:]
  var serverGenerations: [String: UInt64] = [:]
  @Published var showRemoteDirectory = false { didSet { updateInputGate() } }
  @Published var remotePersistence: [String: Bool] = [:]
  var groupsMachines: Bool { !remotes.isEmpty }
  @Published var loadedRemotes = Set<String>()
  func remoteSidebarState(_ id: String) -> RemoteSidebarState {
    RemoteSidebarState.resolve(
      loaded: loadedRemotes.contains(id), loading: remoteLoading.contains(id),
      paused: pausedRemotes.contains(id), hasResult: remoteMessages[id] != nil)
  }
  @Published var remoteMessages: [String: String] = [:] { didSet { updateInputGate() } }
  var stateRevisions: [String: UInt64] = [:]
  var remoteHomes: [String: String] = [:]
  @Published var remoteLoading = Set<String>()
  @Published var remoteConnecting = Set<String>()
  var remoteConnectionGenerations: [String: UInt64] = [:]
  func remoteConnectionState(_ id: String) -> RemoteConnectionState {
    RemoteConnectionState.resolve(
      connected: remoteMessages[id] == "Connected", attempting: remoteConnecting.contains(id),
      paused: pausedRemotes.contains(id), hasConnected: loadedRemotes.contains(id),
      hasResult: remoteMessages[id] != nil)
  }
  var remoteNext: [String: Date] = [:]
  var remoteDelay: [String: Double] = [:]
  @Published var selectedMachine = WorkspaceSelection.restoredMachine(
    workspace: JamDefaults.shared.string(forKey: "workspace") ?? "",
    machine: JamDefaults.shared.string(forKey: "machine") ?? ""
  ) {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(selectedMachine, forKey: "machine")
      }
      updateInputGate()
    }
  }
  var currentEndpoint: String { selectedMachine }
  @Published var workspaces: [Workspace] = []
  @Published var workspaceCreations: [String: WorkspaceCreation] = [:]
  var workspaceCreationOrigins: [String: String] = [:]
  var workspaceCreationEpochs: [String: UInt64] = [:]
  var displayedWorkspaces: [Workspace] {
    if workspaceCreations.isEmpty {
      return WorkspaceSelection.ordered(
        workspaces, machines: [""] + remotes.map(\.id), id: { $0.id })
    }
    // A poll can discover the new workspace before its creation reply arrives.
    // Keep that row represented by the placeholder until its identity is known.
    let visible = workspaces.filter { workspace in
      guard let creation = workspaceCreations[WorkspaceSelection.endpoint(of: workspace.id)] else {
        return true
      }
      return creation.includesDuringCreation(workspace.id)
        || workspaceCreationOrigins[workspace.id] == creation.id
    }
    let represented = Set(visible.compactMap { workspaceCreationOrigins[$0.id] })
    let pending = workspaceCreations.values.filter { !represented.contains($0.id) }
      .sorted { $0.id < $1.id }.map {
        Workspace(id: $0.id, name: "~", path: $0.home ?? "~", tabs: [])
      }
    return WorkspaceSelection.ordered(
      visible + pending, machines: [""] + remotes.map(\.id), id: { $0.id })
  }
  func isCreatingWorkspace(_ id: String) -> Bool {
    workspaceCreations.values.contains { $0.id == id }
  }
  @Published var panes: [String: Pane] = [:]
  @Published var selected = JamDefaults.shared.string(forKey: "workspace") ?? "" {
    didSet {
      if !selected.isEmpty {
        let endpoint = WorkspaceSelection.endpoint(of: selected)
        if selectedMachine != endpoint { selectedMachine = endpoint }
      }
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(selected, forKey: "workspace")
      }
    }
  }
  @Published var tab = JamDefaults.shared.string(forKey: "tab") ?? "" {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(tab, forKey: "tab")
      }
    }
  }
  @Published var focus = JamDefaults.shared.string(forKey: "pane") ?? "" {
    didSet {
      reportFocus()
      updateInputGate()
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(focus, forKey: "pane")
      }
    }
  }
  @Published var showPalette = false {
    didSet { updateInputGate() }
  }
  lazy var finder = WorkspaceFinder(store: self)
  @Published var showFind = false { didSet { updateInputGate() } }
  @Published var showSettings = false {
    didSet { updateInputGate() }
  }
  @Published var showAgentIntegrations = false { didSet { updateInputGate() } }
  @Published var showWorktree = false {
    didSet { updateInputGate() }
  }
  @Published var worktreePicker: WorktreePickerContext? { didSet { updateInputGate() } }
  @Published var busy = false
  @Published var movingLayout = false
  @Published var liftedPane: String?
  let terminalViews = TerminalViewPool()
  @Published var availableAgents: [String: Set<String>] = [:]
  @Published var config: JamConfig?
  var monitor: Any?
  var selections: [String: (String, String)] =
    (JamDefaults.shared.dictionary(forKey: "workspaceSelections") as? [String: [String]] ?? [:])
    .compactMapValues { v in v.count == 2 ? (v[0], v[1]) : nil }
  {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(
          selections.mapValues { [$0.0, $0.1] }, forKey: "workspaceSelections")
      }
    }
  }
  @Published var workspaceTints =
    JamDefaults.shared.dictionary(forKey: "workspaceTints") as? [String: String] ?? [:]
  {
    didSet {
      if !CommandLine.arguments.contains("--preview") {
        JamDefaults.shared.set(workspaceTints, forKey: "workspaceTints")
      }
    }
  }
  @Published var gitSummaries: [String: GitSummary] = [:]
  var canReadGitSummary = false
  var gitSummaryLoading = false
  var gitSummaryTime = Date.distantPast
  var tintPicker: WorkspaceTintPicker?
  @Published var paneSizes: [String: CGSize] = [:]
  @Published var restartingPanes = Set<String>()
  var serverIncarnations: [String: String] = [:]
  var pendingServerRecovery = Set<String>()
  @Published var enablingPersistence = Set<String>()
  @Published var closingTabs = Set<String>()
  @Published var canMoveTabs = false
  @Published var canMovePanes = false
  lazy var paneDragController = PaneDragController(store: self)
  @Published var draggingPane: String? { didSet { updateInputGate() } }
  @Published var canNameWorkspaces = false
  var canOrderWorkspaces = false
  var reorderingWorkspaces = false
  var workspaceOrderGeneration: UInt64 = 0
  var terminalTrimWork: DispatchWorkItem?
  var capabilitiesByEndpoint: [String: Set<String>] = [:]
  var capabilitiesLoading = Set<String>()
  var editingWorkspace: String? { didSet { updateInputGate() } }
  @Published var editingTab: String? { didSet { updateInputGate() } }
  func updateInputGate() {
    TerminalInputGate.blocked =
      coordinatorRecovery != nil || showFind || showPalette || showSettings || showAgentIntegrations
      || showWorktree || showRemote
      || showRemoteDirectory || worktreePicker != nil
      || editingWorkspace != nil
      || editingTab != nil
      || draggingPane != nil
      || isCreatingWorkspace(selected)
      || serverAction != nil || serverBusy.contains(currentEndpoint)
      || (!currentEndpoint.isEmpty && remoteMessages[currentEndpoint] != "Connected")
  }
  @Published var draggingTab: String?
  var dragOriginal: [Workspace]?
  @Published var paneErrors: [String: String] = [:]
  @Published var error = ""
  private var dismissedConnectionWarning: String?
  func dismissWarning() {
    if error == connectionError { dismissedConnectionWarning = connectionError }
    error = ""
  }
  @Published var ready = false
  @Published var connected = false
  @Published var starting = false
  var connectionError = ""
  @Published var fontScale =
    JamDefaults.shared.double(forKey: "fontScale") == 0
    ? 1 : JamDefaults.shared.double(forKey: "fontScale")
  { didSet { JamDefaults.shared.set(fontScale, forKey: "fontScale") } }
  private let queue = DispatchQueue(label: "jam.control")
  private var launchDirectory: URL?
  private var openedLaunchDirectory = false
  var timer: Timer?
  var loading = false
  var current: Workspace? {
    displayedWorkspaces.first { $0.id == selected }
      ?? workspaces.first { w in
        selectedMachine.isEmpty ? !w.id.contains("|") : w.id.hasPrefix(selectedMachine + "|")
      }
  }
  var currentTab: Tab? {
    guard surface == .terminals else { return nil }
    return current?.tabs.first { $0.id == tab } ?? current?.tabs.first
  }
  var visiblePaneIDs: [String] {
    if surface == .coordinator { return current?.coordinator.map { [$0.pane] } ?? [] }
    return currentTab?.root.ids ?? []
  }
  init() {
    if CommandLine.arguments.contains("--preview") {
      preview()
      return
    }
    do {
      launchDirectory = try LaunchDirectory.resolve(
        arguments: CommandLine.arguments,
        cwd: URL(fileURLWithPath: FileManager.default.currentDirectoryPath),
        home: FileManager.default.homeDirectoryForCurrentUser,
        fromTerminal: isatty(STDIN_FILENO) == 1 || isatty(STDOUT_FILENO) == 1
          || isatty(STDERR_FILENO) == 1)
    } catch { self.error = "\(error)" }
    reloadRemotes()
    reloadConfig()
    installShortcuts()
    if launchDirectory != nil { start() }
    timer = Timer.scheduledTimer(withTimeInterval: 0.6, repeats: true) { [weak self] _ in
      self?.refresh()
      self?.refreshRemotes()
      self?.refreshServers()
      self?.reloadConfig()
      self?.refreshGitSummaries()
      self?.refreshAgentUsage()
    }
  }
  func start() {
    guard !starting, !CommandLine.arguments.contains("--preview") else { return }
    starting = true
    let bin = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/jamd").path
    let root = ProcessInfo.processInfo.environment["JAM_DAEMON"] ?? bin
    queue.async {
      let result = nativeString(jam_start(root))
      DispatchQueue.main.async {
        if let values = try? JSONSerialization.jsonObject(with: Data(result.utf8))
          as? [String: Any], let error = values["error"] as? String
        {
          self.starting = false
          self.connectionError = error
          self.error = error
          self.connected = false
        } else {
          self.refreshCapabilities(force: true)
          self.ready = true
          self.connected = true
          if self.openedLaunchDirectory {
            self.starting = false
            self.refresh()
          } else {
            self.openInitialWorkspace()
          }
        }
      }
    }
  }
  func openInitialWorkspace() {
    guard let directory = launchDirectory else {
      starting = false
      return
    }
    starting = true
    queue.async {
      var reply = rpc("State")
      var targetID: String?
      if let state = reply["State"] as? [String: Any],
        let entries = state["workspaces"] as? [[String: Any]]
      {
        let existing = entries.first {
          ($0["path"] as? String).map {
            URL(fileURLWithPath: $0).standardizedFileURL.resolvingSymlinksInPath() == directory
          } ?? false
        }
        if let existing, let id = existing["id"] as? String {
          targetID = id
        } else {
          reply = rpc([
            "CreateWorkspace": ["path": directory.path, "name": ""]
          ])
          if let next = reply["State"] as? [String: Any],
            let ws = next["workspaces"] as? [[String: Any]]
          {
            targetID =
              ws.first(where: { $0["path"] as? String == directory.path })?["id"] as? String
          }
        }
      }
      let result = reply
      let workspace = targetID
      DispatchQueue.main.async {
        self.starting = false
        if let message = result["error"] as? String {
          self.error = message
          return
        }
        guard let state = result["State"] as? [String: Any], let workspace else {
          self.error = "Could not open the launch directory"
          return
        }
        self.apply(state)
        let saved = self.selections[workspace]
        let liveTabs =
          self.workspaces.first { $0.id == workspace }?.tabs.filter {
            $0.root.ids.contains { self.panes[$0]?.exited == false }
          } ?? []
        let targetTab =
          self.workspaces.first { $0.id == workspace }?.tabs.first { $0.id == saved?.0 }
          ?? liveTabs.first
        let livePanes = targetTab?.root.ids.filter { self.panes[$0]?.exited == false } ?? []
        let targetPane = livePanes.first { $0 == saved?.1 } ?? livePanes.first
        if self.currentEndpoint.isEmpty {
          self.select(workspace, tab: targetTab?.id, pane: targetPane)
        }
        self.openedLaunchDirectory = true
        self.error = ""
      }
    }
  }
  func requestEndpoint(_ value: Any) -> String {
    var localIDs = Set<String>()
    for workspace in workspaces where !workspace.id.contains("|") {
      localIDs.insert(workspace.id)
      for tab in workspace.tabs {
        localIDs.insert(tab.id)
        localIDs.formUnion(tab.root.ids)
      }
      if let coordinator = workspace.coordinator { localIDs.insert(coordinator.pane) }
    }
    return RequestRouting.endpoint(value, fallback: currentEndpoint, localIDs: localIDs)
  }
  func request(_ value: Any, renameNewTab: Bool = false) {
    let endpoint = requestEndpoint(value)
    guard !serverBusy.contains(endpoint) else { return }
    if !endpoint.isEmpty && remoteMessages[endpoint] != "Connected" {
      error = "Remote is unavailable" + (remoteMessages[endpoint].map { ".\n" + $0 } ?? "")
      return
    }
    let revision = stateRevisions[endpoint]
    // A state poll can learn about the new tab before this request completes.
    // Keep the baseline from the action, so auto-rename cannot be skipped.
    let tabsBeforeRequest = Set(workspaces.flatMap { $0.tabs.map(\.id) })
    let requestedWorkspace = ((value as? [String: Any])?["NewTab"] as? [String: String])?[
      "workspace"]
    endpointQueue(endpoint).async {
      let r = rpc(value, endpoint: endpoint, revision: revision)
      DispatchQueue.main.async {
        if let e = r["error"] as? String { self.error = e }
        if let state = r["State"] as? [String: Any] {
          let oldWorkspaces = Set(self.workspaces.map(\.id))
          let oldTabs =
            renameNewTab ? tabsBeforeRequest : Set(self.workspaces.flatMap { $0.tabs.map(\.id) })
          let oldPanes = Set(self.panes.keys)
          let newCount = (state["panes"] as? [[String: Any]])?.count ?? oldPanes.count
          if newCount < oldPanes.count {
            withAnimation(
              NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
                ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: 0.25)
            ) {
              self.apply(state, endpoint: endpoint)
            }
          } else {
            self.apply(state, endpoint: endpoint)
          }
          if self.currentEndpoint != endpoint { return }
          if let w = self.workspaces.first(where: { !oldWorkspaces.contains($0.id) }) {
            self.select(w.id)
          }
          if let w = self.workspaces.first(where: {
            (!renameNewTab || requestedWorkspace == nil || $0.id == requestedWorkspace)
              && $0.tabs.contains { !oldTabs.contains($0.id) }
          }),
            let t = w.tabs.first(where: { !oldTabs.contains($0.id) })
          {
            self.select(w.id, tab: t.id)
            if renameNewTab { self.editingTab = t.id }
          }
          if let p = self.currentTab?.root.ids.first(where: { !oldPanes.contains($0) }) {
            self.focus = p
          }
        }
        self.trimTerminalViews()
        self.refresh()
      }
    }
  }
  func refresh() {
    guard ready, !loading, !starting, !serverBusy.contains(""), draggingTab == nil,
      draggingPane == nil, !reorderingWorkspaces
    else { return }
    loading = true
    let orderGeneration = workspaceOrderGeneration
    let creationEpoch = workspaceCreationEpochs[""] ?? 0
    let settings = config?.hostSettings
    queue.async {
      let host = rpc("HostInfo")["HostInfo"] as? [String: Any]
      let agents = configureHost(
        settings, endpoint: "", incarnation: host?["incarnation"] as? String)
      let r = rpc("State")
      DispatchQueue.main.async {
        self.loading = false
        guard self.draggingTab == nil, self.draggingPane == nil, !self.reorderingWorkspaces,
          self.workspaceOrderGeneration == orderGeneration
        else {
          return
        }
        if let e = r["error"] as? String {
          self.connected = false
          self.connectionError = e
          if self.dismissedConnectionWarning != e { self.error = e }
          return
        }
        self.connected = true
        if let agents, self.availableAgents[""] != agents { self.availableAgents[""] = agents }
        if self.error == self.connectionError { self.error = "" }
        self.connectionError = ""
        self.dismissedConnectionWarning = nil
        guard let s = r["State"] as? [String: Any] else { return }
        self.observeServer(host, endpoint: "")
        self.applyRefresh(s, endpoint: "", creationEpoch: creationEpoch)
      }
    }
  }
  @discardableResult
  func applyRefresh(_ state: [String: Any], endpoint: String, creationEpoch: UInt64) -> Bool {
    guard creationEpoch == workspaceCreationEpochs[endpoint, default: 0] else { return false }
    apply(state, endpoint: endpoint)
    return true
  }
  func apply(_ s: [String: Any], endpoint supplied: String? = nil) {
    guard !reorderingWorkspaces else { return }
    let entries = s["workspaces"] as? [[String: Any]] ?? []
    let firstID = entries.first?["id"] as? String ?? ""
    let endpoint =
      supplied ?? (firstID.contains("|") ? String(firstID.split(separator: "|")[0]) : "")
    let revision = (s["revision"] as? NSNumber)?.uint64Value
    if let revision, let applied = stateRevisions[endpoint], revision < applied { return }
    stateRevisions[endpoint] = revision
    func belongs(_ id: String) -> Bool {
      endpoint.isEmpty ? !id.contains("|") : id.hasPrefix(endpoint + "|")
    }
    let keptPanes = panes.filter { !belongs($0.key) }
    let keptWorkspaces = workspaces.filter { !belongs($0.id) }

    let previousErrors = panes.mapValues { $0.launchError }
    let ps = s["panes"] as? [[String: Any]] ?? []
    panes = Dictionary(
      uniqueKeysWithValues: ps.compactMap { p in
        guard let id = p["id"] as? String else { return nil }
        return (
          id,
          Pane(
            id: id, title: p["title"] as? String ?? "Terminal", cwd: p["cwd"] as? String ?? "",
            status: p["status"] as? String ?? "unknown", exited: p["exited"] as? Bool ?? false,
            reports: p["reports"] as? [[String: Any]] ?? [],
            launchError: p["launch_error"] as? String,
            agentProvider: p["agent_provider"] as? String, agentRole: p["agent_role"] as? String)
        )
      })
    panes.merge(keptPanes) { current, _ in current }
    if let failed = panes.values.first(where: {
      $0.status == "recovery_failed" && $0.launchError != nil
        && (previousErrors[$0.id] ?? nil) != $0.launchError
    }) {
      error = failed.launchError ?? "Could not resume interrupted work"
    }
    workspaces =
      keptWorkspaces
      + (s["workspaces"] as? [[String: Any]] ?? []).compactMap { w in
        guard let id = w["id"] as? String else { return nil }
        let coordinator = WorkspaceCoordinator(w["coordinator"] as? [String: Any])
        // Older servers can still return a coordinator inside their tab layout
        // while awaiting upgrade. Present it only once, in its dedicated child.
        let tabs = (w["tabs"] as? [[String: Any]] ?? []).compactMap { t -> Tab? in
          guard let id = t["id"] as? String, let node = Node(t["root"] as Any),
            let root = node.removing(coordinator?.pane)
          else { return nil }
          let role = t["agent_role"] as? String
          return Tab(
            agentRole: role == "coordinator" ? nil : role, id: id,
            name: pendingRenames[id]?.name ?? t["name"] as? String ?? "Terminal",
            root: root)
        }
        let path = w["path"] as? String ?? ""
        let active =
          (w["active_pane"] as? String).flatMap { panes[$0] }.flatMap {
            $0.agentRole == "coordinator" ? nil : $0
          }
          ?? ps.first(where: {
            $0["workspace"] as? String == id && $0["agent_role"] as? String != "coordinator"
          }).flatMap { p in
            (p["id"] as? String).flatMap { panes[$0] }
          }
        let home =
          endpoint.isEmpty
          ? FileManager.default.homeDirectoryForCurrentUser.path : remoteHomes[endpoint]
        let name = WorkspaceName.display(
          w["name"] as? String ?? "Workspace", automatic: w["auto_name"] as? Bool == true,
          directory: active?.cwd ?? (w["last_cwd"] as? String) ?? path, home: home)
        return Workspace(
          id: id, name: pendingRenames[id]?.name ?? name, path: path,
          tabs: tabs, coordinator: coordinator,
          repository: w["repository"] as? String, mainPath: w["main_path"] as? String,
          branch: w["branch"] as? String, parent: w["parent"] as? String,
          context: (w["context"] as? [String: Any]).flatMap {
            guard let data = try? JSONSerialization.data(withJSONObject: $0) else { return nil }
            return try? JSONDecoder().decode(RepositoryContext.self, from: data)
          })
      }
    // Display old servers safely while they await an explicit upgrade.
    if s["workspace_model"] == nil || (s["workspace_model"] as? Int ?? 0) == 0 {
      for index in workspaces.indices where belongs(workspaces[index].id) {
        let w = workspaces[index]
        if w.path != w.mainPath, let repository = w.repository,
          let owner = workspaces.first(where: {
            belongs($0.id) && $0.repository == repository && $0.path == w.mainPath
          })
        {
          workspaces[index].parent = owner.id
        }
      }
    }
    workspaces = WorkspaceSelection.ordered(
      workspaces, machines: [""] + remotes.map(\.id), id: { $0.id })
    workspaceCreationOrigins = workspaceCreationOrigins.filter { origin in
      workspaces.contains { $0.id == origin.key }
    }
    let next =
      isCreatingWorkspace(selected)
      ? selected
      : WorkspaceSelection.reconcile(
        selected: selected, machine: currentEndpoint, refreshed: endpoint,
        workspaces: displayedWorkspaces.map(\.id))
    if next != selected { selected = next }
    if surface == .coordinator && current?.hasCoordinator != true {
      surface = .terminals
      if let saved = selections[selected] {
        tab = saved.0
        focus = saved.1
      }
    }
    if current == nil {
      if !tab.isEmpty { tab = "" }
      if !focus.isEmpty { focus = "" }
    } else if !visiblePaneIDs.contains(focus) {
      focus = visiblePaneIDs.first ?? ""
    }
    recoverVisiblePanes(endpoint: endpoint)
  }
  func openFolder() {
    if !currentEndpoint.isEmpty {
      showRemoteDirectory = true
      return
    }
    let p = NSOpenPanel()
    p.canChooseDirectories = true
    p.canChooseFiles = false
    p.prompt = "Open Workspace"
    if p.runModal() == .OK, let url = p.url {
      request(["CreateWorkspace": ["path": url.path, "name": ""]])
    }
  }
  func newWorkspace() {
    guard !serverBusy.contains(currentEndpoint) else { return }
    guard canNameWorkspaces || !currentEndpoint.isEmpty else {
      error = "New workspaces require the updated daemon. Existing shells are still running."
      return
    }
    let endpoint = currentEndpoint
    if let pending = workspaceCreations[endpoint] {
      select(pending.id)
      return
    }
    guard endpoint.isEmpty || remoteMessages[endpoint] == "Connected" else {
      error = "Remote is unavailable"
      return
    }
    let creation = WorkspaceCreation(
      endpoint: endpoint,
      home: endpoint.isEmpty
        ? FileManager.default.homeDirectoryForCurrentUser.path : remoteHomes[endpoint],
      previous: selected, knownWorkspaceIDs: Set(workspaces.map(\.id)))
    if surface == .terminals { selections[selected] = (tab, focus) }
    surface = .terminals
    workspaceCreationEpochs[endpoint, default: 0] &+= 1
    workspaceCreations[endpoint] = creation
    selected = creation.id
    tab = ""
    focus = ""
    endpointQueue(endpoint).async {
      // An unknown remote home must be resolved on that host, never on this Mac.
      let hostReply = creation.home == nil ? rpc("HostInfo", endpoint: endpoint) : [:]
      let home =
        creation.home ?? (hostReply["HostInfo"] as? [String: Any])?["home"] as? String
      let reply: [String: Any]
      if let home, !home.isEmpty {
        reply = rpc(["NewWorkspace": ["path": home]], endpoint: endpoint)
      } else {
        reply = [
          "error": hostReply["error"] as? String ?? "Could not determine the remote home directory"
        ]
      }
      DispatchQueue.main.async { self.finishWorkspaceCreation(creation, reply: reply) }
    }
  }
  func finishWorkspaceCreation(_ creation: WorkspaceCreation, reply: [String: Any]) {
    guard workspaceCreations[creation.endpoint]?.id == creation.id else { return }
    let state = reply["State"] as? [String: Any]
    // NewWorkspace appends one workspace and returns its state under the same daemon lock.
    let created = (state?["workspaces"] as? [[String: Any]])?.last?["id"] as? String
    let wasSelected = selected == creation.id
    let next = creation.selection(
      after: created, selected: selected, available: workspaces.map(\.id))
    workspaceCreationEpochs[creation.endpoint, default: 0] &+= 1
    if let created { workspaceCreationOrigins[created] = creation.id }
    // Keep the placeholder through snapshot reconciliation, then switch from it
    // so select() preserves the previously saved tab and pane on failure.
    if let state { apply(state, endpoint: creation.endpoint) }
    workspaceCreations.removeValue(forKey: creation.endpoint)
    if wasSelected {
      if !next.isEmpty {
        select(next)
      } else {
        selected = ""
        tab = ""
        focus = ""
      }
    }
    selections.removeValue(forKey: creation.id)
    if created == nil { error = reply["error"] as? String ?? "Could not create workspace" }
  }
  func reportFocus() {
    guard canNameWorkspaces, !focus.isEmpty else { return }
    let pane = focus
    endpointQueue(currentEndpoint).async { _ = rpc(["FocusPane": ["pane": pane]]) }
  }
  func restartPane(_ id: String, automatic: Bool = false) {
    if automatic && panes[id]?.agentRole != nil { return }
    guard !serverBusy.contains(WorkspaceSelection.endpoint(of: id)) else { return }
    guard panes[id]?.exited == true || panes[id]?.status == "recovery_failed",
      restartingPanes.insert(id).inserted
    else { return }
    let endpoint = WorkspaceSelection.endpoint(of: id)
    let managed = panes[id]?.agentRole != nil
    let generation = serverGenerations[endpoint, default: 0]
    paneErrors.removeValue(forKey: id)
    endpointQueue(endpoint).async {
      var request: [String: Any] = ["Restart": ["pane": id]]
      if managed,
        let data = rpc(
          ["Coordination": ["action": "list", "include_archived": true]], endpoint: endpoint)[
            "Coordination"]
          as? [String: Any],
        let agents = data["agents"] as? [[String: Any]],
        let agent = agents.first(where: { $0["pane"] as? String == id }),
        let agentID = agent["id"] as? String
      {
        request = ["Coordination": ["action": "resume", "agent": agentID]]
      }
      let reply = rpc(request, endpoint: endpoint)
      DispatchQueue.main.async {
        self.restartingPanes.remove(id)
        guard self.serverGenerations[endpoint, default: 0] == generation else { return }
        if let error = reply["error"] as? String { self.error = error }
        if let state = reply["State"] as? [String: Any] { self.apply(state, endpoint: endpoint) }
      }
    }
  }
  func newTab() {
    guard let w = current, !isCreatingWorkspace(w.id) else { return }
    request(["NewTab": ["workspace": w.id]], renameNewTab: true)
  }
  func canSplit(_ vertical: Bool, pane: String? = nil) -> Bool {
    guard let size = paneSizes[pane ?? focus] else { return false }
    return panes[pane ?? focus]?.agentRole != "coordinator"
      && SplitLayout.canSplit(size, vertical: vertical)
  }
  func split(_ vertical: Bool) {
    guard
      let target = currentTab?.root.ids.first(where: { $0 == focus }) ?? currentTab?.root.ids.first
    else { return }
    guard canSplit(vertical, pane: target) else { return }
    request(["Split": ["pane": target, "vertical": vertical]])
  }
  func close(_ pane: String) {
    request(["Close": ["pane": pane]])
  }
  func closeTab(_ target: Tab) {
    let endpoint = WorkspaceSelection.endpoint(of: target.id)
    guard endpoint.isEmpty || remoteMessages[endpoint] == "Connected" else {
      error = "Remote is unavailable"
      return
    }
    guard closingTabs.insert(target.id).inserted else { return }
    endpointQueue(endpoint).async {
      let reply = rpc(["CloseTab": ["tab": target.id]], endpoint: endpoint)
      let state = reply["State"] as? [String: Any]
      let error = reply["error"] as? String
      DispatchQueue.main.async {
        self.closingTabs.remove(target.id)
        if let state { self.apply(state, endpoint: endpoint) }
        if let error { self.error = error }
        if self.tab == target.id,
          !self.workspaces.contains(where: { $0.tabs.contains { $0.id == target.id } })
        {
          self.tab = self.current?.tabs.first?.id ?? ""
          self.focus = self.currentTab?.root.ids.first ?? ""
          self.focusTerminal()
        }
        self.trimTerminalViews()
      }
    }
  }
  func preview() {
    if PreviewCapture.loadStateIfRequested(into: self) { return }
    ready = false
    let p1 = Pane(
      id: "preview-a", title: "zsh", cwd: "~/Projects/jam", status: "idle", exited: false)
    let p2 = Pane(
      id: "preview-b", title: "Build", cwd: "~/Projects/jam", status: "working", exited: false)
    panes = [p1.id: p1, p2.id: p2]
    workspaces = [
      Workspace(
        id: "jam", name: "jam", path: "~/Projects/jam",
        tabs: [
          Tab(
            id: "main", name: "Development", root: .split(false, 0.54, .leaf(p1.id), .leaf(p2.id)))
        ]), Workspace(id: "notes", name: "Playground", path: "~/Projects/playground", tabs: []),
    ]
    if CommandLine.arguments.contains("--capture-demo") {
      connected = true
      workspaces[0].repository = "~/Projects/jam"
      workspaces[0].mainPath = "~/Projects/jam"
      workspaces[0].branch = "main"
      workspaces[0].coordinator = WorkspaceCoordinator(
        id: "preview-coordinator", pane: "preview-coordinator", provider: "claude")
      workspaces.append(
        Workspace(
          id: "worktree", name: "terminal-search", path: "~/Worktrees/jam/terminal-search",
          tabs: [],
          repository: "~/Projects/jam", mainPath: "~/Projects/jam",
          branch: "feature/terminal-search",
          parent: "jam"))
      panes["preview-coordinator"] = Pane(
        id: "preview-coordinator", title: "Claude", cwd: "~/Projects/jam", status: "idle",
        exited: false, agentProvider: "claude", agentRole: "coordinator")
      let demoRemote = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
      remotes = [
        RemoteMachine(id: demoRemote, name: "Build server", destination: "demo@example.invalid")
      ]
      remoteMessages[demoRemote] = "Connected"
      remotePersistence[demoRemote] = true
      loadedRemotes.insert(demoRemote)
      workspaces.append(
        Workspace(
          id: demoRemote + "|docs", name: "documentation", path: "~/Projects/docs", tabs: []))
      agentUsage[""] = [
        AgentUsageReport(
          provider: "claude", profileKey: "demo", accountKey: "demo", accountLabel: "Demo account",
          activeSessions: 1,
          windows: [
            AgentUsageWindow(key: "five_hour", label: "5h", percent: 31, resetsAt: nil),
            AgentUsageWindow(key: "weekly", label: "7d", percent: 18, resetsAt: nil),
          ], extra: nil, checking: false, checkedAt: nil, lastSuccessAt: nil, error: nil)
      ]
    }
    selected = "jam"
    tab = "main"
    focus = p1.id
  }
}
@main struct JamApp: App {
  init() {
    handleSSHAskpass()
    PreviewCapture.installIfRequested()
  }
  @StateObject var store = Store()
  var body: some Scene {
    Window("Jam", id: "main") {
      ContentView().environmentObject(store).onAppear { PreviewCapture.installIfRequested() }
        .preferredColorScheme(
          store.config?.appearance == "dark"
            ? .dark : store.config?.appearance == "light" ? .light : nil
        ).frame(minWidth: 800, minHeight: 520)
        .sheet(isPresented: $store.showSettings) {
          AppSettingsView(store: store, close: { store.showSettings = false })
            .frame(width: 860, height: 600)
        }
        .sheet(isPresented: $store.showPalette) { CommandPalette().environmentObject(store) }
        .sheet(isPresented: $store.showRemote) {
          RemoteForm(remote: store.editingRemote).environmentObject(store)
        }
        .sheet(isPresented: $store.showRemoteDirectory) {
          RemoteDirectoryPicker(endpoint: store.currentEndpoint).environmentObject(store)
        }

        .sheet(item: $store.coordinatorRecovery) { recovery in
          CoordinatorRecoveryView(recovery: recovery).environmentObject(store)
        }
        .sheet(isPresented: $store.showAgentIntegrations) { AgentIntegrations() }
        .sheet(isPresented: $store.showWorktree) { WorktreeForm().environmentObject(store) }
        .sheet(item: $store.worktreePicker) { context in
          WorktreePicker(context: context).environmentObject(store)
        }
        .sheet(item: $store.serverAction) { action in
          ServerActionSheet(action: action).environmentObject(store)
        }
    }.defaultSize(width: 1100, height: 760).windowStyle(.hiddenTitleBar).windowToolbarStyle(
      .unifiedCompact
    ).commands {
      CommandGroup(after: .appSettings) {
        Button("Check for Updates…") { store.appUpgrade.check() }
        Button("Settings…") { store.execute("settings") }.keyboardShortcut(",", modifiers: .command)
        Button("Agent Integrations…") { store.execute("agent_integrations") }
      }
      CommandMenu("Terminal") {
        Button("Command Palette…") { store.execute("palette") }.keyboardShortcut(
          store.shortcut("palette"))
        Button("New Worktree…") { store.execute("new_worktree") }.disabled(
          !store.enabled("new_worktree"))
        Button("Open Worktree…") { store.execute("open_worktree") }.disabled(
          !store.enabled("open_worktree"))
        Button("Increase Font Size") { store.fontScale = min(2, store.fontScale + 0.1) }
          .keyboardShortcut("+")
        Button("Decrease Font Size") { store.fontScale = max(0.7, store.fontScale - 0.1) }
          .keyboardShortcut("-")
        Button("Reset Font Size") { store.fontScale = 1 }.keyboardShortcut("0")
      }
      CommandGroup(replacing: .newItem) {
        Button("New Workspace") { store.execute("new_workspace") }
        Button("Open Workspace…") { store.execute("open_workspace") }.keyboardShortcut(
          store.shortcut("open_workspace"))
        Button("New Tab") { store.execute("new_tab") }.keyboardShortcut(store.shortcut("new_tab"))
        Button("Close Pane") { store.execute("close_pane") }.keyboardShortcut(
          store.shortcut("close_pane"))
        Divider()
        Button("Split Right") { store.execute("split_right") }.keyboardShortcut(
          store.shortcut("split_right"))
        Button("Split Down") { store.execute("split_down") }.keyboardShortcut(
          store.shortcut("split_down"))
      }
    }
  }
}
struct ContentView: View {
  @EnvironmentObject var store: Store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  var body: some View {
    NavigationSplitView {
      VStack(spacing: 4) {
        HStack {
          Text("WORKSPACES").font(.caption).foregroundStyle(.secondary)
          Spacer()
          Button(action: store.newWorkspace) {
            Image(systemName: "plus").frame(width: 28, height: 28).contentShape(Rectangle())
          }.buttonStyle(.plain).help("New Workspace")
        }.padding(.leading, 12).padding(.trailing, 4).padding(.vertical, 5)
        WorkspaceSidebar(store: store)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
        UsagePanel().environmentObject(store)
        MachineList().environmentObject(store)
      }.navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
    } detail: {
      VStack(spacing: 0) {
        if store.displayedWorkspaces.isEmpty && store.currentEndpoint.isEmpty {
          HStack {
            Spacer()
            Button(action: store.openSettings) {
              Image(systemName: "gearshape").frame(width: 28, height: 28).contentShape(Rectangle())
            }.buttonStyle(.plain).help("Settings")
          }.frame(height: 34).padding(.trailing, 6)
          VStack(spacing: 12) {
            if store.starting {
              ProgressView("Opening terminal…")
            } else if store.error.isEmpty {
              Button("Open Terminal") { store.openInitialWorkspace() }
            } else {
              Text("Unable to open terminal").foregroundStyle(.secondary)
            }
          }.frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
          if store.surface == .terminals {
            HStack(spacing: 2) {
              if let current = store.current {
                ScrollViewReader { proxy in
                  ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 3) {
                      ForEach(store.current?.tabs ?? []) { t in
                        DraggableTab(
                          id: t.id, title: t.name,
                          selected: store.currentTab?.id == t.id,
                          activityStatus: store.activityStatus(for: t),
                          editing: store.editingTab == t.id,
                          edit: { store.editingTab = t.id },
                          rename: { value in
                            store.editingTab = nil
                            if let name = value?.trimmingCharacters(in: .whitespacesAndNewlines),
                              !name.isEmpty
                            {
                              store.rename(t.id, to: name)
                            }
                            store.focusTerminal()
                          },
                          click: { if let w = store.current { store.select(w.id, tab: t.id) } },
                          close: { store.closeTab(t) },
                          begin: { store.beginTabDrag(t.id) }, end: { store.cancelTabDrag() },
                          siblings: store.current?.tabs.map(\.id) ?? [],
                          reorder: { store.previewTabMove(to: $0) },
                          drop: {
                            guard let id = store.draggingTab,
                              let w = store.workspaces.first(where: {
                                $0.tabs.contains { $0.id == id }
                              }
                              )
                            else { return false }
                            return store.finishTabDrag(workspace: w.id)
                          }
                        )
                        .frame(
                          width: TabDragView.preferredWidth(
                            for: t.name, showsStatus: store.activityStatus(for: t) != nil),
                          height: 28
                        )
                        .background(
                          store.currentTab?.id == t.id ? Color.primary.opacity(0.07) : .clear,
                          in: RoundedRectangle(cornerRadius: 5)
                        )
                        .opacity(store.draggingTab == t.id ? 0 : 1)
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                        .id(t.id).accessibilityLabel(t.name).accessibilityValue(
                          store.currentTab?.id == t.id ? "Selected" : ""
                        )
                        .buttonStyle(.plain).contextMenu {
                          Button("Rename") { store.editingTab = t.id }
                          Divider()
                          Button("Close Tab") { store.closeTab(t) }
                            .disabled(store.closingTabs.contains(t.id))
                        }
                      }
                      Spacer()
                      if current.tabs.contains(where: { !$0.root.ids.isEmpty }) {
                        Button(action: store.newTab) {
                          Image(systemName: "plus").frame(width: 28, height: 28).contentShape(
                            Rectangle())
                        }.buttonStyle(.plain).help("New Tab")
                          .disabled(store.isCreatingWorkspace(current.id))
                          .transition(.opacity)
                      }
                    }
                    .animation(
                      reduceMotion ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: 0.22),
                      value: current.tabs.map(\.id)
                    )
                    .id(current.id)
                  }.scrollIndicators(.never).frame(height: 34).padding(.horizontal, 6)
                    .onChange(of: store.tab) { _, value in
                      withAnimation(
                        reduceMotion ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: 0.22)
                      ) {
                        proxy.scrollTo(value, anchor: .trailing)
                      }
                    }
                }
              }
              if store.current == nil { Spacer() }
              Button {
                store.openSettings()
              } label: {
                Image(systemName: "gearshape").frame(width: 28, height: 28).contentShape(
                  Rectangle())
              }.buttonStyle(.plain).help("Settings").padding(.trailing, 6)
            }
          }
          if store.surface == .coordinator, let coordinator = store.current?.coordinator {
            PaneCard(id: coordinator.pane).padding(5)
          } else if store.isCreatingWorkspace(store.selected) {
            ProgressView("Creating workspace…")
              .frame(maxWidth: .infinity, maxHeight: .infinity)
          } else if let t = store.currentTab {
            GeometryReader { geometry in
              let viewport = CGSize(
                width: max(0, geometry.size.width - 10), height: max(0, geometry.size.height - 10))
              let canvas = SplitLayout.canvas(viewport: viewport)
              Group {
                if t.root.ids.isEmpty {
                  VStack(spacing: 12) {
                    Text("No terminals").foregroundStyle(.secondary)
                    Button("New Terminal") { store.request(["NewPane": ["tab": t.id]]) }
                  }.frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(EmptyPaneDropSurface(tab: t.id))
                } else if let root = t.root.removing(store.liftedPane) {
                  // Structural animations belong to edits within a tab, never navigation.
                  PaneTree(node: root, tab: t.id).id(t.id)
                } else {
                  RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.025))
                    .overlay(Text("Drop pane here").foregroundStyle(.secondary))
                    .background(EmptyPaneDropSurface(tab: t.id))
                }
              }.frame(width: canvas.width, height: canvas.height).padding(5).clipped()
            }
          } else {
            VStack(spacing: 12) {
              Text(store.current == nil ? "No workspaces" : "No terminals")
                .font(.title2).fontWeight(.semibold)
              if store.current == nil && !store.currentEndpoint.isEmpty {
                Text(store.remoteMessages[store.currentEndpoint] ?? "Connecting…")
                  .foregroundStyle(.secondary).multilineTextAlignment(.center)
              }
              if store.current == nil {
                Button("New Workspace") { store.newWorkspace() }
                  .disabled(
                    !store.currentEndpoint.isEmpty
                      && store.remoteMessages[store.currentEndpoint] != "Connected")
              } else {
                Button("New Tab") { store.execute("new_tab") }
              }
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
              .background(Color(nsColor: .windowBackgroundColor))
          }
        }
        if store.busy && !store.movingLayout {
          ProgressView("Working…").controlSize(.small).padding(6)
        }
      }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottomTrailing) {
          AppStatusOverlay(store: store, controller: store.appUpgrade)
        }
        .overlay(alignment: .topTrailing) {
          if store.showFind {
            WorkspaceFindBar(finder: store.finder).padding(.top, 38).padding(.trailing, 12)
              .transition(.move(edge: .top).combined(with: .opacity))
          }
        }
    }.tint(Color(nsColor: .controlAccentColor))
  }
}
struct PaneTree: View {
  @EnvironmentObject var store: Store
  let node: Node
  let tab: String
  @State private var ratios: [String: Double] = [:]
  @State private var ratioPaneIDs: [String] = []
  @State private var resizing: String?
  var body: some View {
    GeometryReader { geometry in
      let layout = PaneLayout(
        node: node, size: geometry.size, ratios: ratioPaneIDs == node.ids ? ratios : [:])
      ZStack(alignment: .topLeading) {
        ForEach(layout.panes) { pane in
          GeometryReader { proxy in
            PaneCard(id: pane.id).frame(width: proxy.size.width, height: proxy.size.height)
              .clipped()
              .onAppear {
                if store.paneSizes[pane.id] != proxy.size { store.paneSizes[pane.id] = proxy.size }
              }
              .onChange(of: proxy.size) { _, size in
                if store.paneSizes[pane.id] != size { store.paneSizes[pane.id] = size }
              }
          }.frame(width: pane.frame.width, height: pane.frame.height)
            .offset(x: pane.frame.minX, y: pane.frame.minY)
            .transition(.identity)
        }
        ForEach(layout.dividers) { divider in
          Rectangle().fill(Color.clear)
            .frame(width: divider.frame.width, height: divider.frame.height)
            .contentShape(Rectangle())
            .offset(x: divider.frame.minX, y: divider.frame.minY)
            .transition(.identity)
            .onHover { inside in
              if inside {
                if divider.vertical {
                  NSCursor.resizeUpDown.set()
                } else {
                  NSCursor.resizeLeftRight.set()
                }
              } else {
                NSCursor.arrow.set()
              }
            }
            .gesture(
              DragGesture(coordinateSpace: .named("pane-layout-\(tab)"))
                .onChanged { value in
                  resizing = divider.id
                  ratioPaneIDs = node.ids
                  let total = divider.vertical ? divider.parent.height : divider.parent.width
                  let position =
                    divider.vertical
                    ? value.location.y - divider.parent.minY
                    : value.location.x - divider.parent.minX
                  ratios[divider.id] = min(
                    0.9, max(0.1, position / max(1, total - SplitLayout.divider)))
                }
                .onEnded { _ in
                  resizing = nil
                  if let ratio = ratios[divider.id] {
                    store.request(["Ratio": ["tab": tab, "path": divider.path, "ratio": ratio]])
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                      if resizing != divider.id && ratios[divider.id] == ratio {
                        ratios.removeValue(forKey: divider.id)
                      }
                    }
                  }
                })
        }
      }.coordinateSpace(name: "pane-layout-\(tab)")
        .animation(WorkspaceFinder.animation, value: node.ids)
        .onChange(of: node.ids) { _, _ in ratios.removeAll() }
    }.transition(.identity)
  }
}
struct PaneCard: View {
  @EnvironmentObject var store: Store
  let id: String
  var pane: Pane? { store.panes[id] }
  var body: some View {
    VStack(spacing: 0) {
      HStack(spacing: 8) {
        if let pane, pane.isAIPane {
          ActivityIndicator(status: pane.activityStatus).frame(width: 6, height: 6)
        }
        AgentBrandIcon(provider: pane?.agentProvider)
        Text(
          pane?.agentProvider.map {
            AgentBrand.name($0)
              + (pane?.agentRole == "coordinator"
                ? " · Coordinator" : pane?.agentRole == "reviewer" ? " · Reviewer" : "")
          } ?? pane?.title ?? "Terminal"
        )
        .font(.system(size: 11, weight: .medium, design: .monospaced)).foregroundStyle(.secondary)
        .lineLimit(1)
        .truncationMode(.middle)
        Text(URL(fileURLWithPath: pane?.cwd ?? "").lastPathComponent).font(
          .system(size: 10, design: .monospaced)
        )
        .foregroundStyle(.secondary)
        Spacer()
        if pane?.status == "recovery_failed" {
          Button("Resume") { store.restartPane(id) }
        }
        Text(
          pane?.status == "unknown"
            ? "" : pane?.status == "recovery_failed" ? "Recovery paused" : (pane?.status ?? "")
        ).foregroundStyle(
          pane?.status == "blocked" || pane?.status == "error" ? Color.orange : Color.secondary
        )
        if pane?.agentRole != "coordinator" {
          Button {
            store.focus = id
            store.split(false)
          } label: {
            Image(systemName: "rectangle.split.2x1")
          }.help(
            store.canSplit(false, pane: id) ? "Split right" : "Not enough space to split right"
          )
          .disabled(!store.canSplit(false, pane: id))
          Button {
            store.focus = id
            store.split(true)
          } label: {
            Image(systemName: "rectangle.split.1x2")
          }.help(store.canSplit(true, pane: id) ? "Split down" : "Not enough space to split down")
            .disabled(!store.canSplit(true, pane: id))
        }
        Button {
          store.close(id)
        } label: {
          Image(systemName: "xmark")
        }.help(pane?.agentRole == "coordinator" ? "Close Coordinator" : "Close terminal")
      }.buttonStyle(.plain).font(.system(size: 10)).padding(.horizontal, 12).frame(height: 34)
        .background(Color.primary.opacity(0.025))
        .overlay(alignment: .leading) {
          if pane?.agentRole != "coordinator" {
            GeometryReader { geometry in
              PaneDragHandle(pane: id, store: store).frame(
                width: max(0, geometry.size.width - 100), height: 34)
            }
          }
        }
      Divider()
      if id.hasPrefix("preview") {
        Text(
          id == "preview-a"
            ? "❯ git status\nOn branch main\n\nNothing to commit, working tree clean.\n\n❯ "
            : "❯ cargo build\n   Compiling jam-core\n   Compiling jam-render\n\n    Finished dev profile\n\nWatching for changes…"
        ).font(.system(size: 12, design: .monospaced)).foregroundStyle(.primary)
          .padding(16).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
          .background(Color(nsColor: TerminalAppearance.background))
      } else if pane?.exited == true {
        if store.restartingPanes.contains(id) {
          ProgressView("Starting shell…").frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
          if pane?.agentRole != nil {
            AgentStoppedView(pane: pane!).environmentObject(store)
          } else {
            VStack(spacing: 12) {
              ContentUnavailableView(
                "Session stopped", systemImage: "pause.circle",
                description: Text("Start a new shell to continue in this folder."))
              Button("Start Shell") { store.restartPane(id) }
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
          }
        }
      } else {
        TerminalSurface(
          pool: store.terminalViews, pane: id, fontScale: store.fontScale,
          copyOnSelect: store.config?.copyOnSelect ?? false,
          focused: store.focus == id,
          onFocus: {
            store.focus = id
            store.request(["Acknowledge": ["pane": id]])
          }, onError: { store.paneErrors[id] = $0 }
        ).id(
          "\(id)-\(pane?.exited ?? false)"
        ).frame(maxWidth: .infinity, maxHeight: .infinity)
          .padding(2)
          .background(Color(nsColor: TerminalAppearance.background))
      }
    }.overlay(alignment: .bottomLeading) {
      if pane?.exited != true, let e = store.paneErrors[id] {
        Text(WarningContent(e).summary).font(.caption).padding(6).background(.regularMaterial)
      }
    }.clipShape(RoundedRectangle(cornerRadius: 10)).overlay(
      RoundedRectangle(cornerRadius: 10).strokeBorder(
        store.focus == id
          ? Color(nsColor: .controlAccentColor).opacity(0.45) : Color.primary.opacity(0.12),
        lineWidth: 1)
    ).shadow(color: .black.opacity(0.07), radius: 10, y: 4).padding(1)
      .background(PaneDropSurface(pane: id))
      .opacity(store.draggingPane == id ? 0.65 : 1)
      .onTapGesture { store.focus = id }
  }
}
struct TerminalSurface: NSViewRepresentable {
  let pool: TerminalViewPool
  let pane: String
  let fontScale: Double
  let copyOnSelect: Bool
  let focused: Bool
  let onFocus: () -> Void
  let onError: (String?) -> Void
  func makeNSView(context: Context) -> TerminalSurfaceHost {
    TerminalSurfaceHost(terminal: pool.view(for: pane))
  }
  func updateNSView(_ host: TerminalSurfaceHost, context: Context) {
    let v = host.terminal
    v.copyOnSelect = copyOnSelect
    v.fontScale = fontScale
    v.onFocus = onFocus
    v.onError = onError
    if v.focused != focused {
      v.focused = focused
      if focused && !TerminalInputGate.blocked { v.window?.makeFirstResponder(v) }
    }
  }
}
final class DisplayTarget: NSObject {
  weak var owner: TerminalNSView?
  @objc func tick() { owner?.render() }
}
final class TerminalNSView: NSView, NSTextInputClient, NSMenuItemValidation {
  let pane: String
  weak var layoutHost: TerminalSurfaceHost?
  var handle: UnsafeMutableRawPointer?
  var displayClock: CADisplayLink?
  var displayTarget: DisplayTarget?
  var onFocus: (() -> Void)?
  var marked = NSAttributedString(string: "")
  var onError: ((String?) -> Void)?
  var meta: [String: Any] = [:]
  private var lastAccessibilityText = ""
  var lastError = ""
  var fontScale: Double = 1
  var focused = false
  var tracking: NSTrackingArea?
  var copyOnSelect = false
  var selectionOrigin: NSEvent?
  var selecting = false
  var applicationDrag = false
  var scrollRemainder: Double = 0
  let composition = NSTextField(labelWithString: "")
  var cellSize: NSSize {
    let scale = window?.backingScaleFactor ?? 1
    return NSSize(
      width: (8.4 * scale * fontScale).rounded() / scale,
      height: (19 * scale * fontScale).rounded() / scale)
  }
  var cursorRect: NSRect {
    NSRect(
      x: CGFloat(meta["cursor_x"] as? Int ?? 0) * cellSize.width,
      y: bounds.height - CGFloat((meta["cursor_y"] as? Int ?? 0) + 1) * cellSize.height,
      width: cellSize.width, height: cellSize.height)
  }
  var activeKey = 0
  var activeModifiers = 0
  var pressed: [UInt16: (Int, String, Int)] = [:]
  let inputQueue = DispatchQueue(label: "jam.input")
  init(pane: String) {
    self.pane = pane
    super.init(frame: .zero)
    wantsLayer = true
    layer?.backgroundColor = TerminalAppearance.background.cgColor
    setAccessibilityElement(true)
    setAccessibilityRole(.textArea)
    setAccessibilityLabel("Terminal")
    setAccessibilityIdentifier("terminal.\(pane)")
    composition.font = .monospacedSystemFont(ofSize: 14, weight: .regular)
    composition.textColor = .labelColor
    composition.backgroundColor = .textBackgroundColor
    composition.drawsBackground = true
    composition.isHidden = true
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override var acceptsFirstResponder: Bool { true }
  override func isAccessibilityEnabled() -> Bool { !TerminalInputGate.blocked }
  override func isAccessibilityFocused() -> Bool { window?.firstResponder === self }
  override func accessibilityValue() -> Any? {
    let value = action("text")
    let object = try? JSONSerialization.jsonObject(with: Data(value.utf8)) as? [String: Any]
    return object?["text"] as? String ?? ""
  }
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    displayClock?.invalidate()
    displayClock = nil
    displayTarget = nil
    guard window != nil else { return }
    invalidateFrame()
    prepareForDisplay()
    if focused {
      DispatchQueue.main.async { [weak self] in
        if let self, self.focused, !TerminalInputGate.blocked {
          self.window?.makeFirstResponder(self)
        }
      }
    }
    let target = DisplayTarget()
    target.owner = self
    displayTarget = target
    let clock = displayLink(target: target, selector: #selector(DisplayTarget.tick))
    clock.add(to: .main, forMode: .common)
    displayClock = clock
  }
  func invalidateFrame() {
    if let handle { jam_view_invalidate(handle) }
  }
  func prepareForDisplay() {
    guard let window, bounds.width > 0, bounds.height > 0 else { return }
    if handle == nil {
      let scale = window.backingScaleFactor
      handle = jam_view_new(
        Unmanaged.passUnretained(self).toOpaque(), pane,
        UInt32(bounds.width * scale), UInt32(bounds.height * scale))
      if handle == nil {
        onError?("The terminal GPU renderer could not start. Check the app's stderr for details.")
        return
      }
      updateTerminalAppearance()
      addSubview(composition)
    }
    // Submit a cached frame during layout, without waiting for a display-link tick.
    render()
  }
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    updateTerminalAppearance()
  }
  private func updateTerminalAppearance() {
    effectiveAppearance.performAsCurrentDrawingAppearance {
      guard let background = TerminalAppearance.background.usingColorSpace(.sRGB),
        let foreground = NSColor.labelColor.usingColorSpace(.sRGB)
      else { return }
      layer?.backgroundColor = background.cgColor
      func rgb(_ color: NSColor) -> UInt32 {
        let r = UInt32((color.redComponent * 255).rounded())
        let g = UInt32((color.greenComponent * 255).rounded())
        let b = UInt32((color.blueComponent * 255).rounded())
        return (r << 16) | (g << 8) | b
      }
      if let handle { jam_view_set_colors(handle, rgb(foreground), rgb(background)) }
    }
  }
  func action(_ a: String, _ x: Int32 = 0, _ y: Int32 = 0) -> String {
    guard let handle else { return "{}" }
    return nativeString(jam_view_action(handle, a, x, y))
  }
  @objc func displayTick() { render() }
  func render() {
    guard let handle, let window, !isHiddenOrHasHiddenAncestor, bounds.width > 0, bounds.height > 0
    else { return }
    let s = window.backingScaleFactor
    let result = nativeString(
      jam_view_draw(
        handle, UInt32(bounds.width * s), UInt32(bounds.height * s), Float(s * fontScale)))
    if let values = try? JSONSerialization.jsonObject(with: Data(result.utf8)) as? [String: Any] {
      meta = values
      if let e = values["error"] as? String, e != lastError {
        lastError = e
        onError?(e)
      }
      if values["error"] is NSNull && !lastError.isEmpty {
        lastError = ""
        onError?(nil)
      }
      if let text = values["text"] as? String, lastAccessibilityText != text {
        lastAccessibilityText = text
        NSAccessibility.post(element: self, notification: .valueChanged)
      }
      updateComposition()
    }
  }
  deinit {
    displayClock?.invalidate()
    if let handle { jam_view_drop(handle) }
  }
  func modifiers(_ event: NSEvent) -> Int {
    (event.modifierFlags.contains(.shift) ? Int(JAM_MOD_SHIFT) : 0)
      | (event.modifierFlags.contains(.control) ? Int(JAM_MOD_CONTROL) : 0)
      | (event.modifierFlags.contains(.option) ? Int(JAM_MOD_ALT) : 0)
      | (event.modifierFlags.contains(.command) ? Int(JAM_MOD_SUPER) : 0)
  }
  func mouse(_ event: NSEvent, _ action: Int, _ button: Int) {
    let p = convert(event.locationInWindow, from: nil)
    let s = window?.backingScaleFactor ?? 1
    let value: [String: Any] = [
      "Mouse": [
        "pane": pane, "action": action, "button": button, "x": max(0, p.x * s),
        "y": max(0, (bounds.height - p.y) * s), "cell_width": Int((8.4 * s * fontScale).rounded()),
        "cell_height": Int((19 * s * fontScale).rounded()), "mods": modifiers(event),
      ]
    ]
    inputQueue.async { _ = rpc(value) }
  }
  func localSelect(_ event: NSEvent, _ extend: Bool) {
    let p = convert(event.locationInWindow, from: nil)
    _ = action(
      extend ? "extend" : "select", Int32(max(0, p.x / cellSize.width)),
      Int32(max(0, (bounds.height - p.y) / cellSize.height)))
  }
  override func mouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    onFocus?()
    applicationDrag = meta["mouse"] as? Bool == true && !event.modifierFlags.contains(.shift)
    if applicationDrag {
      mouse(event, 0, 0)
    } else {
      _ = action("clear_selection")
      selectionOrigin = event
      selecting = false
    }
  }
  override func mouseDragged(with event: NSEvent) {
    if applicationDrag {
      mouse(event, 2, 0)
    } else {
      guard let origin = selectionOrigin else { return }
      if !selecting {
        let delta = hypot(
          event.locationInWindow.x - origin.locationInWindow.x,
          event.locationInWindow.y - origin.locationInWindow.y)
        guard delta >= 3 else { return }
        localSelect(origin, false)
        selecting = true
      }
      localSelect(event, true)
    }
  }
  override func mouseUp(with event: NSEvent) {
    if applicationDrag { mouse(event, 1, 0) } else if selecting && copyOnSelect { copy(nil) }
    selectionOrigin = nil
    selecting = false
    applicationDrag = false
  }
  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    if let tracking { removeTrackingArea(tracking) }
    let area = NSTrackingArea(
      rect: bounds,
      options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect, .enabledDuringMouseDrag],
      owner: self, userInfo: nil)
    addTrackingArea(area)
    tracking = area
  }
  override func mouseMoved(with event: NSEvent) {
    if meta["mouse"] as? Bool == true { mouse(event, 2, 255) }
  }
  override func rightMouseDragged(with event: NSEvent) {
    if meta["mouse"] as? Bool == true { mouse(event, 2, 1) }
  }
  override func rightMouseDown(with event: NSEvent) {
    if meta["mouse"] as? Bool == true && !event.modifierFlags.contains(.shift) {
      mouse(event, 0, 1)
    } else {
      window?.makeFirstResponder(self)
      onFocus?()
      let menu = NSMenu()
      menu.allowsContextMenuPlugIns = false
      let item = menu.addItem(withTitle: "Copy", action: #selector(copy(_:)), keyEquivalent: "")
      item.target = self
      let pasteItem = menu.addItem(
        withTitle: "Paste", action: #selector(paste(_:)), keyEquivalent: "")
      pasteItem.target = self
      NSMenu.popUpContextMenu(menu, with: event, for: self)
    }
  }
  override func rightMouseUp(with event: NSEvent) {
    if meta["mouse"] as? Bool == true && !event.modifierFlags.contains(.shift) {
      mouse(event, 1, 1)
    }
  }
  override func scrollWheel(with event: NSEvent) {
    scrollRemainder += event.scrollingDeltaY / (event.hasPreciseScrollingDeltas ? 8 : 1)
    let steps = Int(scrollRemainder)
    scrollRemainder -= Double(steps)
    guard steps != 0 else { return }
    if meta["mouse"] as? Bool == true {
      for _ in 0..<min(20, abs(steps)) { mouse(event, 0, steps > 0 ? 3 : 4) }
    } else if meta["alternate"] as? Bool == true {
      for _ in 0..<min(20, abs(steps)) { key(steps > 0 ? 78 : 75) }
    } else {
      _ = action("scroll", 0, Int32(-steps))
    }
  }
  func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
    if menuItem.action == #selector(copy(_:)) { return !selectedText().isEmpty }
    if menuItem.action == #selector(paste(_:)) {
      return NSPasteboard.general.string(forType: .string) != nil
    }
    return true
  }
  func selectedText() -> String {
    let raw = action("copy")
    return ((try? JSONSerialization.jsonObject(with: Data(raw.utf8))) as? [String: Any])?["text"]
      as? String ?? ""
  }
  @objc func copy(_ sender: Any?) {
    let text = selectedText()
    guard !text.isEmpty else { return }
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
  }

  func send(_ text: String) {
    let data = Array(text.utf8)
    inputQueue.async { _ = rpc(["Send": ["pane": self.pane, "data": data]]) }
  }
  func key(_ code: Int, _ text: String = "", _ mods: Int = 0, release: Bool = false) {
    if !release { _ = action("bottom") }
    inputQueue.async {
      _ = rpc([
        "Key": ["pane": self.pane, "key": code, "text": text, "mods": mods, "release": release]
      ])
    }
  }
  override func keyDown(with event: NSEvent) {
    guard !TerminalInputGate.blocked else { return }
    // AppKit hardware key codes -> libghostty-vt's pinned public Key enum.
    let special: [UInt16: Int] = [
      36: 58, 76: 58, 48: 64, 51: 53, 53: 120, 115: 71, 119: 69, 114: 72, 117: 68, 116: 74, 121: 73,
      123: 76, 124: 77, 125: 75, 126: 78, 122: 121, 120: 122, 99: 123, 118: 124, 96: 125, 97: 126,
      98: 127, 100: 128, 101: 129, 109: 130, 103: 131, 111: 132,
    ]
    activeModifiers = modifiers(event)
    let scalar = event.charactersIgnoringModifiers?.lowercased().unicodeScalars.first?.value ?? 0
    activeKey = special[event.keyCode] ?? ((97...122).contains(scalar) ? 20 + Int(scalar - 97) : 0)
    if !hasMarkedText(), activeKey != 0 {
      pressed[event.keyCode] = (activeKey, event.charactersIgnoringModifiers ?? "", activeModifiers)
    }
    defer {
      activeKey = 0
      activeModifiers = 0
    }
    if !hasMarkedText(), let code = special[event.keyCode] {
      key(code, "", modifiers(event))
      return
    }
    if !hasMarkedText(), event.modifierFlags.contains(.control),
      let text = event.charactersIgnoringModifiers, let c = text.lowercased().unicodeScalars.first,
      c.value >= 97 && c.value <= 122
    {
      key(20 + Int(c.value - 97), text, modifiers(event))
      return
    }
    interpretKeyEvents([event])
  }
  override func keyUp(with event: NSEvent) {
    if let (code, text, mods) = pressed.removeValue(forKey: event.keyCode) {
      key(code, text, mods, release: true)
    }
  }
  func insertText(_ string: Any, replacementRange: NSRange) {
    guard !TerminalInputGate.blocked else { return }
    let s = (string as? NSAttributedString)?.string ?? string as? String ?? ""
    marked = NSAttributedString(string: "")
    updateComposition()
    key(activeKey, s, activeModifiers)
  }
  override func doCommand(by selector: Selector) {
    let map: [String: Int] = [
      "insertNewline:": 58, "deleteBackward:": 53, "insertTab:": 64, "cancelOperation:": 120,
      "moveUp:": 78, "moveDown:": 75, "moveRight:": 77, "moveLeft:": 76,
      "moveToBeginningOfLine:": 71, "moveToEndOfLine:": 69, "pageUp:": 74, "pageDown:": 73,
    ]
    if let code = map[NSStringFromSelector(selector)] { key(code) }
  }
  @objc func paste(_ sender: Any?) {
    guard !TerminalInputGate.blocked else { return }
    if let text = NSPasteboard.general.string(forType: .string) {
      inputQueue.async { _ = rpc(["Paste": ["pane": self.pane, "text": text]]) }
    }
  }
  func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
    marked = (string as? NSAttributedString) ?? NSAttributedString(string: string as? String ?? "")
    updateComposition()
  }
  func updateComposition() {
    composition.font = .monospacedSystemFont(ofSize: 14 * fontScale, weight: .regular)
    composition.stringValue = marked.string
    composition.isHidden = marked.length == 0
    if marked.length > 0 {
      composition.sizeToFit()
      composition.frame.origin = cursorRect.origin
    }
  }
  func unmarkText() {
    marked = NSAttributedString(string: "")
    updateComposition()
  }
  func selectedRange() -> NSRange { NSRange(location: 0, length: 0) }
  func markedRange() -> NSRange {
    NSRange(location: marked.length > 0 ? 0 : NSNotFound, length: marked.length)
  }
  func hasMarkedText() -> Bool { marked.length > 0 }
  func attributedSubstring(forProposedRange: NSRange, actualRange: NSRangePointer?)
    -> NSAttributedString?
  { nil }
  func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }
  func firstRect(forCharacterRange: NSRange, actualRange: NSRangePointer?) -> NSRect {
    window?.convertToScreen(convert(cursorRect, to: nil)) ?? .zero
  }
  func characterIndex(for: NSPoint) -> Int { 0 }
}
