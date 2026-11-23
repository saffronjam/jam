import SwiftUI

struct MachineList: View {
  @EnvironmentObject var store: Store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @State private var hoveredMachine: String?
  private var motion: Animation? {
    reduceMotion ? nil : .timingCurve(0.22, 0.8, 0.25, 1, duration: 0.24)
  }

  var body: some View {
    VStack(spacing: 4) {
      HStack {
        Text("MACHINES").font(.caption).foregroundStyle(.secondary)
        Spacer()
        Button {
          store.editingRemote = nil
          store.showRemote = true
        } label: {
          Image(systemName: "plus")
            .frame(width: 28, height: 28).contentShape(Rectangle())
        }.buttonStyle(.plain).help("Add Remote")
      }.padding(.leading, 12).padding(.trailing, 4).padding(.top, 2)
      ScrollView {
        VStack(spacing: 2) {
          row(id: "", name: "Local", connected: store.connected)
            .contextMenu {
              serverMenu("")
              Divider()
              Button("Reconnect") { store.reconnectMachine("") }.disabled(
                !store.machineActions("").canReconnect)
            }
          ForEach(store.remotes) { remote in
            row(
              id: remote.id, name: remote.name,
              connected: store.remoteMessages[remote.id] == "Connected"
            )
            .contextMenu {
              serverMenu(remote.id)
              Divider()
              Button("Reconnect") { store.reconnectMachine(remote.id) }.disabled(
                !store.machineActions(remote.id).canReconnect)
              Button("Disconnect") { store.disconnectRemote(remote.id) }.disabled(
                !store.machineActions(remote.id).canDisconnect)
              if store.remotePersistence[remote.id] != true {
                Button("Enable Persistence") { store.enableRemotePersistence(remote.id) }
                  .disabled(!store.machineActions(remote.id).canRestart)
              }
              Button("Edit Remote…") {
                store.editingRemote = remote
                store.showRemote = true
              }.disabled(!store.machineActions(remote.id).canRestart)
              Button("Remove Remote") { store.removeRemote(remote.id) }.disabled(
                store.serverBusy.contains(remote.id))
            }
            .transition(.opacity.combined(with: .move(edge: .bottom)))
          }
        }.padding(.horizontal, 6)
      }.scrollIndicators(.hidden)
        .frame(height: min(160, CGFloat(store.remotes.count + 1) * 30))
    }.padding(.bottom, 8)
      .animation(motion, value: store.remotes.map(\.id))
  }

  @ViewBuilder private func serverMenu(_ endpoint: String) -> some View {
    let state = store.machineActions(endpoint)
    if state.status?.blocked != true {
      if state.incompatibleServer {
        Text("Server update required")
      } else if !state.connected {
        Text(state.connecting ? "Connecting…" : "Offline")
      } else if state.checking {
        Text("Checking server…")
      } else if let status = state.status, status.running, status.compatible,
        status.availableBuild != nil
      {
        Text(state.needsUpdate ? "Update available" : "Server is up to date")
      }
    }
    if state.status?.state == "client_update_required" {
      Button("Update Jam…") { store.appUpgrade.check() }.disabled(state.busy)
    }
    if state.status?.blocked == true && state.status?.state != "client_update_required" {
      Button("Take Over Server…") { store.reconnectMachine(endpoint) }
        .disabled(!state.canTakeover)
    }
    if state.status?.state == "inspection_failed" || (endpoint.isEmpty && store.localTimeout != nil)
    {
      Button("Server Details…") { store.showServerAction(endpoint, kind: .inspect) }
        .disabled(state.busy)
    }
    Button("Update Server…") { store.showServerAction(endpoint, kind: .update) }.disabled(
      !state.canUpdate)
    if store.scheduledServerUpdates.contains(endpoint) {
      Button("Cancel Scheduled Update") { store.scheduledServerUpdates.remove(endpoint) }
        .disabled(!state.canRestart)
    }
    Button("Restart Server…") { store.showServerAction(endpoint, kind: .restart) }.disabled(
      !state.canRestart)
  }

  private func row(id: String, name: String, connected: Bool) -> some View {
    let actions = store.machineActions(id)
    let presentation = store.machinePresentation(id)
    let warning = presentation.warning
    return HStack(spacing: 8) {
      Circle().fill(
        actions.needsUpdate || warning
          ? Color.yellow : connected ? Color.green : Color.secondary.opacity(0.5)
      ).frame(
        width: 6, height: 6)
      Image(systemName: "desktopcomputer").foregroundStyle(.secondary)
      Text(name).lineLimit(1)
      Spacer(minLength: 0)
      HStack(spacing: 4) {
        if let label = presentation.label {
          Button(label) { store.reconnectMachine(id) }.buttonStyle(.plain)
            .font(.system(size: 9)).foregroundStyle(.secondary).lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        if store.serverBusy.contains(id)
          || presentation.isAttempting
          || (!warning && (store.scheduledServerUpdates.contains(id) || actions.needsUpdate))
        {
          ZStack {
            if store.serverBusy.contains(id) {
              ProgressView().controlSize(.mini)
            } else if presentation.isAttempting {
              ProgressView().controlSize(.mini).help(
                presentation.label ?? "Connecting")
            } else if store.scheduledServerUpdates.contains(id) {
              Image(systemName: "clock").foregroundStyle(.secondary).help(
                "Update scheduled for when all sessions end")
            } else {
              Image(systemName: "arrow.down.circle").foregroundStyle(.yellow).help(
                "Server update available")
            }
          }.frame(width: 24, height: 24)
        }
        if warning && !store.serverBusy.contains(id) {
          Button {
            store.reconnectMachine(id)
          } label: {
            Image(systemName: "exclamationmark.triangle")
              .foregroundStyle(.yellow)
              .frame(width: 24, height: 24).contentShape(Rectangle())
          }.buttonStyle(.plain)
            .accessibilityLabel(store.serverStatuses[id]?.label ?? "Server needs attention")
        }
        if !warning && !id.isEmpty && connected && store.remotePersistence[id] == false {
          MachinePersistenceWarning(endpoint: id, name: name)
            .frame(width: 24, height: 24)
        }
      }.fixedSize(horizontal: true, vertical: false)
    }.font(.caption).padding(.horizontal, 8).frame(height: 28)
      .background(
        RoundedRectangle(cornerRadius: 6)
          .fill(Color.primary.opacity(hoveredMachine == id ? 0.055 : 0))
      )
      .contentShape(Rectangle())
      .onHover { inside in
        if inside { hoveredMachine = id } else if hoveredMachine == id { hoveredMachine = nil }
      }
      .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hoveredMachine == id)
      .accessibilityElement(children: .contain)
      .accessibilityLabel(name)
      .accessibilityIdentifier("machine." + (id.isEmpty ? "local" : id))

  }
}

/// Keep the explanation beside the warning, without selecting the machine row.
struct MachinePersistenceWarning: View {
  let endpoint: String
  let name: String
  @EnvironmentObject var store: Store
  @State private var presented = false

  var body: some View {
    Button {
      presented.toggle()
    } label: {
      Image(systemName: "exclamationmark.triangle")
        .foregroundStyle(.secondary)
        .frame(width: 24, height: 24).contentShape(Rectangle())
    }.buttonStyle(.plain)
      .accessibilityLabel("Session persistence warning for \(name)")
      .help("Session persistence is not enabled. Click for details.")
      .popover(isPresented: $presented, arrowEdge: .trailing) {
        VStack(alignment: .leading, spacing: 12) {
          Text("Session persistence is not enabled").font(.headline)
          Text(
            "\(name) is connected, but its terminals may stop when you log out of the remote machine."
          )
          HStack {
            Spacer()
            if store.enablingPersistence.contains(endpoint) {
              ProgressView().controlSize(.small)
            }
            Button("Enable Persistence") { store.enableRemotePersistence(endpoint) }
              .disabled(store.enablingPersistence.contains(endpoint))
          }
        }.font(.callout).padding(16).frame(width: 310)
      }
  }
}
