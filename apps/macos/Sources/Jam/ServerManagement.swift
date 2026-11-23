import AppKit
import CJam
import SwiftUI

extension Store {
  func machinePresentation(_ endpoint: String) -> MachinePresentation {
    let connection: RemoteConnectionState =
      endpoint.isEmpty
      ? (starting ? .connecting : connected ? .connected : .disconnected)
      : remoteConnectionState(endpoint)
    return MachinePresentation(
      status: serverStatuses[endpoint], connection: connection,
      phase: serverPhases[endpoint], hasError: serverActionErrors[endpoint] != nil,
      unresponsive: endpoint.isEmpty && localTimeout != nil)
  }

  func machineActions(_ endpoint: String) -> MachineActions {
    MachineActions(
      connected: endpoint.isEmpty ? connected : remoteMessages[endpoint] == "Connected",
      connecting: endpoint.isEmpty ? starting : remoteConnectionState(endpoint).isAttempting,
      busy: serverBusy.contains(endpoint), checking: checkingServers.contains(endpoint),
      status: serverStatuses[endpoint])
  }
  func logServerStatus(_ event: String, endpoint: String, state: MachineActions) {
    let observation =
      "\(event) endpoint=\(endpoint.isEmpty ? "local" : endpoint) "
      + "connected=\(state.connected) connecting=\(state.connecting) "
      + "busy=\(state.busy) checking=\(state.checking) "
      + "running=\(state.status?.running ?? false) compatible=\(state.status?.compatible ?? false) "
      + "build=\(state.status?.build ?? "none") available=\(state.status?.availableBuild ?? "none") "
      + "update=\(state.status?.updateAvailable ?? false) canUpdate=\(state.canUpdate) "
      + "terminals=\(state.status?.terminals ?? -1)"
    NSLog("Jam server status: %@", observation)
  }
  func reconnectMachine(_ endpoint: String) {
    if (endpoint.isEmpty && localTimeout != nil)
      || serverStatuses[endpoint]?.state == "inspection_failed"
    {
      showServerAction(endpoint, kind: .inspect)
      return
    }
    if serverActionErrors[endpoint] != nil {
      showServerAction(
        endpoint, kind: serverStatuses[endpoint]?.blocked == true ? .takeover : .restart)
      return
    }
    if let status = serverStatuses[endpoint], status.blocked {
      if status.state == "client_update_required" {
        appUpgrade.check()
      } else if machineActions(endpoint).canTakeover {
        showServerAction(endpoint, kind: .takeover)
      }
      return
    }
    guard machineActions(endpoint).canReconnect else { return }
    if endpoint.isEmpty { start() } else { connectRemote(endpoint) }
  }
  // Host incarnations change only when the daemon process changes. An ordinary
  // transport reconnect must not relaunch a shell that the user deliberately exited.
  func observeServer(_ host: [String: Any]?, endpoint: String) {
    guard let incarnation = host?["incarnation"] as? String else { return }
    if serverIncarnations[endpoint] != incarnation {
      pendingServerRecovery.insert(endpoint)
      stateRevisions.removeValue(forKey: endpoint)
      appliedPolls.removeValue(forKey: endpoint)
      serverIncarnations[endpoint] = incarnation
    }
  }
  func recoverVisiblePanes(endpoint: String) {
    guard !serverBusy.contains(endpoint), pendingServerRecovery.remove(endpoint) != nil else {
      return
    }
    paneErrors = paneErrors.filter { WorkspaceSelection.endpoint(of: $0.key) != endpoint }
    guard currentEndpoint == endpoint else { return }
    for id in visiblePaneIDs { restartPane(id, automatic: true) }
  }

  private func serverRequest(
    _ action: String, endpoint: String, force: Bool = false, interactive: Bool = false,
    extra: [String: Any] = [:]
  ) -> [String: Any] {
    var request: [String: Any] = [
      "action": action, "endpoint": endpoint, "force": force,
      "daemon": ProcessInfo.processInfo.environment["JAM_DAEMON"]
        ?? Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/jamd").path,
      "artifacts": Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/remote").path,
    ]
    request.merge(extra) { _, new in new }
    if interactive { request["askpass"] = Bundle.main.executableURL?.path }
    guard let data = try? JSONSerialization.data(withJSONObject: request) else {
      return ["error": "Could not prepare the server request."]
    }
    return
      (try? JSONSerialization.jsonObject(
        with: Data(nativeString(jam_server(String(decoding: data, as: UTF8.self))).utf8))
      as? [String: Any]) ?? ["error": "The server returned an invalid response."]
  }
  func refreshServers() {
    for endpoint in [""] + remotes.map(\.id) where !pausedRemotes.contains(endpoint) {
      let interval = scheduledServerUpdates.contains(endpoint) ? 5.0 : 15.0
      // Status is a passive local-socket probe, including existing SSH tunnels.
      // Probe disconnected endpoints too so protocol upgrades remain reachable.
      if Date().timeIntervalSince(serverCheckDates[endpoint] ?? .distantPast) >= interval {
        checkServer(endpoint)
      }
    }
  }
  func refreshServerAfterConnection(_ endpoint: String) {
    // A passive probe can finish before the SSH forwarding socket exists.
    // Discard that result, including a reply still in flight, once connected.
    serverGenerations[endpoint, default: 0] &+= 1
    serverStatuses.removeValue(forKey: endpoint)
    serverCheckDates.removeValue(forKey: endpoint)
    checkServer(endpoint)
  }
  func checkServer(_ endpoint: String) {
    guard !serverBusy.contains(endpoint), checkingServers.insert(endpoint).inserted else { return }
    serverCheckDates[endpoint] = Date()
    let generation = serverGenerations[endpoint, default: 0]
    DispatchQueue.global(qos: .utility).async {
      let result = self.serverRequest("status", endpoint: endpoint)
      let decoded = (try? JSONSerialization.data(withJSONObject: result)).flatMap {
        try? JSONDecoder().decode(ServerStatus.self, from: $0)
      }
      DispatchQueue.main.async {
        self.checkingServers.remove(endpoint)
        guard self.serverGenerations[endpoint, default: 0] == generation else {
          if !self.serverBusy.contains(endpoint),
            endpoint.isEmpty || self.remotes.contains(where: { $0.id == endpoint })
          {
            self.checkServer(endpoint)
          }
          return
        }
        self.serverStatuses[endpoint] =
          decoded
          ?? ServerStatus(
            running: false, compatible: false, build: nil, availableBuild: nil,
            updateAvailable: false, terminals: nil,
            error: result["error"] as? String ?? "Could not inspect the server",
            state: "inspection_failed", managementAvailable: false)
        if let minimum = decoded?.minimumVersion {
          self.appUpgrade.requireClientUpgrade(endpoint: endpoint, minimum: minimum, manual: false)
        }
        // A transient failed launch must not strand a now-responsive local daemon.
        if endpoint.isEmpty, decoded?.compatible == true, decoded?.blocked != true,
          !self.ready, !self.starting
        {
          self.start()
        }
        self.logServerStatus("checked", endpoint: endpoint, state: self.machineActions(endpoint))
        guard self.scheduledServerUpdates.contains(endpoint), let status = decoded else { return }
        if status.running && status.compatible && !status.updateAvailable
          && status.availableBuild != nil
        {
          self.scheduledServerUpdates.remove(endpoint)
        } else if status.readyForScheduledUpdate {
          self.performServerAction(endpoint, kind: .update, force: false, interactive: false)
        }
      }
    }
  }
  func showServerAction(_ endpoint: String, kind: ServerAction.Kind) {
    guard !serverBusy.contains(endpoint) else { return }
    let name = endpoint.isEmpty ? "Local" : remotes.first { $0.id == endpoint }?.name ?? endpoint
    serverAction = ServerAction(endpoint: endpoint, name: name, kind: kind)
    if kind == .takeover {
      serverGenerations[endpoint, default: 0] &+= 1
      let generation = serverGenerations[endpoint, default: 0]
      serverTokens.removeValue(forKey: endpoint)
      checkingServers.insert(endpoint)
      DispatchQueue.global().async {
        let reply = self.serverRequest("prepare_takeover", endpoint: endpoint, interactive: true)
        DispatchQueue.main.async {
          guard self.serverGenerations[endpoint, default: 0] == generation else { return }
          self.checkingServers.remove(endpoint)
          self.serverTokens[endpoint] = reply["token"] as? String
          if let rawStatus = reply["status"],
            let data = try? JSONSerialization.data(withJSONObject: rawStatus),
            let status = try? JSONDecoder().decode(ServerStatus.self, from: data)
          {
            self.serverStatuses[endpoint] = status
          }
          if let error = reply["error"] as? String { self.serverActionErrors[endpoint] = error }
        }
      }
    } else {
      checkServer(endpoint)
    }
  }
  func scheduleServerUpdate(_ endpoint: String) {
    scheduledServerUpdates.insert(endpoint)
    serverAction = nil
    checkServer(endpoint)
  }
  func performServerAction(
    _ endpoint: String, kind: ServerAction.Kind, force: Bool, interactive: Bool = true
  ) {
    guard serverBusy.insert(endpoint).inserted else { return }
    serverActionErrors.removeValue(forKey: endpoint)
    serverGenerations[endpoint, default: 0] &+= 1
    if !endpoint.isEmpty {
      remoteConnectionGenerations[endpoint, default: 0] &+= 1
      remoteLoading.remove(endpoint)
      remoteConnecting.remove(endpoint)
    }
    workspaceOrderGeneration &+= 1
    let confirmation = serverTokens.removeValue(forKey: endpoint) ?? ""
    DispatchQueue.global(qos: .userInitiated).async {
      var result: [String: Any]
      do {
        let started = self.serverRequest(
          kind == .takeover ? "takeover" : "begin_operation", endpoint: endpoint, force: force,
          interactive: interactive,
          extra: ["token": confirmation, "kind": kind == .update ? "update" : "restart"])
        result = started
        if let id = started["id"] as? String {
          repeat {
            result = self.serverRequest("operation", endpoint: endpoint, extra: ["id": id])
            let phase = result["phase"] as? String ?? "preparing"
            DispatchQueue.main.async { self.serverPhases[endpoint] = phase }
            if result["done"] as? Bool != true && result["error"] == nil {
              Thread.sleep(forTimeInterval: 0.2)
            }
          } while result["done"] as? Bool != true && result["error"] == nil
          if let failure = result["failure"] as? [String: Any] {
            result["error"] =
              (failure["summary"] as? String ?? "Server action failed.") + "\n"
              + (failure["details"] as? String ?? "")
          }
        }
      }
      DispatchQueue.main.async {
        self.serverBusy.remove(endpoint)
        self.serverPhases.removeValue(forKey: endpoint)
        self.serverGenerations[endpoint, default: 0] &+= 1
        self.workspaceOrderGeneration &+= 1
        if result["deferred"] as? Bool == true {
          self.scheduledServerUpdates.insert(endpoint)
          self.serverCheckDates[endpoint] = Date()
          return
        } else {
          self.scheduledServerUpdates.remove(endpoint)
          if let error = result["error"] as? String {
            self.serverActionErrors[endpoint] = error
          } else {
            self.serverAction = nil
            self.pendingServerRecovery.insert(endpoint)
            self.capabilitiesByEndpoint.removeValue(forKey: endpoint)
            if endpoint.isEmpty {
              // Initial startup can fail against an older protocol. A verified
              // replacement must enable refresh even if startup never reached ready.
              self.ready = true
              self.connected = true
              self.refresh()
            } else {
              self.pausedRemotes.remove(endpoint)
              self.remoteMessages[endpoint] = "Connected"
              self.remoteNext[endpoint] = .distantPast
              self.refreshRemotes()
            }
            if self.currentEndpoint == endpoint { self.refreshCapabilities(force: true) }
          }
        }
        self.checkServer(endpoint)
      }
    }
  }
}

struct ServerActionSheet: View {
  let action: ServerAction
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) private var dismiss
  private var status: ServerStatus? { store.serverStatuses[action.endpoint] }
  private var checking: Bool {
    store.checkingServers.contains(action.endpoint)
      || (status == nil && store.serverActionErrors[action.endpoint] == nil)
  }
  private var progressMessage: String {
    guard store.serverBusy.contains(action.endpoint) else { return "Checking server…" }
    switch store.serverPhases[action.endpoint] {
    case "connecting": return "Connecting…"
    case "restoring": return "Restoring…"
    case "installing": return "Installing…"
    default: return "Preparing…"
    }
  }
  private var explanation: String {
    if action.kind == .inspect {
      return
        "Could not verify the server. Jam will keep retrying without stopping running sessions."
    }
    var parts: [String] = []
    if action.kind == .takeover {
      if status?.state == "recovery_required" {
        return
          "Finish the interrupted server operation and reconnect. Running terminals may stop. Saved workspaces and layouts will be preserved."
      }
      let reason =
        status?.state == "takeover_required"
        ? "The server belongs to another Jam installation."
        : "The server uses an unsupported version."
      return reason
        + " Replace it with this app’s server and reconnect. Running terminals will stop. Saved workspaces and layouts will be preserved."
    }
    if action.kind == .update {
      if status?.availableBuild == nil {
        parts.append("No matching server is bundled. Rebuild the app with its server bundle.")
      } else if status?.compatible == true && status?.updateAvailable == false {
        parts.append("The server is up to date.")
      }
    }
    if let count = status?.terminals {
      parts.append(
        count == 0
          ? "No terminals are running."
          : "Restarting will close \(count) running terminal\(count == 1 ? "" : "s").")
    } else {
      parts.append(
        "The running sessions could not be counted. Restarting will close all terminals on this machine."
      )
    }
    parts.append("Workspace layout is preserved. Previous commands will not be replayed.")
    return parts.joined(separator: " ")
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text(
        action.kind == .inspect
          ? "Server — \(action.name)"
          : action.kind == .takeover
            ? "Take over the server on \(action.name)?"
            : "\(action.kind == .update ? "Update" : "Restart") Server — \(action.name)"
      ).font(
        .headline)
      if store.serverBusy.contains(action.endpoint) || checking {
        ProgressView(progressMessage)
          .frame(maxWidth: .infinity, alignment: .center)
      } else {
        Text(explanation).textSelection(.enabled)
        if let error = store.serverActionErrors[action.endpoint]
          ?? (action.endpoint.isEmpty ? store.localTimeout : nil) ?? status?.error,
          !error.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
          ErrorNotice(message: error)
        }
      }
      Divider()
      HStack {
        Spacer()
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        if action.kind == .inspect {
          Button("Retry") { store.checkServer(action.endpoint) }
            .disabled(checking).keyboardShortcut(.defaultAction)
        } else if action.kind == .takeover {
          Button(
            store.serverActionErrors[action.endpoint] != nil
              ? "Try Again"
              : status?.state == "recovery_required"
                ? "Recover and Reconnect" : "Take Over and Reconnect"
          ) {
            if store.serverActionErrors[action.endpoint] != nil {
              store.serverActionErrors.removeValue(forKey: action.endpoint)
              store.showServerAction(action.endpoint, kind: .takeover)
            } else {
              store.performServerAction(action.endpoint, kind: .takeover, force: true)
            }
          }.disabled(
            (store.serverTokens[action.endpoint] == nil
              && store.serverActionErrors[action.endpoint] == nil)
              || store.serverBusy.contains(action.endpoint))
        } else if action.kind == .update {
          if status?.compatible == true && (status?.terminals ?? 0) > 0 {
            Button("Update When Sessions End") { store.scheduleServerUpdate(action.endpoint) }
              .disabled(checking || status?.updateAvailable != true)
          }
          Button(
            (status?.terminals ?? 1) > 0 || status?.compatible != true
              ? "Update and Restart" : "Update"
          ) {
            store.performServerAction(
              action.endpoint, kind: .update,
              force: (status?.terminals ?? 1) > 0 || status?.compatible != true)
          }.disabled(checking || status?.updateAvailable != true)
        } else {
          Button("Restart Server") {
            store.performServerAction(action.endpoint, kind: .restart, force: true)
          }
          .disabled(checking).keyboardShortcut(.defaultAction)
        }
      }
    }.disabled(store.serverBusy.contains(action.endpoint)).interactiveDismissDisabled(
      store.serverBusy.contains(action.endpoint)
    ).padding(24).frame(width: 540)
      .onChange(of: status?.state) { _, state in
        if action.kind == .inspect, state != nil, state != "inspection_failed",
          !(action.endpoint.isEmpty && store.localTimeout != nil), status?.error == nil
        {
          dismiss()
        }
      }
  }
}
