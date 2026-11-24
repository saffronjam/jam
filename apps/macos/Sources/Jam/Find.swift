import AppKit
import CJam
import SwiftUI

/// UI state stays on the main thread. Native search only queues work and returns small summaries.
final class WorkspaceFinder: ObservableObject {
  weak var store: Store?
  @Published var query = ""
  @Published var count = 0
  @Published var ordinal = 0
  @Published var spinning = false
  @Published var message = ""
  private var generation: UInt64 = 0
  private var timer: Timer?
  private var pending: DispatchWorkItem?
  private var started = Date()
  private var scope = ""
  private var views: [TerminalNSView] = []
  private var counts: [Int] = []
  private var chosen: (String, Int)?
  private var observedRevisions: [Int] = []
  private var settled = false

  init(store: Store) { self.store = store }
  func toggle() { if store?.showFind == true { close() } else { open() } }
  func open() {
    guard let store else { return }
    withAnimation(Self.animation) { store.showFind = true }
    timer?.invalidate()
    timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
      self?.poll()
    }
    update()
  }
  func close() {
    pending?.cancel()
    pending = nil
    timer?.invalidate()
    timer = nil
    for view in views { _ = view.find(["action": "cancel"]) }
    views = []
    spinning = false
    if let store {
      withAnimation(Self.animation) { store.showFind = false }
      store.focusTerminal()
    }
  }
  static var animation: Animation? {
    NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
      ? nil : .timingCurve(0.22, 1, 0.36, 1, duration: 0.2)
  }
  private func visibleTerminals() -> [TerminalNSView] {
    guard let store, let root = NSApp.keyWindow?.contentView else { return [] }
    let ids = store.visiblePaneIDs
    var found: [String: TerminalNSView] = [:]
    func visit(_ view: NSView) {
      if let terminal = view as? TerminalNSView, ids.contains(terminal.pane),
        terminal.handle != nil,
        !terminal.isHiddenOrHasHiddenAncestor
      {
        found[terminal.pane] = terminal
      }
      for child in view.subviews { visit(child) }
    }
    visit(root)
    return ids.compactMap { found[$0] }
  }
  func update() {
    pending?.cancel()
    for view in views { _ = view.find(["action": "cancel"]) }
    count = 0
    ordinal = 0
    chosen = nil
    spinning = false
    message = ""
    guard store?.showFind == true, !query.isEmpty else {
      views = []
      return
    }
    let item = DispatchWorkItem { [weak self] in self?.start() }
    pending = item
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.04, execute: item)
  }
  private func start() {
    pending = nil
    guard let store, store.showFind, !query.isEmpty else { return }
    generation += 1
    started = Date()
    settled = false
    spinning = false
    views = visibleTerminals()
    scope =
      "\(store.selected)/\(store.surface == .coordinator ? "coordinator" : store.currentTab?.id ?? "")"
    counts = views.map { _ in 0 }
    observedRevisions = []
    for view in views { _ = view.find(["action": "start", "id": generation, "query": query]) }
  }
  private func poll() {
    guard let store, store.showFind, !query.isEmpty, pending == nil else { return }
    let live = visibleTerminals()
    if scope
      != "\(store.selected)/\(store.surface == .coordinator ? "coordinator" : store.currentTab?.id ?? "")"
      || live.map(ObjectIdentifier.init) != views.map(ObjectIdentifier.init)
    {
      update()
      return
    }
    guard !views.isEmpty else { return }
    let results = views.map { $0.find(["action": "poll"]) }
    let busy = results.contains {
      ($0["id"] as? UInt64) != generation || $0["busy"] as? Bool == true
    }
    spinning = busy && Date().timeIntervalSince(started) > 0.12
    // Keep the last settled result during a refresh of the same query and tab.
    // Query and tab changes still clear it in update().
    guard !busy else { return }
    if results.contains(where: { $0["stale"] as? Bool == true }) {
      start()
      return
    }
    counts = results.map { $0["count"] as? Int ?? 0 }
    count = counts.reduce(0, +)
    message = results.compactMap { $0["error"] as? String }.first ?? ""
    if !settled {
      settled = true
      observedRevisions = results.map { $0["snapshot_revision"] as? Int ?? 0 }
      if count > 0 {
        choose(min(count - 1, max(0, ordinal - 1)))
      } else {
        chosen = nil
        ordinal = 0
      }
    }
    let revisions = results.map { $0["revision"] as? Int ?? 0 }
    if (revisions != observedRevisions || !message.isEmpty)
      && Date().timeIntervalSince(started) > 0.75
    {
      // Coalesce live output; never restart a running scan merely because more output arrived.
      start()
    }
  }
  func next(_ direction: Int) {
    guard settled, count > 0 else { return }
    choose((max(0, ordinal - 1) + direction + count) % count)
  }
  private func choose(_ index: Int) {
    var remaining = index
    for (i, n) in counts.enumerated() {
      if remaining < n {
        chosen = (views[i].pane, remaining)
        ordinal = index + 1
        store?.focus = views[i].pane
        _ = views[i].find(["action": "select", "id": generation, "index": remaining])
        return
      }
      remaining -= n
    }
  }
}

struct WorkspaceFindBar: View {
  @ObservedObject var finder: WorkspaceFinder
  @FocusState private var focused: Bool
  var body: some View {
    HStack(spacing: 9) {
      Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
      TextField("Find in all panes…", text: $finder.query)
        .textFieldStyle(.plain).focused($focused)
        .accessibilityLabel("Find in tab history")
        .onChange(of: finder.query) { _, _ in finder.update() }
        .onSubmit { finder.next(1) }
      if finder.spinning { ProgressView().controlSize(.small).frame(width: 16, height: 16) }
      Text(finder.query.isEmpty ? "" : "\(finder.ordinal)/\(finder.count)")
        .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
      Button {
        finder.next(-1)
      } label: {
        Image(systemName: "chevron.up")
      }
      .help("Previous match (Shift-Return)").disabled(finder.count == 0)
      Button {
        finder.next(1)
      } label: {
        Image(systemName: "chevron.down")
      }
      .help("Next match (Return)").disabled(finder.count == 0)
      Button {
        finder.close()
      } label: {
        Image(systemName: "xmark")
      }.help("Close find (Escape)")
    }
    .buttonStyle(.plain).padding(12).frame(width: 390)
    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
    .overlay(RoundedRectangle(cornerRadius: 10).stroke(.primary.opacity(0.12)))
    .shadow(color: .black.opacity(0.2), radius: 12, y: 5)
    .help(
      finder.message.isEmpty ? "Searches the history of every pane in this tab" : finder.message
    )
    .onAppear { DispatchQueue.main.async { focused = true } }
    .onExitCommand { finder.close() }
  }
}

extension TerminalNSView {
  func find(_ request: [String: Any]) -> [String: Any] {
    guard let handle, let data = try? JSONSerialization.data(withJSONObject: request),
      let json = String(data: data, encoding: .utf8)
    else { return [:] }
    let result = nativeString(jam_view_find(handle, json))
    return (try? JSONSerialization.jsonObject(with: Data(result.utf8)) as? [String: Any]) ?? [:]
  }
}
