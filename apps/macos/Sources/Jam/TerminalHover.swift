import AppKit

/// The highlight lives over the terminal's Metal surface, independently of the selection.
final class TerminalHover {
  weak var view: TerminalNSView?
  private let underline = CAShapeLayer()
  private var monitor: Any?
  private var target: TerminalTarget?
  private var work: DispatchWorkItem?
  private var cache: [String: (Date, Bool)] = [:]
  private var generation = 0
  private var drawn: TerminalTarget?
  private var lastBounds: NSRect?
  private var pointer: NSPoint?
  private var holdingControl = false
  var cwd = "" {
    didSet {
      if oldValue != cwd {
        cache.removeAll()
        update(nil)
        refresh()
      }
    }
  }
  var supported: Bool? {
    didSet {
      if oldValue != supported {
        cache.removeAll()
        update(nil)
        refresh()
      }
    }
  }

  init(_ view: TerminalNSView) {
    self.view = view
    underline.zPosition = 100
    underline.fillColor = nil
    underline.actions = ["path": NSNull(), "strokeColor": NSNull()]
  }
  deinit {
    if let monitor { NSEvent.removeMonitor(monitor) }
    work?.cancel()
  }

  func track(_ event: NSEvent) {
    guard let view else { return }
    pointer = view.convert(event.locationInWindow, from: nil)
    holdingControl = event.modifierFlags.contains(.control)
    if monitor == nil {
      monitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] e in
        self?.holdingControl = e.modifierFlags.contains(.control)
        self?.refresh()
        return e
      }
    }
    refresh()
  }

  func leave() {
    if let monitor {
      NSEvent.removeMonitor(monitor)
      self.monitor = nil
    }
    pointer = nil
    holdingControl = false
    update(nil)
  }

  func refresh() {
    guard holdingControl, let view, let point = pointer, view.bounds.contains(point),
      view.window?.isKeyWindow == true
    else {
      update(nil)
      return
    }
    update(hit(point))
  }

  func hit(_ point: NSPoint) -> TerminalTarget? {
    guard let view, view.bounds.contains(point) else { return nil }
    let response = view.action(
      "link", Int32(point.x / view.cellSize.width),
      Int32((view.bounds.height - point.y) / view.cellSize.height))
    guard
      let context = try? JSONSerialization.jsonObject(with: Data(response.utf8)) as? [String: Any]
    else { return nil }
    return TerminalTarget.hit(context)
  }

  func confirmed(_ path: String) -> Bool? {
    guard let entry = cache[path], entry.0 > Date() else { return nil }
    return entry.1
  }

  private func update(_ next: TerminalTarget?) {
    guard target != next || lastBounds != view?.bounds else { return }
    target = next
    lastBounds = view?.bounds
    generation += 1
    work?.cancel()
    guard let next, let view else {
      draw(nil)
      return
    }
    switch next.destination {
    case .web: draw(next)
    case .file(let location):
      if supported != true {
        draw(location.explicit || next.fallbackURL != nil ? next : nil)
        return
      }
      if let entry = cache[location.path], entry.0 > Date() {
        draw(entry.1 || next.fallbackURL != nil ? next : nil)
        return
      }
      draw(nil)
      let token = generation
      let reader = PreviewReader(pane: view.pane)
      let item = DispatchWorkItem { [weak self] in
        let exists = (try? reader.inspect(location.path)) != nil
        DispatchQueue.main.async { [weak self] in
          guard let self, self.generation == token else { return }
          if self.cache.count >= 64 { self.cache.removeAll() }
          self.cache[location.path] = (Date().addingTimeInterval(2), exists)
          self.draw(exists || next.fallbackURL != nil ? next : nil)
        }
      }
      work = item
      DispatchQueue.global(qos: .userInitiated).asyncAfter(deadline: .now() + 0.075, execute: item)
    }
  }

  private func draw(_ target: TerminalTarget?) {
    guard let view else { return }
    let hadHighlight = drawn != nil
    drawn = target
    guard let target else {
      underline.removeFromSuperlayer()
      if hadHighlight { NSCursor.iBeam.set() }
      return
    }
    let path = CGMutablePath()
    var i = target.cells.lowerBound
    while i < target.cells.upperBound {
      let row = i / target.columns
      let end = min((row + 1) * target.columns, target.cells.upperBound)
      let y = view.bounds.height - CGFloat(row + 1) * view.cellSize.height + 2
      path.move(to: CGPoint(x: CGFloat(i % target.columns) * view.cellSize.width, y: y))
      path.addLine(to: CGPoint(x: CGFloat(end - row * target.columns) * view.cellSize.width, y: y))
      i = end
    }
    underline.frame = view.bounds
    underline.strokeColor = view.theme.accent.cgColor
    underline.lineWidth = 1
    underline.path = path
    if underline.superlayer == nil { view.layer?.addSublayer(underline) }
    NSCursor.pointingHand.set()
  }
}
