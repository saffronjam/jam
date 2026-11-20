import AppKit

/// A context menu can run in an inactive app. Activate the owning window before
/// requesting its field editor, so visible selection also owns keyboard input.
func activateInlineEditor(_ editor: NSTextField) {
  guard let window = editor.window else { return }
  NSApp.activate()
  window.makeKeyAndOrderFront(nil)
  DispatchQueue.main.async { [weak editor, weak window] in
    guard let editor, let window, editor.window === window else { return }
    // selectText starts a field-editing session; calling it again commits the
    // existing session. Repeated activation must keep the current editor alive.
    guard editor.currentEditor() == nil else { return }
    window.makeKeyAndOrderFront(nil)
    editor.selectText(nil)
  }
}

final class InlineNameEditor: NSTextField, NSTextFieldDelegate {
  var completion: ((String?) -> Void)?
  private var hasJoinedWindow = false
  override init(frame: NSRect) {
    super.init(frame: frame)
    delegate = self
    font = .systemFont(ofSize: 12)
    isEditable = true
    isSelectable = true
    isBezeled = true
    drawsBackground = true
    placeholderString = "Automatic folder name"
    toolTip = "Leave empty to follow the current folder"
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    if window != nil {
      hasJoinedWindow = true
      activateInlineEditor(self)
    } else if hasJoinedWindow {
      finish(nil)
    }
  }
  func finish(_ value: String?) {
    guard let completion else { return }
    self.completion = nil
    completion(value)
  }
  func control(_ control: NSControl, textView: NSTextView, doCommandBy command: Selector) -> Bool {
    if command == #selector(NSResponder.cancelOperation(_:)) {
      finish(nil)
      return true
    }
    if command == #selector(NSResponder.insertNewline(_:)) {
      finish(stringValue)
      return true
    }
    return false
  }
  func controlTextDidEndEditing(_ notification: Notification) { finish(stringValue) }
}

struct PendingRename {
  let token = UUID()
  let name: String
}

extension Store {
  private func setDisplayName(_ name: String, for id: String) {
    for index in workspaces.indices {
      if workspaces[index].id == id { workspaces[index].name = name }
      if let tab = workspaces[index].tabs.firstIndex(where: { $0.id == id }) {
        workspaces[index].tabs[tab].name = name
      }
    }
  }
  func rename(_ id: String, to value: String) {
    let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else {
      request(["Rename": ["id": id, "name": name]])
      return
    }
    guard
      let oldName = workspaces.first(where: { $0.id == id })?.name
        ?? workspaces.flatMap(\.tabs).first(where: { $0.id == id })?.name
    else { return }
    let pending = PendingRename(name: name)
    pendingRenames[id] = pending
    setDisplayName(name, for: id)
    let endpoint = WorkspaceSelection.endpoint(of: id)
    endpointQueue(endpoint).async {
      // Renaming by stable identity does not depend on split/layout geometry.
      let reply = rpc(["Rename": ["id": id, "name": name]], endpoint: endpoint)
      let state =
        reply["State"] as? [String: Any]
        ?? rpc("State", endpoint: endpoint)["State"] as? [String: Any]
      DispatchQueue.main.async {
        if self.pendingRenames[id]?.token == pending.token {
          self.pendingRenames.removeValue(forKey: id)
          if reply["error"] != nil { self.setDisplayName(oldName, for: id) }
        }
        if let state { self.apply(state, endpoint: endpoint) }
        if let error = reply["error"] as? String { self.error = error }
      }
    }
  }
}
