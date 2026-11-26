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
  func retain(live: Set<String>, visible: Set<String>, capacity: Int) {
    let kept = policy.retain(live: live, visible: visible, capacity: capacity)
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
    wantsLayer = true
    layer?.masksToBounds = true
    terminal.layoutHost = self
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func layout() {
    super.layout()
    guard terminal.layoutHost === self, bounds.width > 0, bounds.height > 0 else { return }
    // Animate the clip around the terminal while the drawable and PTY use the
    // destination size. Resizing a Metal surface every frame causes stalls.
    let size = terminal.desiredSize ?? bounds.size
    let frame = NSRect(
      x: bounds.minX, y: bounds.maxY - size.height,
      width: size.width, height: size.height)
    if terminal.frame != frame { terminal.frame = frame }
    if terminal.superview !== self {
      terminal.invalidateFrame()
      addSubview(terminal)
    }
    terminal.prepareForDisplay()
  }
}
