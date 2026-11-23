import AppKit
import SwiftUI

extension Store {
  func refreshCapabilities(force: Bool = false) {
    let endpoint = currentEndpoint
    if !force, let cached = capabilitiesByEndpoint[endpoint] {
      applyCapabilities(cached)
      return
    }
    guard capabilitiesLoading.insert(endpoint).inserted else { return }
    DispatchQueue.global().async {
      let supported = rpc("Capabilities", endpoint: endpoint)["Capabilities"] as? [String]
      DispatchQueue.main.async {
        self.capabilitiesLoading.remove(endpoint)
        if let supported { self.capabilitiesByEndpoint[endpoint] = Set(supported) }
        guard self.currentEndpoint == endpoint else { return }
        self.applyCapabilities(Set(supported ?? []))
      }
    }
  }
  private func applyCapabilities(_ supported: Set<String>) {
    let firstFocus = !canNameWorkspaces && supported.contains("workspace_names")
    if canReadGitSummary != supported.contains("git_summary") {
      canReadGitSummary = supported.contains("git_summary")
    }
    if canMoveTabs != supported.contains("move_tab") {
      canMoveTabs = supported.contains("move_tab")
    }
    if canMovePanes != supported.contains("move_pane") {
      canMovePanes = supported.contains("move_pane")
    }
    if canNameWorkspaces != supported.contains("workspace_names") {
      canNameWorkspaces = supported.contains("workspace_names")
    }
    canOrderWorkspaces = supported.contains("workspace_order")
    if firstFocus { reportFocus() }
  }
  func beginTabDrag(_ id: String) -> Bool {
    guard !busy else { return false }
    guard canMoveTabs else {
      error =
        "Tab moving requires the updated daemon. Finish active work before restarting the daemon."
      return false
    }
    dragOriginal = workspaces
    draggingTab = id
    return true
  }
  func previewTabMove(to: Int) {
    guard let draggingTab,
      let w = workspaces.firstIndex(where: { $0.tabs.contains { $0.id == draggingTab } }),
      let from = workspaces[w].tabs.firstIndex(where: { $0.id == draggingTab }),
      workspaces[w].tabs.indices.contains(to), from != to
    else { return }
    withAnimation(.timingCurve(0.22, 1, 0.36, 1, duration: 0.22)) {
      let tab = workspaces[w].tabs.remove(at: from)
      workspaces[w].tabs.insert(tab, at: to)
    }
  }
  func cancelTabDrag() {
    guard draggingTab != nil else { return }
    if let original = dragOriginal { workspaces = original }
    draggingTab = nil
    dragOriginal = nil
    focusTerminal()
  }
  func finishTabDrag(workspace: String) -> Bool {
    guard let id = draggingTab, let target = workspaces.first(where: { $0.id == workspace }) else {
      return false
    }
    let index = target.tabs.firstIndex(where: { $0.id == id }) ?? target.tabs.count
    let original = dragOriginal
    movingLayout = true
    busy = true
    draggingTab = nil
    dragOriginal = nil
    DispatchQueue.global().async {
      let reply = rpc(["MoveTab": ["tab": id, "workspace": workspace, "index": index]])
      DispatchQueue.main.async {
        self.busy = false
        self.movingLayout = false
        if let state = reply["State"] as? [String: Any] {
          self.apply(state)
          self.select(workspace, tab: id)
        } else {
          if let original { self.workspaces = original }
          self.error = reply["error"] as? String ?? "Could not move tab"
        }
      }
    }
    return true
  }
}

struct DraggableTab: NSViewRepresentable {
  let id: String
  let title: String
  let selected: Bool
  var activityStatus: ActivityStatus? = nil
  let editing: Bool
  let edit: () -> Void
  let rename: (String?) -> Void
  let click: () -> Void
  let close: () -> Void
  let begin: () -> Bool
  let end: () -> Void
  let siblings: [String]
  let reorder: (Int) -> Void
  let drop: () -> Bool
  func makeNSView(context: Context) -> TabDragView { TabDragView() }
  func updateNSView(_ view: TabDragView, context: Context) {
    view.activityStatus = activityStatus
    view.tabID = id
    view.label.stringValue = title
    view.setAccessibilityLabel(title)
    view.setAccessibilityValue(selected ? "Selected" : "")
    view.click = click
    view.close = close
    view.begin = begin
    view.end = end
    view.siblings = siblings
    view.reorder = reorder
    view.drop = drop
    view.edit = edit
    if editing && view.editor == nil {
      view.beginRename(rename)
    }
  }
}

final class TabDragView: NSView {
  static func preferredWidth(for title: String, showsStatus: Bool = false) -> CGFloat {
    // Measure the actual control, including NSTextField's text insets. Measuring
    // glyphs alone leaves the field too narrow and truncates even "Tab 1".
    let field = NSTextField(labelWithString: title)
    field.font = .systemFont(ofSize: 12)
    field.maximumNumberOfLines = 1
    let extra: CGFloat = showsStatus ? 14 : 0
    return min(260, max(80 + extra, ceil(field.intrinsicContentSize.width) + 40 + extra))
  }
  var tabID = ""
  let statusIndicator = ActivityDotView()
  var activityStatus: ActivityStatus? {
    didSet {
      statusIndicator.isHidden = activityStatus == nil
      statusIndicator.status = activityStatus ?? .unknown
      needsLayout = true
    }
  }
  let label = NSTextField(labelWithString: "")
  let icon = NSImageView(
    image: NSImage(systemSymbolName: "terminal", accessibilityDescription: nil)!)
  var editor: InlineNameEditor?
  var edit: (() -> Void)?
  var click: (() -> Void)?
  var close: (() -> Void)?
  private var middlePressed = false
  var begin: (() -> Bool)?
  var end: (() -> Void)?
  var siblings: [String] = []
  var reorder: ((Int) -> Void)?
  var reorderSession: ReorderSession?
  weak var dragClip: NSClipView?
  var stripBounds = NSRect.zero
  var drop: (() -> Bool)?
  var destination: (() -> Bool)?
  var ghost: DragGhost?
  weak var highlightedRow: NSTableRowView?
  var origin: NSEvent?
  var dragging = false
  var grabOffset = NSPoint.zero
  weak var previousResponder: NSResponder?
  override var acceptsFirstResponder: Bool { true }
  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    label.font = .systemFont(ofSize: 12)
    label.lineBreakMode = .byTruncatingTail
    label.isEditable = false
    label.isSelectable = false
    statusIndicator.isHidden = true
    addSubview(statusIndicator)
    addSubview(icon)
    addSubview(label)
    setAccessibilityElement(true)
    setAccessibilityRole(.button)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func layout() {
    super.layout()
    let extra: CGFloat = activityStatus == nil ? 0 : 14
    statusIndicator.frame = NSRect(x: 10, y: 11, width: 6, height: 6)
    icon.frame = NSRect(x: 10 + extra, y: 8, width: 12, height: 12)
    label.frame = NSRect(x: 28 + extra, y: 6, width: max(0, bounds.width - 36 - extra), height: 16)
  }
  override func hitTest(_ point: NSPoint) -> NSView? {
    if editor != nil { return super.hitTest(point) }
    return super.hitTest(point) == nil ? nil : self
  }
  func beginRename(_ completion: @escaping (String?) -> Void) {
    guard editor == nil else { return }
    let field = InlineNameEditor(frame: bounds.insetBy(dx: 4, dy: 3))
    field.stringValue = label.stringValue
    field.setAccessibilityLabel("Tab name")
    editor = field
    label.isHidden = true
    field.completion = { [weak self, weak field] value in
      if let name = value?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty {
        self?.label.stringValue = name
      }
      completion(value)
      field?.removeFromSuperview()
      self?.editor = nil
      self?.label.isHidden = false
    }
    // InlineNameEditor activates itself once it joins the window.
    addSubview(field)
  }
  override func otherMouseDown(with event: NSEvent) {
    guard event.buttonNumber == 2 else {
      super.otherMouseDown(with: event)
      return
    }
    middlePressed = !dragging
  }
  override func otherMouseUp(with event: NSEvent) {
    guard event.buttonNumber == 2 else {
      super.otherMouseUp(with: event)
      return
    }
    let shouldClose = middlePressed && bounds.contains(convert(event.locationInWindow, from: nil))
    middlePressed = false
    if shouldClose { close?() }
  }
  override func mouseDown(with event: NSEvent) {
    origin = event
    dragging = false
    reorderSession = nil
    dragClip = nil
    stripBounds = .zero
  }
  override func mouseUp(with event: NSEvent) {
    guard origin != nil else { return }
    if dragging {
      if event.window === window { updateDestination(event) } else { destination = nil }
      _ = destination?()
      end?()
      alphaValue = 1
      ghost?.removeFromSuperview()
      ghost = nil
      highlightedRow?.layer?.borderWidth = 0
    } else {
      if event.clickCount == 2 { edit?() } else { click?() }
    }
    if let previousResponder { window?.makeFirstResponder(previousResponder) }
    previousResponder = nil
    origin = nil
    dragging = false
    destination = nil
  }
  override func accessibilityPerformPress() -> Bool {
    click?()
    return true
  }
  override func mouseDragged(with event: NSEvent) {
    guard let origin, let window else { return }
    if !dragging {
      guard
        hypot(
          event.locationInWindow.x - origin.locationInWindow.x,
          event.locationInWindow.y - origin.locationInWindow.y) >= 3,
        begin?() == true
      else { return }
      dragging = true
      captureReorderSession(origin: origin, window: window)
      grabOffset = convert(origin.locationInWindow, from: nil)
      previousResponder = window.firstResponder
      window.makeFirstResponder(self)
      let image = NSImage(size: bounds.size)
      image.lockFocus()
      NSColor.controlBackgroundColor.withAlphaComponent(0.9).setFill()
      NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).fill()
      if let status = activityStatus {
        let dot = NSBezierPath(ovalIn: statusIndicator.frame.insetBy(dx: 0.5, dy: 0.5))
        status.color.setFill()
        status.color.setStroke()
        if status.filled { dot.fill() } else { dot.stroke() }
      }
      icon.image?.draw(in: icon.frame)
      label.stringValue.draw(
        in: label.frame,
        withAttributes: [
          .font: NSFont.systemFont(ofSize: 12), .foregroundColor: NSColor.labelColor,
        ])
      image.unlockFocus()
      let ghost = DragGhost(frame: bounds)
      ghost.image = image
      ghost.alphaValue = 1
      alphaValue = 0
      window.contentView?.addSubview(ghost)
      self.ghost = ghost
    }
    updateDestination(event)
  }
  override func keyDown(with event: NSEvent) {
    guard event.keyCode == 53, dragging else { return }
    end?()
    alphaValue = 1
    ghost?.removeFromSuperview()
    ghost = nil
    highlightedRow?.layer?.borderWidth = 0
    if let previousResponder { window?.makeFirstResponder(previousResponder) }
    previousResponder = nil
    origin = nil
    dragging = false
    destination = nil
  }
  func captureReorderSession(origin: NSEvent, window: NSWindow) {
    guard let content = window.contentView else { return }
    dragClip = enclosingScrollView?.contentView
    let scrollOffset = dragClip?.bounds.origin.x ?? 0
    var frames: [String: NSRect] = [:]
    func collect(_ view: NSView) {
      if let tab = view as? TabDragView, siblings.contains(tab.tabID), tab.window === window {
        frames[tab.tabID] = tab.convert(tab.bounds, to: nil)
      }
      for child in view.subviews { collect(child) }
    }
    collect(content)
    guard let source = siblings.firstIndex(of: tabID),
      frames.count == siblings.count
    else { return }
    let rects = siblings.compactMap { frames[$0] }
    stripBounds = rects.reduce(NSRect.null) { $0.union($1) }.insetBy(dx: 0, dy: -3)
    // Use strip/document coordinates, so horizontal scrolling does not move the
    // frozen thresholds. Reordering siblings never changes these drag targets.
    reorderSession = ReorderSession(
      centers: rects.map { $0.midX + scrollOffset }, source: source,
      grabPosition: origin.locationInWindow.x + scrollOffset)
  }
  func updateDestination(_ event: NSEvent) {
    guard let window, let content = window.contentView else { return }
    let point = content.convert(event.locationInWindow, from: nil)
    ghost?.setFrameOrigin(NSPoint(x: point.x - grabOffset.x, y: point.y - grabOffset.y))
    highlightedRow?.layer?.borderWidth = 0
    highlightedRow = nil
    destination = nil
    // SwiftUI installs transparent hosting views above representable controls.
    // Resolve drop targets by their visible window rectangles instead of the
    // hosting hierarchy's hitTest result.
    func target(in view: NSView) -> NSView? {
      guard !view.isHiddenOrHasHiddenAncestor else { return nil }
      if view is WorkspaceOutline,
        view.convert(view.visibleRect.intersection(view.bounds), to: nil).contains(
          event.locationInWindow)
      {
        return view
      }
      for child in view.subviews {
        if let found = target(in: child) { return found }
      }
      return nil
    }
    let strip = dragClip.map { $0.convert($0.bounds, to: nil) } ?? stripBounds
    if strip.contains(event.locationInWindow) {
      if let index = reorderSession?.destination(
        at: event.locationInWindow.x + (dragClip?.bounds.origin.x ?? 0))
      {
        reorder?(index)
      }
      destination = drop
    } else if let outline = target(in: content) as? WorkspaceOutline {
      let row = outline.row(at: outline.convert(event.locationInWindow, from: nil))
      if row >= 0, let item = outline.item(atRow: row) as? WorkspaceSidebar.Item,
        let workspace = item.workspace, let store = outline.coordinator?.store,
        !store.isCreatingWorkspace(workspace.id)
      {
        let rowView = outline.rowView(atRow: row, makeIfNecessary: false)
        rowView?.wantsLayer = true
        rowView?.layer?.borderColor = NSColor.controlAccentColor.cgColor
        rowView?.layer?.borderWidth = 1
        highlightedRow = rowView
        destination = { store.finishTabDrag(workspace: workspace.id) }
      }
    }
  }
}
final class DragGhost: NSImageView {
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
