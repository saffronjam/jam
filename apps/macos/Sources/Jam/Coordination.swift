import SwiftUI

extension Store {
  func addCoordinator(_ workspace: String, provider: String) {
    guard config?.enabledAgents.contains(provider) != false,
      workspaces.first(where: { $0.id == workspace })?.hasCoordinator == false,
      startingCoordinators.insert(workspace).inserted
    else { return }
    let endpoint = WorkspaceSelection.endpoint(of: workspace)
    let settings = config?.hostSettings
    DispatchQueue.global(qos: .userInitiated).async {
      _ = configureHost(settings, endpoint: endpoint)
      let result = rpc(
        ["Coordination": ["action": "start", "workspace": workspace, "provider": provider]],
        endpoint: endpoint)
      DispatchQueue.main.async {
        self.startingCoordinators.remove(workspace)
        if let content = result["Coordination"] as? [String: Any],
          let state = content["State"] as? [String: Any]
        {
          self.apply(state, endpoint: endpoint)
          self.openCoordinator(workspace)
        } else {
          self.error = result["error"] as? String ?? "Could not create coordinator"
        }
      }
    }
  }
}

struct CoordinationView: View {
  @EnvironmentObject var store: Store
  @State private var agents: [[String: Any]] = []
  @State private var messages: [[String: Any]] = []
  @State private var tasks: [[String: Any]] = []
  @State private var instructions = ""
  @State private var failure = ""
  @State private var loading = false
  @State private var showArchived = false
  private func refresh() {
    guard !loading else { return }
    loading = true
    let endpoint = store.currentEndpoint
    let scope = store.current?.parent ?? store.current?.id
    let history = showArchived
    DispatchQueue.global().async {
      let result = rpc(
        ["Coordination": ["action": "list", "include_archived": history]], endpoint: endpoint)
      DispatchQueue.main.async {
        loading = false
        if history != showArchived {
          refresh()
          return
        }
        if let data = result["Coordination"] as? [String: Any] {
          failure = ""
          agents = (data["agents"] as? [[String: Any]] ?? []).filter {
            scope == nil || $0["root"] as? String == scope
          }
          tasks = (data["tasks"] as? [[String: Any]] ?? []).filter {
            scope == nil || $0["root"] as? String == scope
          }
          let ids = Set(agents.compactMap { $0["id"] as? String })
          messages = (data["messages"] as? [[String: Any]] ?? []).filter {
            ids.contains($0["to"] as? String ?? "")
              && (history
                || ($0["state"] as? String != "acknowledged"
                  && $0["state"] as? String != "cancelled"))
          }

        } else {
          failure = result["error"] as? String ?? "Coordination unavailable"
        }
      }
    }
  }
  private func action(_ command: String, agent: String) {
    let endpoint = store.currentEndpoint
    DispatchQueue.global().async {
      let result = rpc(["Coordination": ["action": command, "agent": agent]], endpoint: endpoint)
      DispatchQueue.main.async {
        if command == "instructions", let data = result["Coordination"] {
          if let content = data as? [String: Any] {
            let files = content["files"] as? [[String: Any]] ?? []
            let guidance: String = content["guidance"] as? String ?? ""
            let fileSections: [String] = files.map { file -> String in
              let path: String = file["path"] as? String ?? ""
              let text: String = file["text"] as? String ?? ""
              return "\n" + path + "\n" + text
            }
            instructions = ([guidance] + fileSections).joined(separator: "\n")
          }
        }
        if let error = result["error"] as? String { failure = error }
        refresh()
      }
    }
  }
  private func taskState(_ task: [String: Any]) -> String {
    if task["archived"] as? Bool == true { return task["state"] as? String ?? "Archived" }
    switch task["delivery_status"] as? String {
    case "waiting_for_agent": return "Waiting for agent readiness"
    case "queued": return "Queued for delivery"
    case "sending": return "Sending assignment…"
    case "transport_accepted": return "Sent · awaiting acknowledgement"
    case "acknowledged": return "Acknowledged by agent"
    case "delivery_failed": return "Delivery failed"
    default: return task["state"] as? String ?? ""
    }
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        VStack(alignment: .leading, spacing: 4) {
          Text(store.current?.name ?? "No workspace selected").font(.headline)
        }
        Spacer()
        Toggle("Include archived", isOn: $showArchived).toggleStyle(.checkbox)
        if loading { ProgressView().controlSize(.small) }
        Button("Refresh", action: refresh)
      }
      if !failure.isEmpty { ErrorNotice(message: failure) }
      List {
        Section("Agents") {
          ForEach(agents.indices, id: \.self) { i in
            let agent = agents[i]
            HStack {
              VStack(alignment: .leading) {
                Text("\(agent["provider"] as? String ?? "") · \(agent["role"] as? String ?? "")")
                if let model = agent["model"] as? String {
                  Text(model).font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                Text(agent["path"] as? String ?? "").font(.caption).foregroundStyle(.secondary)
              }
              Text(
                (agent["recovery"] as? [String: Any])?["state"] as? String ?? agent["activity"]
                  as? String ?? "Unknown"
              )
              .font(.caption).foregroundStyle(.secondary)
              Spacer()
              Button("Instructions") { action("instructions", agent: agent["id"] as? String ?? "") }
              if agent["archived"] as? Bool == true {
                Text("Archived").font(.caption).foregroundStyle(.secondary)
              } else if agent["closed"] as? Bool == true {
                Text("Closed").font(.caption).foregroundStyle(.secondary)
              } else {
                Button(agent["paused"] as? Bool == true ? "Resume" : "Pause") {
                  action(
                    agent["paused"] as? Bool == true ? "resume" : "pause",
                    agent: agent["id"] as? String ?? "")
                }
              }
            }
          }
        }
        if !messages.isEmpty {
          Section(showArchived ? "Messages" : "Pending messages") {
            ForEach(messages.indices, id: \.self) { i in
              VStack(alignment: .leading) {
                Text(messages[i]["text"] as? String ?? "")
                Text(messages[i]["error"] as? String ?? messages[i]["state"] as? String ?? "Queued")
                  .font(.caption).foregroundStyle(.secondary)
              }
            }
          }
        }
        Section("Tasks") {
          ForEach(tasks.indices, id: \.self) { i in
            VStack(alignment: .leading) {
              Text(tasks[i]["objective"] as? String ?? "")
              Text(
                (tasks[i]["archived"] as? Bool == true ? "Archived · " : "") + taskState(tasks[i])
              ).font(.caption).foregroundStyle(.secondary)
              if let error = tasks[i]["delivery_error"] as? String { ErrorNotice(message: error) }
              if let result = tasks[i]["result"] as? String {
                Text(result).font(.caption).textSelection(.enabled)
              }
            }
          }
        }
      }
      if !instructions.isEmpty {
        ScrollView {
          Text(instructions).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
        }.frame(height: 160)
      }
    }.padding(.horizontal, 22).padding(.bottom, 12)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .onAppear(perform: refresh)
      .onChange(of: showArchived) { _, _ in refresh() }
      .onChange(of: store.selected) { _, _ in refresh() }
  }
}

struct CoordinatorRecovery: Identifiable {
  let id: String
  let pane: String
  let endpoint: String
  let provider: String
}
extension Store {
  func openCoordinator(_ workspace: String) {
    guard let w = workspaces.first(where: { $0.id == workspace }) else { return }
    if let coordinator = w.coordinator, panes[coordinator.pane] != nil {
      select(workspace, surface: .coordinator)
      return
    }
    let endpoint = WorkspaceSelection.endpoint(of: workspace)
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = rpc(
        ["Coordination": ["action": "open", "workspace": workspace]], endpoint: endpoint)
      DispatchQueue.main.async {
        if let data = reply["Coordination"] as? [String: Any],
          let state = data["State"] as? [String: Any]
        {
          self.apply(state, endpoint: endpoint)
          self.openCoordinator(workspace)
        } else {
          self.error = reply["error"] as? String ?? "Could not open coordinator"
        }
      }
    }
  }
  func recoverCoordinator(_ pane: Pane) {
    guard let w = workspaces.first(where: { $0.coordinator?.pane == pane.id }),
      let id = w.coordinator?.id
    else { return }
    coordinatorRecovery = CoordinatorRecovery(
      id: id, pane: pane.id,
      endpoint: WorkspaceSelection.endpoint(of: w.id), provider: pane.agentProvider ?? "codex")
  }
}
struct AgentStoppedView: View {
  @EnvironmentObject var store: Store
  let pane: Pane
  var body: some View {
    VStack(spacing: 16) {
      Image(systemName: "pause.circle").font(.system(size: 32)).foregroundStyle(.secondary)
      Text(pane.agentRole == "coordinator" ? "Coordinator stopped" : "Agent stopped").font(
        .title2.bold())
      if let error = pane.launchError {
        ErrorNotice(message: error)
      } else {
        Text("Resume the saved conversation to continue.").foregroundStyle(.secondary)
      }
      HStack {
        if pane.agentRole == "coordinator" {
          Button("Recover Conversation…") { store.recoverCoordinator(pane) }
        }
        Button("Resume") { store.restartPane(pane.id) }
      }
    }.padding(24).frame(maxWidth: 520).frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}
struct CoordinatorRecoveryView: View {
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) private var dismiss
  let recovery: CoordinatorRecovery
  @State private var sessions: [[String: Any]] = []
  @State private var selected: String?
  @State private var loading = false
  @State private var busy = false
  @State private var failure = ""
  private func load() {
    guard recovery.provider == "codex" else { return }
    loading = true
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = rpc(
        ["Coordination": ["action": "sessions", "agent": recovery.id]], endpoint: recovery.endpoint)
      DispatchQueue.main.async {
        loading = false
        if let value = reply["Coordination"] as? [[String: Any]] {
          sessions = value
        } else {
          failure = reply["error"] as? String ?? "Could not load saved conversations"
        }
      }
    }
  }
  private func recover(session: String?) {
    busy = true
    var request: [String: Any] = ["action": "recover", "agent": recovery.id]
    request["session"] = session.map { $0 as Any } ?? NSNull()
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = rpc(["Coordination": request], endpoint: recovery.endpoint)
      DispatchQueue.main.async {
        busy = false
        if let error = reply["error"] as? String { failure = error } else { dismiss() }
      }
    }
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("Recover Coordinator").font(.headline)
      Text(
        "Choose a saved conversation from this folder, or start a new one. Existing conversation history is kept."
      )
      .foregroundStyle(.secondary)
      if loading { ProgressView("Loading conversations…") }
      if recovery.provider == "codex" {
        List(selection: $selected) {
          ForEach(sessions, id: \.sessionID) { session in
            VStack(alignment: .leading, spacing: 4) {
              Text(session["name"] as? String ?? session["preview"] as? String ?? "Conversation")
                .lineLimit(2)
              Text(session.sessionID).font(.system(.caption, design: .monospaced)).foregroundStyle(
                .secondary)
            }.tag(session.sessionID)
          }
        }.frame(minHeight: 180)
      }
      if !failure.isEmpty { ErrorNotice(message: failure) }
      HStack {
        Button("Start New Conversation") { recover(session: nil) }.disabled(busy)
        Spacer()
        if busy { ProgressView().controlSize(.small) }
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction).disabled(busy)
        if recovery.provider == "codex" {
          Button("Resume Selected") { recover(session: selected) }
            .disabled(selected == nil || busy || loading).keyboardShortcut(.defaultAction)
        }
      }
    }.padding(22).frame(width: 590).onAppear(perform: load).interactiveDismissDisabled(busy)
  }
}
extension Dictionary where Key == String, Value == Any {
  fileprivate var sessionID: String { self["session"] as? String ?? "" }
}
