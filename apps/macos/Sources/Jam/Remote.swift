import AppKit
import CJam
import SwiftUI

private let endpointQueueLock = NSLock()
private var endpointQueues: [String: DispatchQueue] = [:]
func endpointQueue(_ endpoint: String) -> DispatchQueue {
  endpointQueueLock.lock()
  defer { endpointQueueLock.unlock() }
  if let queue = endpointQueues[endpoint] { return queue }
  let queue = DispatchQueue(label: "jam.control." + endpoint)
  endpointQueues[endpoint] = queue
  return queue
}

struct RemoteMachine: Identifiable, Decodable {
  let id: String
  let name: String
  let destination: String
}
func remoteCommand(_ command: [String: String]) -> [String: Any] {
  guard let data = try? JSONSerialization.data(withJSONObject: command) else { return [:] }
  let text = nativeString(jam_remote(String(decoding: data, as: UTF8.self)))
  return (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] ?? [:]
}
extension Store {
  func disconnectRemote(_ id: String) {
    appUpgrade.forgetRequirement(endpoint: id)
    remoteConnectionGenerations[id, default: 0] &+= 1
    remoteConnecting.remove(id)
    remoteLoading.remove(id)
    pausedRemotes.insert(id)
    remoteMessages[id] = "Disconnected"
    DispatchQueue.global().async { _ = remoteCommand(["action": "disconnect", "id": id]) }
  }
  func enableRemotePersistence(_ id: String) {
    guard enablingPersistence.insert(id).inserted else { return }
    DispatchQueue.global().async {
      let reply = remoteCommand(["action": "enable_persistence", "id": id])
      DispatchQueue.main.async {
        self.enablingPersistence.remove(id)
        if let error = reply["error"] as? String {
          self.error = error
        } else {
          self.remotePersistence[id] = true
          self.remoteNext[id] = .distantPast
          self.refreshRemotes()
        }
      }
    }
  }
  func reloadRemotes() {
    let text = nativeString(jam_remote("{\"action\":\"list\"}"))
    remotes = (try? JSONDecoder().decode([RemoteMachine].self, from: Data(text.utf8))) ?? []
    appUpgrade.retainRequirements(endpoints: Set(remotes.map(\.id)))
    for remote in remotes {
      if !workspaces.contains(where: { $0.id.hasPrefix(remote.id + "|") }),
        let data = JamDefaults.shared.data(forKey: "remoteState." + remote.id),
        let state = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
      {
        apply(state, endpoint: remote.id)
      }
    }
  }
  func removeRemote(_ id: String) {
    DispatchQueue.global().async {
      let result = remoteCommand(["action": "remove", "id": id])
      DispatchQueue.main.async {
        if let error = result["error"] as? String {
          self.error = error
          return
        }
        self.appUpgrade.forgetRequirement(endpoint: id)
        self.workspaces.removeAll { $0.id.hasPrefix(id + "|") }
        self.workspaceCreations.removeValue(forKey: id)
        self.loadedRemotes.remove(id)
        self.remoteConnecting.remove(id)
        self.remoteLoading.remove(id)
        self.remoteConnectionGenerations[id, default: 0] &+= 1
        self.panes = self.panes.filter { !$0.key.hasPrefix(id + "|") }
        self.reloadRemotes()
      }
    }
  }
  func connectRemote(_ id: String) {
    pausedRemotes.remove(id)
    guard let remote = remotes.first(where: { $0.id == id }), !remoteLoading.contains(id) else {
      return
    }
    remoteLoading.insert(id)
    remoteConnecting.insert(id)
    remoteConnectionGenerations[id, default: 0] &+= 1
    let connectionGeneration = remoteConnectionGenerations[id, default: 0]
    capabilitiesByEndpoint.removeValue(forKey: id)
    DispatchQueue.global().async {
      let result = remoteCommand([
        "action": "connect", "id": remote.id,
        "askpass": Bundle.main.executableURL?.path ?? "",
        "artifacts": Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/remote").path,
      ])
      DispatchQueue.main.async {
        guard self.remoteConnectionGenerations[id, default: 0] == connectionGeneration,
          self.remotes.contains(where: { $0.id == id })
        else { return }
        self.remoteLoading.remove(id)
        self.remoteConnecting.remove(id)
        self.remoteMessages[id] = result["error"] as? String ?? "Connected"
        if let required = result["upgrade_required"] as? [String: Any],
          let minimum = required["minimum"] as? String
        {
          self.appUpgrade.requireClientUpgrade(endpoint: id, minimum: minimum, manual: true)
        }
        if let persistent = result["persistent"] as? Bool {
          self.remotePersistence[id] = persistent
        }
        if result["error"] == nil {
          self.appUpgrade.forgetRequirement(endpoint: id)
          self.observeServer(result["HostInfo"] as? [String: Any], endpoint: id)
        }
        self.remoteNext[id] = .distantPast
        self.refreshRemotes()
        if self.currentEndpoint == id { self.refreshCapabilities(force: true) }
      }
    }
  }
  func refreshRemotes() {
    for remote in remotes
    where !pausedRemotes.contains(remote.id) && !remoteLoading.contains(remote.id)
      && !serverBusy.contains(remote.id)
      && serverStatuses[remote.id]?.blocked != true
      && (remoteNext[remote.id] ?? .distantPast) < Date()
    {
      remoteLoading.insert(remote.id)
      if remoteMessages[remote.id] != "Connected" { remoteConnecting.insert(remote.id) }
      let connectionGeneration = remoteConnectionGenerations[remote.id, default: 0]
      let orderGeneration = workspaceOrderGeneration
      let creationEpoch = workspaceCreationEpochs[remote.id] ?? 0
      let settings = config?.hostSettings
      DispatchQueue.global().async {
        let connection = remoteCommand([
          "action": "connect", "id": remote.id,
          "artifacts": Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/remote")
            .path,
        ])
        let agents =
          connection["error"] == nil
          ? configureHost(
            settings, endpoint: remote.id,
            incarnation: (connection["HostInfo"] as? [String: Any])?["incarnation"] as? String)
          : nil
        let state = connection["error"] == nil ? rpc("State", endpoint: remote.id) : connection
        DispatchQueue.main.async {
          guard self.remoteConnectionGenerations[remote.id, default: 0] == connectionGeneration
          else { return }
          self.remoteLoading.remove(remote.id)
          self.remoteConnecting.remove(remote.id)
          guard self.remotes.contains(where: { $0.id == remote.id }) else { return }
          if let error = state["error"] as? String {
            if let required = connection["upgrade_required"] as? [String: Any],
              let minimum = required["minimum"] as? String
            {
              self.appUpgrade.requireClientUpgrade(
                endpoint: remote.id, minimum: minimum, manual: false)
            }
            self.remoteMessages[remote.id] = error
            self.checkServer(remote.id)
            let delay = min(30, (self.remoteDelay[remote.id] ?? 0.5) * 2)
            self.remoteDelay[remote.id] = delay
            self.remoteNext[remote.id] = Date().addingTimeInterval(
              delay * Double.random(in: 0.8...1.2))
          } else {
            self.appUpgrade.forgetRequirement(endpoint: remote.id)
            self.remoteMessages[remote.id] = "Connected"
            if let agents, self.availableAgents[remote.id] != agents {
              self.availableAgents[remote.id] = agents
            }
            if let persistent = connection["persistent"] as? Bool {
              self.remotePersistence[remote.id] = persistent
            }
            self.ready = true
            self.remoteDelay[remote.id] = 0.5
            self.remoteNext[remote.id] = Date().addingTimeInterval(0.6)
            if let host = connection["HostInfo"] as? [String: Any] {
              self.remoteHomes[remote.id] = host["home"] as? String
            }
            if let value = state["State"] as? [String: Any] {
              if !self.reorderingWorkspaces && self.workspaceOrderGeneration == orderGeneration
                && self.workspaceCreationEpochs[remote.id, default: 0] == creationEpoch
              {
                self.observeServer(connection["HostInfo"] as? [String: Any], endpoint: remote.id)
                self.applyRefresh(value, endpoint: remote.id, creationEpoch: creationEpoch)
                if let data = try? JSONSerialization.data(withJSONObject: value) {
                  JamDefaults.shared.set(data, forKey: "remoteState." + remote.id)
                }
                let firstLoad = self.loadedRemotes.insert(remote.id).inserted
                if firstLoad && self.currentEndpoint == remote.id {
                  self.refreshCapabilities(force: true)
                }
              }
            }
          }
        }
      }
    }
  }
}
struct RemoteForm: View {
  let remote: RemoteMachine?
  init(remote: RemoteMachine? = nil) {
    self.remote = remote
    _name = State(initialValue: remote?.name ?? "")
    _destination = State(initialValue: remote?.destination ?? "")
  }
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) var dismiss
  @State private var name = ""
  @State private var destination = ""
  @State private var busy = false
  @State private var error = ""
  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Text(remote == nil ? "Add Remote" : "Edit Remote").font(.headline)
      TextField("Name", text: $name)
      TextField("SSH destination, e.g. dev@dev-server", text: $destination).disabled(remote != nil)
      Text("Uses your SSH configuration and agent. New host keys require confirmation.").font(
        .caption
      )
      .foregroundStyle(.secondary)
      if !error.isEmpty { ErrorNotice(message: error) }
      HStack {
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        Spacer()
        if busy { ProgressView().controlSize(.small) }
        Button(remote == nil ? "Add" : "Save") {
          busy = true
          DispatchQueue.global().async {
            let result = remoteCommand([
              "action": remote == nil ? "add" : "rename", "id": remote?.id ?? "", "name": name,
              "destination": destination,
            ])
            DispatchQueue.main.async {
              busy = false
              if let message = result["error"] as? String {
                error = message
              } else {
                store.reloadRemotes()
                if let id = result["id"] as? String { store.connectRemote(id) }
                dismiss()
              }
            }
          }
        }.disabled(busy || name.isEmpty || destination.isEmpty).keyboardShortcut(.defaultAction)
      }
    }.padding(24).frame(width: 420)
  }
}
struct RemoteDirectoryPicker: View {
  let endpoint: String
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) var dismiss
  @State private var path = "~"
  @State private var directories: [String] = []
  @State private var busy = false
  @State private var error = ""
  @State private var generation = UUID()
  func load() {
    busy = true
    let token = UUID()
    generation = token
    let requested = path
    DispatchQueue.global().async {
      let reply = rpc(["Directory": ["path": requested]], endpoint: endpoint)
      DispatchQueue.main.async {
        guard generation == token else { return }
        busy = false
        if let message = reply["error"] as? String {
          error = message
          return
        }
        if let value = reply["Directory"] as? [String: Any] {
          path = value["path"] as? String ?? requested
          directories = value["directories"] as? [String] ?? []
          error = ""
        }
      }
    }
  }
  var body: some View {
    VStack(spacing: 12) {
      Text("Open Remote Workspace").font(.headline)
      HStack {
        TextField("Path", text: $path).onSubmit(load)
        Button("Go", action: load)
      }
      if busy { ProgressView() }
      if !error.isEmpty { ErrorNotice(message: error) }
      List {
        Button("..") {
          path = (path as NSString).deletingLastPathComponent
          load()
        }
        ForEach(directories, id: \.self) { name in
          Button(name) {
            path = (path as NSString).appendingPathComponent(name)
            load()
          }
        }
      }
      HStack {
        Button("Cancel") {
          generation = UUID()
          dismiss()
        }.keyboardShortcut(.cancelAction)
        Spacer()
        Button("Open") {
          store.request(["CreateWorkspace": ["path": path, "name": ""]])
          dismiss()
        }.disabled(busy || !error.isEmpty).keyboardShortcut(.defaultAction)
      }
    }.padding(20).frame(width: 480, height: 420).onAppear(perform: load)
  }
}

/// OpenSSH starts the same executable as a short-lived, native prompt helper.
func handleSSHAskpass() {
  guard ProcessInfo.processInfo.environment["SSH_ASKPASS_REQUIRE"] == "force",
    CommandLine.arguments.count == 2
  else { return }
  let prompt = CommandLine.arguments[1]
  NSApplication.shared.setActivationPolicy(.accessory)
  NSApplication.shared.activate(ignoringOtherApps: true)
  let alert = NSAlert()
  alert.messageText = "SSH Authentication"
  alert.informativeText = prompt
  let confirmation = prompt.contains("yes/no")
  let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 340, height: 24))
  if !confirmation { alert.accessoryView = field }
  alert.addButton(withTitle: confirmation ? "Trust Host" : "Continue")
  alert.addButton(withTitle: "Cancel")
  alert.window.initialFirstResponder = confirmation ? nil : field
  guard alert.runModal() == .alertFirstButtonReturn else { exit(1) }
  print(confirmation ? "yes" : field.stringValue)
  exit(0)
}
