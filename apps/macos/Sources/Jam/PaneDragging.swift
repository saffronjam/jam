import AppKit
import SwiftUI

struct PaneDragHandle: NSViewRepresentable {
  let pane: String
  let store: Store
  func makeNSView(context: Context) -> PaneHeaderDragView { PaneHeaderDragView() }
  func updateNSView(_ view: PaneHeaderDragView, context: Context) {
    view.begin = { [weak view, weak store] event in
      guard let store, let window = view?.window else { return }
      store.paneDragController.begin(pane: pane, event: event, window: window)
    }
  }
}
final class PaneHeaderDragView: NSView {
  var begin: ((NSEvent) -> Void)?
  override func mouseDown(with event: NSEvent) { begin?(event) }
  override func resetCursorRects() { addCursorRect(bounds, cursor: .openHand) }
}
struct PaneDropSurface: NSViewRepresentable {
  let pane: String
  func makeNSView(context: Context) -> PaneDropView { PaneDropView() }
  func updateNSView(_ view: PaneDropView, context: Context) { view.pane = pane }
}
final class PaneDropView: NSView {
  var pane = ""
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
struct EmptyPaneDropSurface: NSViewRepresentable {
  let tab: String
  func makeNSView(context: Context) -> EmptyPaneDropView { EmptyPaneDropView() }
  func updateNSView(_ view: EmptyPaneDropView, context: Context) { view.tab = tab }
}
final class EmptyPaneDropView: NSView {
  var tab = ""
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
final class FloatingPanePreview: NSView {
  var title = ""
  var text = ""
  override var isFlipped: Bool { true }
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
  override func draw(_ dirtyRect: NSRect) {
    let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 10, yRadius: 10)
    NSColor(red: 18 / 255, green: 21 / 255, blue: 28 / 255, alpha: 0.96).setFill()
    path.fill()
    NSColor.controlAccentColor.withAlphaComponent(0.8).setStroke()
    path.lineWidth = 1
    path.stroke()
    let clip = NSBezierPath(rect: bounds.insetBy(dx: 12, dy: 10))
    clip.addClip()
    (title as NSString).draw(
      in: NSRect(x: 12, y: 10, width: bounds.width - 24, height: 20),
      withAttributes: [
        .font: NSFont.systemFont(ofSize: 11, weight: .semibold), .foregroundColor: NSColor.white,
      ])
    (text as NSString).draw(
      in: NSRect(x: 12, y: 38, width: bounds.width - 24, height: bounds.height - 48),
      withAttributes: [
        .font: NSFont.monospacedSystemFont(ofSize: 10, weight: .regular),
        .foregroundColor: NSColor.white.withAlphaComponent(0.85),
      ])
  }
}
final class PaneSnapView: NSView {
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
  override func draw(_ dirtyRect: NSRect) {
    let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 2, dy: 2), xRadius: 8, yRadius: 8)
    NSColor.controlAccentColor.withAlphaComponent(0.18).setFill()
    path.fill()
    NSColor.controlAccentColor.withAlphaComponent(0.8).setStroke()
    path.lineWidth = 2
    path.stroke()
  }
}

final class PaneDragController {
  weak var store: Store?
  weak var window: NSWindow?
  var source: String?
  var origin = NSPoint.zero
  var previous: (String, String, String)?
  var started = false
  var monitor: Any?
  var observer: Any?
  var destination: [String: Any]?
  var overlay: PaneSnapView?
  var ghost: FloatingPanePreview?
  var ghostOffset = NSPoint.zero
  var lastPoint = NSPoint.zero
  var timer: Timer?
  var hoverID: String?
  var hoverWork: DispatchWorkItem?
  init(store: Store) { self.store = store }
  func begin(pane: String, event: NSEvent, window: NSWindow) {
    guard let store, !store.busy, store.panes[pane]?.agentRole != "coordinator" else { return }
    cleanup(restore: false)
    self.window = window
    source = pane
    origin = event.locationInWindow
    previous = (store.selected, store.tab, pane)
    store.focus = pane
    store.focusTerminal()
    monitor = NSEvent.addLocalMonitorForEvents(matching: [
      .leftMouseDragged, .leftMouseUp, .keyDown,
    ]) { [weak self] event in
      guard let self else { return event }
      return self.handle(event)
    }
    observer = NotificationCenter.default.addObserver(
      forName: NSWindow.didResignKeyNotification, object: window, queue: .main
    ) { [weak self] _ in
      self?.cleanup(restore: true)
    }
  }
  func handle(_ event: NSEvent) -> NSEvent? {
    guard let store, let source else { return event }
    if event.type == .keyDown {
      if event.keyCode == 53 {
        cleanup(restore: true)
        return nil
      }
      return started ? nil : event
    }
    if event.type == .leftMouseUp {
      let wasStarted = started
      if event.window === window { update(event.locationInWindow) } else { destination = nil }
      let target = destination
      if wasStarted, let target {
        stopTracking()
        overlay?.removeFromSuperview()
        overlay = nil
        store.movePane(source, destination: target) { [weak self] in self?.cleanup(restore: false) }
      } else {
        cleanup(restore: true)
      }
      return wasStarted ? nil : event
    }
    guard event.window === window else { return event }
    if !started {
      guard hypot(event.locationInWindow.x - origin.x, event.locationInWindow.y - origin.y) >= 4
      else { return event }
      guard store.canMovePanes else {
        store.error = "Moving panes requires the updated daemon. Existing shells are still running."
        cleanup(restore: false)
        return nil
      }
      started = true
      makeGhost(source: source, point: event.locationInWindow)
      store.draggingPane = source
      withAnimation(WorkspaceFinder.animation) { store.liftedPane = source }
      lastPoint = event.locationInWindow
      timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30, repeats: true) { [weak self] _ in
        guard let self, self.started else { return }
        self.update(self.lastPoint)
      }
      if let timer { RunLoop.main.add(timer, forMode: .common) }
      let overlay = PaneSnapView(frame: .zero)
      window?.contentView?.addSubview(overlay)
      self.overlay = overlay
      if let ghost {
        window?.contentView?.addSubview(ghost, positioned: .above, relativeTo: overlay)
      }
    }
    NSCursor.closedHand.set()
    update(event.locationInWindow)
    return nil
  }
  func update(_ point: NSPoint) {
    guard let store, let content = window?.contentView, let source else { return }
    lastPoint = point
    let cursor = content.convert(point, from: nil)
    ghost?.setFrameOrigin(NSPoint(x: cursor.x - ghostOffset.x, y: cursor.y - ghostOffset.y))
    destination = nil
    overlay?.isHidden = true
    func find(_ view: NSView) -> NSView? {
      guard !view.isHiddenOrHasHiddenAncestor else { return nil }
      if let pane = view as? PaneDropView, pane.pane == source { return nil }
      if view is PaneDropView || view is EmptyPaneDropView || view is TabDragView
        || view is WorkspaceOutline,
        view.convert(view.visibleRect.intersection(view.bounds), to: nil).contains(point)
      {
        return view
      }
      for child in view.subviews { if let found = find(child) { return found } }
      return nil
    }
    var hover: (String, () -> Void)?
    if let pane = find(content) as? PaneDropView, pane.pane != source {
      let rect = pane.convert(pane.bounds, to: content)
      let local = pane.convert(point, from: nil)
      let x = local.x / max(1, pane.bounds.width)
      let y =
        pane.isFlipped
        ? local.y / max(1, pane.bounds.height) : 1 - local.y / max(1, pane.bounds.height)
      let edges: [(String, CGFloat)] = [
        ("Left", x), ("Right", 1 - x), ("Above", y), ("Below", 1 - y),
      ]
      let edge = edges.min { $0.1 < $1.1 }!
      let placement = edge.0
      destination = ["Pane": ["pane": pane.pane, "placement": placement]]
      var zone = rect
      switch placement {
      case "Left": zone.size.width /= 2
      case "Right":
        zone.origin.x += zone.width / 2
        zone.size.width /= 2
      case "Above":
        zone.size.height /= 2
        if !content.isFlipped { zone.origin.y += zone.height }
      case "Below":
        zone.size.height /= 2
        if content.isFlipped { zone.origin.y += zone.height }
      default: break
      }
      show(zone)
    } else if let empty = find(content) as? EmptyPaneDropView {
      destination = ["Tab": ["tab": empty.tab]]
      show(empty.convert(empty.bounds, to: content))
    } else if let tab = find(content) as? TabDragView,
      let w = store.workspaces.first(where: { $0.tabs.contains { $0.id == tab.tabID } })
    {
      let id = tab.tabID
      destination = ["Tab": ["tab": id]]
      hover = ("tab-" + id, { store.select(w.id, tab: id) })
      show(tab.convert(tab.bounds, to: content))
    } else if let outline = find(content) as? WorkspaceOutline {
      let row = outline.row(at: outline.convert(point, from: nil))
      if row >= 0, let item = outline.item(atRow: row) as? WorkspaceSidebar.Item,
        let w = item.workspace, !store.isCreatingWorkspace(w.id)
      {
        destination = ["Workspace": ["workspace": w.id]]
        hover = ("workspace-" + w.id, { store.select(w.id) })
        show(outline.convert(outline.rect(ofRow: row), to: content))
      }
    }
    if hoverID != hover?.0 {
      hoverWork?.cancel()
      hoverWork = nil
      hoverID = hover?.0
      if let hover {
        let work = DispatchWorkItem { [weak self] in
          guard self?.started == true else { return }
          hover.1()
        }
        hoverWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45, execute: work)
      }
    }
  }
  func makeGhost(source: String, point: NSPoint) {
    guard let store, let content = window?.contentView else { return }
    var rect = NSRect(x: 0, y: 0, width: 400, height: 240)
    func visit(_ view: NSView) {
      if let pane = view as? PaneDropView, pane.pane == source {
        rect = pane.convert(pane.bounds, to: content)
      }
      for child in view.subviews { visit(child) }
    }
    visit(content)
    let size = NSSize(
      width: min(400, max(180, rect.width * 0.7)), height: min(260, max(110, rect.height * 0.7)))
    let ghost = FloatingPanePreview(frame: NSRect(origin: .zero, size: size))
    ghost.title = store.panes[source]?.title ?? "Terminal"
    ghost.text = store.terminalViews.existing(source)?.meta["text"] as? String ?? ""
    ghost.wantsLayer = true
    ghost.shadow = NSShadow()
    ghost.shadow?.shadowBlurRadius = 14
    ghost.shadow?.shadowColor = NSColor.black.withAlphaComponent(0.4)
    let cursor = content.convert(point, from: nil)
    ghostOffset = NSPoint(
      x: min(size.width - 16, max(16, (cursor.x - rect.minX) / max(1, rect.width) * size.width)),
      y: content.isFlipped ? 16 : size.height - 16)
    ghost.setFrameOrigin(NSPoint(x: cursor.x - ghostOffset.x, y: cursor.y - ghostOffset.y))
    self.ghost = ghost
  }
  func stopTracking() {
    if let monitor { NSEvent.removeMonitor(monitor) }
    monitor = nil
    if let observer { NotificationCenter.default.removeObserver(observer) }
    observer = nil
    timer?.invalidate()
    timer = nil
    hoverWork?.cancel()
    hoverWork = nil
    hoverID = nil
  }
  func show(_ rect: NSRect) {
    overlay?.frame = rect
    overlay?.isHidden = false
    overlay?.needsDisplay = true
  }
  func cleanup(restore: Bool) {
    let wasStarted = started
    let previous = self.previous
    stopTracking()
    ghost?.removeFromSuperview()
    ghost = nil
    withAnimation(WorkspaceFinder.animation) { store?.liftedPane = nil }
    overlay?.removeFromSuperview()
    overlay = nil
    source = nil
    started = false
    destination = nil
    self.previous = nil
    store?.draggingPane = nil
    NSCursor.arrow.set()
    if restore, wasStarted, let previous {
      store?.select(previous.0, tab: previous.1, pane: previous.2)
    } else if wasStarted {
      store?.focusTerminal()
    }
  }
}
extension Store {
  func movePane(_ pane: String, destination: [String: Any], completion: @escaping () -> Void) {
    guard !busy else {
      completion()
      return
    }
    movingLayout = true
    busy = true
    DispatchQueue.global().async {
      let reply = rpc(["MovePane": ["pane": pane, "destination": destination]])
      DispatchQueue.main.async {
        self.busy = false
        self.movingLayout = false
        withAnimation(WorkspaceFinder.animation) {
          if let state = reply["State"] as? [String: Any] {
            self.apply(state)
            self.liftedPane = nil
            if let w = self.workspaces.first(where: {
              $0.tabs.contains { $0.root.ids.contains(pane) }
            }),
              let tab = w.tabs.first(where: { $0.root.ids.contains(pane) })
            {
              self.select(w.id, tab: tab.id, pane: pane)
            }
          } else {
            self.liftedPane = nil
            self.error = reply["error"] as? String ?? "Could not move pane"
            self.focusTerminal()
          }
        }
        DispatchQueue.main.async {
          completion()
          self.trimTerminalViews()
        }
      }
    }
  }
}
