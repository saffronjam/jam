import SwiftUI

struct WorktreePickerContext: Identifiable {
  let workspace: String
  let name: String
  let path: String
  let endpoint: String
  var id: String { workspace }
}

struct WorktreePicker: View {
  let context: WorktreePickerContext
  @EnvironmentObject var store: Store
  @Environment(\.dismiss) private var dismiss
  @FocusState private var searching: Bool
  @State private var checkouts: [WorktreeCheckout] = []
  @State private var choices = WorktreeChoices(checkouts: [], query: "", opened: [])
  @State private var query = ""
  @State private var selected: String?
  @State private var loading = true
  @State private var error = ""
  @State private var generation = UUID()

  private var opened: Set<String> {
    store.openedCheckoutPaths(in: context.workspace)
  }
  private func updateChoices() {
    choices = WorktreeChoices(checkouts: checkouts, query: query, opened: opened)
    selected = choices.selection(selected)
  }
  private var available: Bool {
    context.endpoint.isEmpty || store.remoteMessages[context.endpoint] == "Connected"
  }
  private var canOpen: Bool {
    !loading && error.isEmpty && available && !store.busy
      && choices.available.contains { $0.id == selected }
  }

  private func load() {
    let token = UUID()
    generation = token
    loading = true
    error = ""
    DispatchQueue.global(qos: .userInitiated).async {
      let reply = rpc(["Worktrees": ["workspace": context.workspace]], endpoint: context.endpoint)
      let decoded: [WorktreeCheckout]?
      if let repository = reply["Worktrees"] as? [String: Any],
        let raw = repository["checkouts"],
        let data = try? JSONSerialization.data(withJSONObject: raw)
      {
        decoded = try? JSONDecoder().decode([WorktreeCheckout].self, from: data)
      } else {
        decoded = nil
      }
      DispatchQueue.main.async {
        guard generation == token else { return }
        loading = false
        if let decoded {
          checkouts = decoded
          updateChoices()
        } else {
          error = reply["error"] as? String ?? "Could not load worktrees"
        }
      }
    }
  }
  private func openSelected() {
    guard canOpen, let checkout = choices.available.first(where: { $0.id == selected }) else {
      return
    }
    store.openCheckout(checkout.path, endpoint: context.endpoint, workspace: context.workspace)
    dismiss()
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack {
        VStack(alignment: .leading, spacing: 4) {
          Text("Open Worktree").font(.headline)
          Text(context.path).font(.caption).foregroundStyle(.secondary)
            .lineLimit(1).truncationMode(.middle).help(context.path)
        }
        Spacer()
        if loading {
          ProgressView().controlSize(.small)
        } else {
          Text("\(checkouts.count) checkouts").font(.caption).foregroundStyle(.secondary)
        }
      }
      TextField("Filter by branch or path", text: $query)
        .textFieldStyle(.roundedBorder).focused($searching).onSubmit(openSelected)
      WorktreeList(
        checkouts: choices.filtered, opened: opened, selection: $selected,
        enabled: !loading, onOpen: openSelected
      ).frame(height: 300)
        .overlay {
          if !loading && error.isEmpty && choices.filtered.isEmpty {
            Text(checkouts.isEmpty ? "No worktrees found" : "No matching worktrees")
              .foregroundStyle(.secondary)
          }
        }

      if !error.isEmpty {
        HStack {
          ErrorNotice(message: error)
          Spacer()
          Button("Retry", action: load).disabled(loading)
        }
      }
      Divider()
      HStack {
        if !available {
          Text("Remote is unavailable").font(.caption).foregroundStyle(.secondary)
        } else if !loading && !checkouts.isEmpty && checkouts.allSatisfy({ choices.isOpen($0) }) {
          Text("All worktrees are already open").font(.caption).foregroundStyle(.secondary)
        }
        Spacer()
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
        Button("Open", action: openSelected).keyboardShortcut(.defaultAction).disabled(!canOpen)
      }
    }.padding(20).frame(width: 600)
      .onAppear {
        searching = true
        load()
      }
      .onDisappear { generation = UUID() }
      .onChange(of: query) { _, _ in updateChoices() }
      .onChange(of: opened) { _, _ in updateChoices() }
      .onKeyPress(.downArrow) {
        selected = choices.move(selected, by: 1)
        return .handled
      }
      .onKeyPress(.upArrow) {
        selected = choices.move(selected, by: -1)
        return .handled
      }
  }
}
