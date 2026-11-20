import AppKit
import CJam
import SwiftUI

extension Store {
  func machineActions(_ endpoint: String) -> MachineActions {
    MachineActions(
      connected: endpoint.isEmpty ? connected : remoteMessages[endpoint] == "Connected",
      connecting: endpoint.isEmpty ? starting : remoteConnectionState(endpoint).isAttempting,
      busy: serverBusy.contains(endpoint), checking: checkingServers.contains(endpoint),
      status: serverStatuses[endpoint])
  }
  func reconnectMachine(_ endpoint: String) {
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
    _ action: String, endpoint: String, force: Bool = false, interactive: Bool = false
  ) -> [String: Any] {
    var request: [String: Any] = [
      "action": action, "endpoint": endpoint, "force": force,
      "daemon": Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/jamd").path,
      "artifacts": Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/remote").path,
    ]
    if interactive { request["askpass"] = Bundle.main.executableURL?.path }
    guard let data = try? JSONSerialization.data(withJSONObject: request) else { return [:] }
    return
      (try? JSONSerialization.jsonObject(
        with: Data(nativeString(jam_server(String(decoding: data, as: UTF8.self))).utf8))
      as? [String: Any]) ?? [:]
  }
  func refreshServers() {
    for endpoint in [""] + remotes.map(\.id) {
      let interval = scheduledServerUpdates.contains(endpoint) ? 5.0 : 15.0
      // Status is a passive local-socket probe, including existing SSH tunnels.
      // Probe disconnected endpoints too so protocol upgrades remain reachable.
      if Date().timeIntervalSince(serverCheckDates[endpoint] ?? .distantPast) >= interval {
        checkServer(endpoint)
      }
    }
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
            error: result["error"] as? String ?? "Could not inspect the server")
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
    checkServer(endpoint)
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
    if serverAction?.endpoint == endpoint { serverAction = nil }
    serverGenerations[endpoint, default: 0] &+= 1
    workspaceOrderGeneration &+= 1
    DispatchQueue.global(qos: .userInitiated).async {
      let result = self.serverRequest(
        kind == .update ? "update" : "restart", endpoint: endpoint, force: force,
        interactive: interactive
      )
      DispatchQueue.main.async {
        self.serverBusy.remove(endpoint)
        self.serverGenerations[endpoint, default: 0] &+= 1
        self.workspaceOrderGeneration &+= 1
        if result["deferred"] as? Bool == true {
          self.scheduledServerUpdates.insert(endpoint)
          self.serverCheckDates[endpoint] = Date()
          return
        } else {
          self.scheduledServerUpdates.remove(endpoint)
          if let error = result["error"] as? String {
            self.error = error
          } else {
            self.pendingServerRecovery.insert(endpoint)
            self.capabilitiesByEndpoint.removeValue(forKey: endpoint)
            if endpoint.isEmpty {
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
  private var checking: Bool { store.checkingServers.contains(action.endpoint) || status == nil }
  private var explanation: String {
    var parts: [String] = []
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
    parts.append("Workspace layout is preserved; previous commands will not be replayed.")
    return parts.joined(separator: " ")
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("\(action.kind == .update ? "Update" : "Restart") Server — \(action.name)").font(
        .headline)
      if checking {
        ProgressView("Checking server…")
      } else {
        Text(explanation).textSelection(.enabled)
        if let error = status?.error, !error.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
          ErrorNotice(message: error)
        }
      }
      Divider()
      HStack {
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        Spacer()
        if action.kind == .update {
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
    }.padding(24).frame(width: 540)
  }
}
