import AppKit

/// Move the same native terminal surface between layout hosts. Reparenting must
/// not rebuild its renderer, detach its mirror, or discard the last frame.
final class TerminalViewPool {
  private var views: [String: TerminalNSView] = [:]
  private var policy = TerminalCachePolicy()
  func view(for pane: String) -> TerminalNSView {
    policy.touch(pane)
    if let view = views[pane] { return view }
    let view = TerminalNSView(pane: pane)
    views[pane] = view
    return view
  }
  func existing(_ pane: String) -> TerminalNSView? { views[pane] }
  func retain(live: Set<String>, visible: Set<String>) {
    let kept = policy.retain(live: live, visible: visible, capacity: 16)
    for id in Array(views.keys) where !kept.contains(id) {
      views.removeValue(forKey: id)?.removeFromSuperview()
    }
  }
}
final class TerminalSurfaceHost: NSView {
  let terminal: TerminalNSView
  init(terminal: TerminalNSView) {
    self.terminal = terminal
    super.init(frame: .zero)
    terminal.layoutHost = self
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func layout() {
    super.layout()
    guard terminal.layoutHost === self, bounds.width > 0, bounds.height > 0 else { return }
    // SwiftUI creates hosts at zero size. Keep the cached surface intact until
    // its destination has real geometry, then attach it at that size in one pass.
    terminal.frame = bounds
    if terminal.superview !== self {
      terminal.invalidateFrame()
      addSubview(terminal)
    }
    terminal.prepareForDisplay()
  }
}
