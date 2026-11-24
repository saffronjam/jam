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
  func rename(_ id: String, to value: String) {
    let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else {
      request(["Rename": ["id": id, "name": name]])
      return
    }
    let pending = PendingRename(name: name)
    if submitLayout(
      ["Rename": ["id": id, "name": name]], endpoint: WorkspaceSelection.endpoint(of: id),
      preview: { layout in
        for index in layout.indices {
          if layout[index].id == id {
            layout[index].name = name
            return true
          }
          if let tab = layout[index].tabs.firstIndex(where: { $0.id == id }) {
            layout[index].tabs[tab].name = name
            return true
          }
        }
        return false
      },
      complete: { [weak self] _ in
        if self?.pendingRenames[id]?.token == pending.token {
          self?.pendingRenames.removeValue(forKey: id)
        }
      })
    {
      pendingRenames[id] = pending
    }
  }
}
