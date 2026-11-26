import AppKit
import CJam
import SwiftUI

// Off-screen CI windows cannot reliably become the active OS window. Control
// that input here while exercising the real responder and daemon paths.
final class FocusTestWindow: NSWindow {
  var keyStatus: Bool?
  override var isKeyWindow: Bool { keyStatus ?? super.isKeyWindow }
}

struct ServerSheetFixture: View {
  @ObservedObject var store: Store
  var body: some View {
    ContentView().environmentObject(store)
      .sheet(isPresented: $store.showSettings) {
        AppSettingsView(store: store, close: { store.showSettings = false })
          .frame(width: 860, height: 600)
      }
      .sheet(item: $store.serverAction) {
        ServerActionSheet(action: $0).environmentObject(store)
      }
  }
}

struct SettingsSheetFixture: View {
  @ObservedObject var store: Store
  var body: some View {
    Color.clear.sheet(isPresented: $store.showSettings) {
      AppSettingsView(store: store, close: { store.showSettings = false })
        .frame(width: 860, height: 600)
    }
  }
}

final class WorktreeListFixtureState: ObservableObject {
  @Published var checkouts: [WorktreeCheckout] = (0..<1000).map {
    WorktreeCheckout(
      path: "/remote/worktree/\($0)", branch: "feature-\($0)", main: $0 == 0, locked: false)
  }
  @Published var selection: String? = "/remote/worktree/1"
  let opened: Set<String> = ["/remote/worktree/0", "/remote/worktree/2"]
  var opens = 0
}
struct WorktreeListFixture: View {
  @ObservedObject var state: WorktreeListFixtureState
  var body: some View {
    WorktreeList(
      checkouts: state.checkouts, opened: state.opened,
      selection: $state.selection, onOpen: { state.opens += 1 })
  }
}

/// Runs against a private daemon, without launching or modifying the user's app.
@main struct TerminalSwitchTests {
  static func main() throws {
    setbuf(stdout, nil)
    let application = NSApplication.shared
    application.setActivationPolicy(.accessory)
    application.finishLaunching()
    if CommandLine.arguments.contains("--mouse-gestures") {
      try checkTerminalMouseGestures()
      return
    }
    for provider in ["claude", "codex"] {
      assert(AgentBrand.image(provider) != nil, "Provider vector asset is missing")
      assert(
        AgentBrand.image(provider)?.isTemplate == true,
        "Provider icons must follow system appearance")
    }
    for title in ["Tab 1", "Tab 2", "WWW", "日常", "Development", "Review notes"] {
      let tab = TabDragView(
        frame: NSRect(x: 0, y: 0, width: TabDragView.preferredWidth(for: title), height: 28))
      tab.label.stringValue = title
      tab.layout()
      assert(
        tab.label.bounds.width >= ceil(tab.label.intrinsicContentSize.width),
        "Short tab title is truncated")
      assert(tab.bounds.width >= 80 && tab.bounds.width <= 260, "Tab width is outside its limits")
      if title == "Tab 1" { assert(tab.bounds.width < 92, "Short tabs should remain compact") }
    }
    checkMiddleClick()
    try checkImageClipboard()
    try checkCoordinatorControls()
    checkActivityIndicators()
    checkEmptyWorkspaceNames()
    checkCoordinatorStateMigration()
    checkContextRequestRouting()
    try checkUpgradeConsent()
    try checkWorktreeNavigation()
    try checkWorkspaceScrollIndicator()
    _ = nativeString(jam_start(""))
    defer { _ = rpc("Stop") }
    try checkTerminalMouseGestures()
    try checkOptimisticSplit()
    try checkManagedResume()
    let store = Store()
    try checkToastTimeouts()
    try checkUpgradeChip(store: store)
    try checkUpdateSettingsControls(store: store)
    try checkSettings(store: store)
    let directory = ProcessInfo.processInfo.environment["JAM_STATE_DIR"]!
    for name in ["first", "second"] {
      let path = directory + "/" + name
      try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
      apply(rpc(["CreateWorkspace": ["path": path, "name": name]]), to: store)
    }
    let first = store.workspaces[0].id
    let second = store.workspaces[1].id
    let pane = store.workspaces[0].tabs[0].root.ids[0]
    apply(rpc(["Split": ["pane": pane, "vertical": false]]), to: store)
    apply(rpc(["NewTab": ["workspace": first]]), to: store)
    let destinations = store.workspaces.flatMap { workspace in
      workspace.tabs.map { (workspace.id, $0.id, $0.root.ids) }
    }
    assert(destinations.count == 3 && first != second)
    let window = FocusTestWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 1100, height: 700),
      styleMask: [.titled, .resizable], backing: .buffered, defer: false)
    window.contentView = NSHostingView(rootView: ServerSheetFixture(store: store))
    window.makeKeyAndOrderFront(nil)
    application.activate(ignoringOtherApps: true)
    try wait("native PTY geometry owner") { application.isActive && window.isKeyWindow }
    defer { window.orderOut(nil) }

    var cached: [String: (TerminalNSView, UnsafeMutableRawPointer, NSRect)] = [:]
    for (workspace, tab, ids) in destinations {
      store.select(workspace, tab: tab)
      try wait {
        ids.allSatisfy { id in
          guard let view = store.terminalViews.existing(id), view.window != nil,
            view.meta["ready"] as? Bool == true
          else { return false }
          return matchesGeometry(view)
        }
      }
      for id in ids {
        let view = store.terminalViews.existing(id)!
        cached[id] = (view, view.handle!, view.bounds)
      }
    }

    var timings: [Double] = []
    for index in 0..<18 {
      let (workspace, tab, ids) = destinations[index % destinations.count]
      let start = Date()
      store.select(workspace, tab: tab)
      try wait {
        ids.allSatisfy { id in
          guard let view = store.terminalViews.existing(id), let previous = cached[id] else {
            return false
          }
          assert(view === previous.0 && view.handle == previous.1, "Cached renderer was rebuilt")
          return view.window === window && view.bounds == previous.2 && matchesGeometry(view)
        }
      }
      timings.append(Date().timeIntervalSince(start) * 1000)
      RunLoop.current.run(until: Date().addingTimeInterval(0.03))
    }
    try checkRapidFocus(store: store, window: window)
    try checkInlineRename(store: store, window: window)
    try checkAutomaticFocusAcknowledgement(store: store, window: window)
    try checkServerRecovery(store: store, destinations: destinations, window: window)
    // A departing SwiftUI host may still receive layout. It must not steal the
    // pooled view back from its new destination or resize it to stale bounds.
    let view = cached.values.first!.0
    let old = TerminalSurfaceHost(terminal: view)
    old.frame.size = NSSize(width: 500, height: 400)
    view.desiredSize = old.bounds.size
    old.layout()
    let current = TerminalSurfaceHost(terminal: view)
    current.frame.size = NSSize(width: 600, height: 450)
    view.desiredSize = current.bounds.size
    current.layout()
    old.layout()
    assert(view.superview === current && view.bounds.size == current.bounds.size)

    timings.sort()
    print(
      "Cached workspace/tab layout: median \(timings[timings.count / 2]) ms, p95 \(timings.last!) ms"
    )
    print(
      "Native terminal tests passed: first-frame geometry, cached renderer reuse, and reparenting.")
    try checkUsagePanel(store: store)
    try checkReviewBadge(store: store, window: window)
    checkEmptyGroups(store: store, window: window)
    checkReconnectingMachine(store: store, window: window)
    checkWorkspaceHandoff(store: store, window: window)
    checkWorktreeMembership(window: window, store: store)
    store.serverStatuses[""] = ServerStatus(
      running: true, compatible: true, build: "old", availableBuild: "new", updateAvailable: true,
      terminals: 2, error: nil)
    store.serverAction = ServerAction(endpoint: "", name: "Local", kind: .restart)
    try wait("server action sheet") { window.attachedSheet != nil }
    assert(window.attachedSheet?.sheetParent === window)
    store.serverAction = nil
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    print("Server restart confirmation is attached to the Jam window.")
    checkSidebarAlignment(window: window, store: store)
    let details = NSHostingView(
      rootView: WarningDetails(
        text:
          "SSH setup check: ssh: connect to host dev-server port 22:\nOperation timed out\nConnection closed."
      ).frame(width: 332))
    details.frame = NSRect(x: 0, y: 0, width: 332, height: 200)
    window.contentView!.addSubview(details)
    RunLoop.current.run(until: Date().addingTimeInterval(0.15))
    assert(
      details.fittingSize.height > 30 && details.fittingSize.height <= 180,
      "Warning details were clipped to height \(details.fittingSize.height)")
    details.removeFromSuperview()
    try checkStoppedPaneFocusAndResume()
  }

  static func checkWorkspaceScrollIndicator() throws {
    let scroll = WorkspaceScrollView(frame: NSRect(x: 0, y: 0, width: 200, height: 200))
    scroll.documentView = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 1000))
    scroll.layoutSubtreeIfNeeded()
    assert(scroll.verticalScroller?.alphaValue == 0, "Workspace scrollbar starts hidden")
    guard
      let wheel = CGEvent(
        scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: -24, wheel2: 0,
        wheel3: 0), let event = NSEvent(cgEvent: wheel)
    else { throw NSError(domain: "Could not create a fixture scroll event", code: 1) }
    scroll.scrollWheel(with: event)
    assert(scroll.verticalScroller?.alphaValue == 1, "User scrolling reveals the scrollbar")
    try wait("workspace scrollbar fades when scrolling stops") {
      scroll.verticalScroller?.alphaValue == 0
    }
  }

  static func checkTerminalMouseGestures() throws {
    let response = rpc(["CreateWorkspace": ["path": "/tmp", "name": "Mouse gestures"]])
    guard let state = response["State"] as? [String: Any],
      let workspaces = state["workspaces"] as? [[String: Any]],
      let workspace = workspaces.last?["id"] as? String,
      let panes = state["panes"] as? [[String: Any]],
      let pane = panes.first(where: { $0["workspace"] as? String == workspace })?["id"] as? String
    else { throw NSError(domain: "Could not create gesture fixture: \(response)", code: 1) }
    defer { _ = rpc(["WorkspaceClose": ["workspace": workspace, "group": false]]) }
    let rect = NSRect(x: 0, y: 0, width: 640, height: 220)
    let window = FocusTestWindow(
      contentRect: rect, styleMask: [.borderless],
      backing: .buffered, defer: false)
    window.keyStatus = true
    window.isReleasedWhenClosed = false
    let view = TerminalNSView(pane: pane)
    view.frame = rect
    let board = NSPasteboard.withUniqueName()
    defer {
      board.releaseGlobally()
      window.close()
    }
    view.copyPasteboard = board
    var selectionCopies = 0
    view.onSelectionCopy = { selectionCopies += 1 }
    var opened: [URL] = []
    view.openWebURL = {
      opened.append($0)
      return true
    }
    window.contentView = view
    window.makeFirstResponder(view)
    func event(
      _ type: NSEvent.EventType, _ col: Int, _ row: Int = 0,
      _ flags: NSEvent.ModifierFlags = []
    ) -> NSEvent {
      let point = NSPoint(
        x: (CGFloat(col) + 0.5) * view.cellSize.width,
        y: rect.height - (CGFloat(row) + 0.5) * view.cellSize.height)
      return NSEvent.mouseEvent(
        with: type, location: view.convert(point, to: nil),
        modifierFlags: flags, timestamp: 0, windowNumber: window.windowNumber, context: nil,
        eventNumber: 0, clickCount: 1, pressure: 1)!
    }
    func send(_ command: String) {
      _ = rpc(["Send": ["pane": pane, "data": Array((command + "\r").utf8)]])
    }
    let link = "https://example.org/jam"
    send(
      #"printf '\033[2J\033[Hhttps://example.org/jam\r\n\033]8;;https://example.org/review\033\\Review\033]8;;\033\\\r\n'"#
    )
    try wait("gesture fixture rendered") {
      view.prepareForDisplay()
      let bytes = Data(view.action("link", 1, 1).utf8)
      let context = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any]
      return context?["url"] as? String == "https://example.org/review"
    }
    view.mouseDown(with: event(.leftMouseDown, 0))
    view.mouseDragged(with: event(.leftMouseDragged, link.count - 1))
    view.mouseUp(with: event(.leftMouseUp, link.count - 1))
    assert(
      board.string(forType: .string) == link,
      "Unexpected drag selection: \(String(describing: board.string(forType: .string)))")
    assert(selectionCopies == 1, "Automatic copy should show one confirmation")
    view.copy(nil)
    assert(selectionCopies == 1, "Manual Copy should not show a confirmation")
    view.mouseDown(with: event(.leftMouseDown, 4, 0, .control))
    view.mouseUp(with: event(.leftMouseUp, 4, 0, .control))
    view.rightMouseDown(with: event(.rightMouseDown, 1, 1, .control))
    view.rightMouseUp(with: event(.rightMouseUp, 1, 1, .control))
    assert(opened.map(\.absoluteString) == [link, "https://example.org/review"])
    assert(TerminalNSView.webLink(["url": "file:///etc/hosts"]) == nil)
    // A mouse-reporting app supplies different clipboard text on button release.
    send(
      #"python3 -c 'import os,tty; tty.setraw(0); os.write(1,b"\x1b[2J\x1b[HDrag here\x1b[?1002h\x1b[?1006h"); exec("while os.read(0,1) != b\"m\": pass"); os.write(1,b"\x1b]52;c;ZnJvbSBhcHA=\x07\x1b[?1002l\x1b[?1006l")'; stty sane"#
    )
    try wait("mouse reporting enabled") {
      view.render()
      return view.meta["mouse"] as? Bool == true
    }
    view.mouseDown(with: event(.leftMouseDown, 0))
    view.mouseDragged(with: event(.leftMouseDragged, 3))
    view.mouseUp(with: event(.leftMouseUp, 3))
    try wait("application selection reached the client pasteboard") {
      view.render()
      return board.string(forType: .string) == "from app"
    }
    assert(selectionCopies == 2, "An app clipboard reply must not repeat the same confirmation")
    print("Client drag-copy, quick toast events, OSC 52 and hyperlink gestures passed")
  }

  static func checkOptimisticSplit() throws {
    let terminal = TerminalNSView(pane: "animation-fixture")
    let host = TerminalSurfaceHost(terminal: terminal)
    terminal.desiredSize = CGSize(width: 460, height: 260)
    for width in [730.0, 630.0, 530.0, 460.0] {
      host.setFrameSize(CGSize(width: width, height: 360))
      host.layout()
      assert(terminal.bounds.size == terminal.desiredSize)
      assert(terminal.frame.maxY == host.bounds.maxY)
    }

    let store = Store()
    let directory = ProcessInfo.processInfo.environment["JAM_STATE_DIR"]!
    apply(rpc(["CreateWorkspace": ["path": directory, "name": "Split fixture"]]), to: store)
    let workspace = store.workspaces.last!
    defer { _ = rpc(["WorkspaceClose": ["workspace": workspace.id, "group": false]]) }
    let tab = workspace.tabs[0]
    let source = tab.root.ids[0]
    store.select(workspace.id, tab: tab.id, pane: source)
    let window = FocusTestWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 1100, height: 700),
      styleMask: [.titled, .resizable], backing: .buffered, defer: false)
    window.contentView = NSHostingView(rootView: ServerSheetFixture(store: store))
    window.makeKeyAndOrderFront(nil)
    let application = NSApplication.shared
    application.activate(ignoringOtherApps: true)
    defer { window.orderOut(nil) }
    try wait("split fixture owns PTY geometry") { application.isActive && window.isKeyWindow }
    try wait("initial split terminal") {
      guard let view = store.terminalViews.existing(source) else { return false }
      return view.desiredSize != nil && matchesGeometry(view) && store.paneSizes[source] != nil
    }
    let view = store.terminalViews.existing(source)!
    let originalWidth = view.desiredSize!.width

    let replyGate = DispatchSemaphore(value: 0)
    var gateReleased = false
    defer { if !gateReleased { replyGate.signal() } }
    endpointQueue("").async { replyGate.wait() }
    store.split(false)
    let pending = store.pendingPaneSplits[tab.id]!
    assert(store.currentTab?.root.ids == [source])
    assert(pending.preview(in: tab.root).ids == [source, pending.placeholder])
    assert(store.focus == source && store.panes[pending.placeholder] == nil)
    store.split(true)
    assert(store.pendingPaneSplits[tab.id]?.placeholder == pending.placeholder)
    try wait("optimistic pane layout and destination terminal size") {
      guard let width = view.desiredSize?.width, width < originalWidth * 0.55 else { return false }
      return view.meta["cols"] as? Int == Int(width / view.cellSize.width)
    }
    assert(store.currentTab?.root.ids == [source], "Preview must precede server confirmation")
    assert(store.terminalViews.existing(pending.placeholder) == nil)
    replyGate.signal()
    gateReleased = true

    try wait("split confirmation") { store.pendingPaneSplits[tab.id] == nil }
    let committed = store.currentTab!.root
    let created = pending.createdPane(in: committed)!
    assert(committed.ids == [source, created] && store.focus == created)
    assert(pending.preview(in: committed).ids == committed.ids)

    // The pane visible in the client can have already been closed by a second client.
    _ = rpc(["Close": ["pane": created]])
    store.paneSizes[created] = CGSize(width: 640, height: 480)
    store.split(true)
    let rejected = store.pendingPaneSplits[tab.id]!
    assert(rejected.preview(in: committed).ids == [source, created, rejected.placeholder])
    try wait("split rejection") { store.pendingPaneSplits[tab.id] == nil }
    assert(!store.error.isEmpty && store.panes[rejected.placeholder] == nil)
    assert(store.currentTab?.root.ids.contains(rejected.placeholder) == false)

    apply(rpc(["Split": ["pane": source, "vertical": true]]), to: store)
    let middle = store.currentTab!.root.ids.last!
    apply(rpc(["Split": ["pane": middle, "vertical": true]]), to: store)
    let bottom = store.currentTab!.root.ids.last!
    store.select(workspace.id, tab: tab.id, pane: bottom)
    store.execute("close_pane")
    assert(store.focus == middle, "Closing a pane should immediately focus its closest neighbor")
    try wait("close confirmation preserves adjacent pane focus") {
      store.pendingLayoutEdits.isEmpty && store.panes[bottom] == nil && store.focus == middle
    }
    print("Optimistic split passed with a delayed reply, confirmation, and rejection.")
  }

  static func checkManagedResume() throws {
    let store = Store()
    let directory = ProcessInfo.processInfo.environment["JAM_STATE_DIR"]!
    apply(rpc(["CreateWorkspace": ["path": directory, "name": "Resume fixture"]]), to: store)
    let workspace = store.workspaces.last!
    defer { _ = rpc(["WorkspaceClose": ["workspace": workspace.id, "group": false]]) }
    let response = rpc([
      "Coordination": ["action": "start", "workspace": workspace.id, "provider": "claude"]
    ])
    guard let started = (response["Coordination"] as? [String: Any])?["agent"] as? [String: Any],
      let id = started["id"] as? String, let pane = started["pane"] as? String
    else { throw NSError(domain: "Could not start fixture agent: \(response)", code: 1) }
    try wait("managed agent launched") {
      (rpc(["Capture": ["pane": pane]])["Capture"] as? String)?.contains("fixture agent ready")
        == true
    }
    assert(rpc(["Coordination": ["action": "pause", "agent": id]])["Coordination"] != nil)
    apply(rpc("State"), to: store)
    assert(store.panes[pane]?.needsResume == true)
    assert(store.panes[pane].flatMap { store.resumeState(for: $0) } == .managed(id))
    let replyGate = DispatchSemaphore(value: 0)
    endpointQueue("").async { replyGate.wait() }
    store.restartPane(pane)
    assert(store.restartingPanes.contains(pane) && store.panes[pane]?.needsResume == true)
    replyGate.signal()
    try wait("managed resume refreshes pane state without polling") {
      !store.restartingPanes.contains(pane)
    }
    assert(store.error.isEmpty && store.panes[pane]?.needsResume == false)
    print("Managed resume keeps the pane pending through the state refresh.")
  }

  static func checkStoppedPaneFocusAndResume() throws {
    let store = Store()
    let directory = ProcessInfo.processInfo.environment["JAM_STATE_DIR"]!
    apply(rpc(["NewWorkspace": ["path": directory]]), to: store)
    let workspace = store.workspaces.last!
    let stopped = workspace.tabs[0].root.ids[0]
    apply(rpc(["Split": ["pane": stopped, "vertical": false]]), to: store)
    let live = store.panes.values.first {
      $0.id != stopped && store.workspaces.last!.tabs[0].root.ids.contains($0.id)
    }!.id
    let session = UUID().uuidString.lowercased()
    let record =
      rpc([
        "Coordination": [
          "action": "register", "pane": stopped, "provider": "claude", "session": session,
          "launch_flags": [],
        ]
      ])["Coordination"] as! [String: Any]
    assert(record["verified_launch"] as? Bool == false)
    let agent = record["id"] as! String
    assert(rpc(["Coordination": ["action": "pause", "agent": agent]])["error"] == nil)
    try wait("stopped agent record") {
      apply(rpc("State"), to: store)
      return store.panes[stopped]?.exited == true
    }
    assert(store.resumeState(for: store.panes[stopped]!) == .manual("claude --resume " + session))
    assert(
      StoppedAgentResume(["id": agent, "session": "--invalid"], provider: "claude")
        == .manual("claude --resume"))
    store.restartPane(stopped)
    assert(store.restartingPanes.isEmpty && store.error.isEmpty)
    store.select(workspace.id, pane: live)
    let window = FocusTestWindow(
      contentRect: NSRect(x: -3200, y: -3200, width: 800, height: 400),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.keyStatus = true
    window.contentView = NSHostingView(
      rootView: HStack(spacing: 0) {
        PaneCard(id: stopped)
        PaneCard(id: live)
      }.environmentObject(store))
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    try wait("live sibling attached") { store.terminalViews.existing(live)?.window === window }
    let liveView = store.terminalViews.existing(live)!
    assert(window.makeFirstResponder(liveView) && window.firstResponder === liveView)
    // Click empty space well outside the label and controls on the stopped pane.
    let click = NSPoint(x: 24, y: 45)
    assert(store.focus == live)
    let hit = window.contentView!.hitTest(window.contentView!.convert(click, from: nil))
    assert(hit != nil)
    for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
      window.sendEvent(
        NSEvent.mouseEvent(
          with: type, location: click, modifierFlags: [],
          timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
          context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!)
    }
    try wait("stopped pane receives click and keyboard focus") {
      store.focus == stopped && window.firstResponder === hit
    }
    assert(store.enabled("close_pane"))
    store.execute("close_pane")
    try wait("close shortcut removes only the stopped pane") {
      store.panes[stopped] == nil && store.panes[live]?.exited == false
    }
    print(
      "Stopped split pane accepts focus and closes independently. Unknown launch permissions show manual resume."
    )
  }

  static func checkRapidFocus(store: Store, window: FocusTestWindow) throws {
    let workspace = store.workspaces.first { $0.tabs.contains { $0.root.ids.count == 2 } }!
    let tab = workspace.tabs.first { $0.root.ids.count == 2 }!
    let panes = tab.root.ids
    store.select(workspace.id, tab: tab.id)
    window.keyStatus = true
    defer { window.keyStatus = nil }
    try wait("two attached focus targets") {
      panes.allSatisfy { store.terminalViews.existing($0)?.window === window }
    }
    let gate = DispatchSemaphore(value: 0)
    endpointQueue("").async { gate.wait() }
    defer { gate.signal() }
    var samples: [Double] = []
    for click in 0..<80 {
      let pane = panes[click % 2]
      let terminal = store.terminalViews.existing(pane)!
      let event = NSEvent.mouseEvent(
        with: .leftMouseDown, location: terminal.convert(NSPoint(x: 12, y: 12), to: nil),
        modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
        context: nil, eventNumber: click, clickCount: 1, pressure: 1)!
      let start = ProcessInfo.processInfo.systemUptime
      terminal.mouseDown(with: event)
      assert(store.focus == pane && window.firstResponder === terminal)
      samples.append((ProcessInfo.processInfo.systemUptime - start) * 1000)
    }
    samples.sort()
    print(
      "Rapid mouse-down to local responder with stalled control RPCs: p95 \(samples[75]) ms, p99 \(samples[79]) ms."
    )
  }

  static func checkAutomaticFocusAcknowledgement(store: Store, window: NSWindow) throws {
    let focusWindow = window as! FocusTestWindow
    focusWindow.keyStatus = true
    defer { focusWindow.keyStatus = nil }
    let workspace = store.workspaces[0]
    let tab = workspace.tabs.first { $0.root.ids.count > 1 }!
    let target = tab.root.ids[0]
    let sibling = tab.root.ids[1]
    store.select(store.workspaces[1].id)
    try wait("focus away from target") { store.focus != target }
    func markDone() {
      for pane in [target, sibling] { _ = rpc(["Status": ["pane": pane, "state": "done"]]) }
      apply(rpc("State"), to: store)
    }
    func status(_ pane: String) -> String? {
      let state = rpc("State")["State"] as? [String: Any]
      return (state?["panes"] as? [[String: Any]])?.first { $0["id"] as? String == pane }?["status"]
        as? String
    }
    markDone()
    store.select(workspace.id, tab: tab.id, pane: target)
    try wait("automatic pane focus acknowledges status") { status(target) == "unknown" }
    assert(status(sibling) == "done", "Focusing one pane cleared another pane's unread status")
    markDone()
    store.select(workspace.id, tab: tab.id, pane: target)
    try wait("reselecting focused pane acknowledges status") { status(target) == "unknown" }
    assert(status(sibling) == "done")
    focusWindow.keyStatus = false
    markDone()
    store.terminalViews.existing(target)!.acknowledgeFocus()
    assert(status(target) == "done", "An inactive window cleared unread status")
    focusWindow.keyStatus = true
    store.focusTerminal()
    try wait("foreground focus acknowledges status") { status(target) == "unknown" }
    print(
      "Automatic focus acknowledges only the selected pane, including repeated workspace selection."
    )
  }

  static func checkImageClipboard() throws {
    let board = NSPasteboard(name: NSPasteboard.Name("jam.image-test.\(UUID())"))
    defer { board.releaseGlobally() }
    board.setString("plain text", forType: .string)
    assert(!TerminalClipboard.hasImage(board))
    let textOnly = try TerminalClipboard.image(board)
    assert(textOnly == nil)
    let bitmap = NSBitmapImageRep(
      bitmapDataPlanes: nil, pixelsWide: 1, pixelsHigh: 1, bitsPerSample: 8,
      samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
      colorSpaceName: .deviceRGB, bytesPerRow: 4, bitsPerPixel: 32)!
    bitmap.bitmapData!.initialize(repeating: 255, count: 4)
    let png = bitmap.representation(using: .png, properties: [:])!
    board.clearContents()
    board.setData(png, forType: .png)
    board.setString("ignored image label", forType: .string)
    assert(TerminalClipboard.hasImage(board))
    let original = try TerminalClipboard.image(board)
    assert(original == png, "Image data must take precedence over its text representation")
    board.clearContents()
    board.setData(bitmap.tiffRepresentation!, forType: .tiff)
    let converted = try TerminalClipboard.image(board)!
    assert(converted.starts(with: [137, 80, 78, 71, 13, 10, 26, 10]))
    assert(TerminalClipboard.quotedPath("/tmp/a'b.png") == "'/tmp/a'\\''b.png' ")
    board.clearContents()
    board.setData(Data(count: TerminalClipboard.maximumImageBytes + 1), forType: .png)
    do {
      _ = try TerminalClipboard.image(board)
      assertionFailure("Oversized clipboard image was accepted")
    } catch TerminalClipboard.ImageError.tooLarge {}
    print("Clipboard images passed: PNG, TIFF, text precedence, quoting, and size limit.")
  }

  static func checkInlineRename(store: Store, window: NSWindow) throws {
    func descendants<T: NSView>(_ root: NSView, of type: T.Type) -> [T] {
      (root as? T).map { [$0] } ?? root.subviews.flatMap { descendants($0, of: type) }
    }
    func mouse(_ type: NSEvent.EventType, in view: NSView, point: NSPoint) -> NSEvent {
      NSEvent.mouseEvent(
        with: type, location: view.convert(point, to: nil), modifierFlags: [],
        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
        context: nil, eventNumber: 0, clickCount: 2, pressure: 1)!
    }
    func liveEditor(_ stage: String, _ field: @escaping () -> InlineNameEditor?) throws -> (
      InlineNameEditor, NSTextView
    ) {
      try wait("inline rename focus: " + stage) { field()?.currentEditor() != nil }
      let editor = field()!
      let text = editor.currentEditor() as! NSTextView
      text.setSelectedRange(NSRange(location: 1, length: 0))
      // Additional activation must not end the session or replace its selection.
      activateInlineEditor(editor)
      activateInlineEditor(editor)
      RunLoop.current.run(until: Date().addingTimeInterval(0.15))
      assert(
        editor.window === window && editor.currentEditor() === text,
        "Inline rename ended while acquiring focus")
      assert(
        text.selectedRange() == NSRange(location: 1, length: 0),
        "Repeated activation changed the editing selection")
      return (editor, text)
    }
    func enter(_ text: NSTextView, name: String) {
      text.insertText(
        name, replacementRange: NSRange(location: 0, length: (text.string as NSString).length))
      text.doCommand(by: #selector(NSResponder.insertNewline(_:)))
    }
    // SwiftUI can request auto-rename before the new tab has a window.
    let unattached = TabDragView(frame: NSRect(x: 0, y: 0, width: 120, height: 28))
    unattached.label.stringValue = "New tab"
    var prematurelyFinished = false
    unattached.beginRename { _ in prematurelyFinished = true }
    assert(unattached.editor != nil && !prematurelyFinished)
    // A real new-tab rename takes the store's input gate before attachment.
    // Keep a queued terminal responder update from the rapid-focus test out.
    store.editingTab = "unattached-rename-fixture"
    window.contentView!.addSubview(unattached)
    let (_, newText) = try liveEditor("attached standalone field") { unattached.editor }
    newText.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
    unattached.removeFromSuperview()
    store.editingTab = nil
    let workspace = store.workspaces[0].id
    let tab = store.workspaces[0].tabs[0].id
    store.select(workspace, tab: tab)
    try wait("tab for rename") {
      descendants(window.contentView!, of: TabDragView.self).contains { $0.tabID == tab }
    }
    // Both the plus action and configured shortcut select the new name field,
    // then Enter transfers focus into that tab's terminal.
    var created: [String] = []
    for index in 0..<2 {
      let oldTabs = Set(store.current!.tabs.map(\.id))
      if index == 0 { store.newTab() } else { store.execute("new_tab") }
      try wait("new tab rename") {
        guard let editing = store.editingTab else { return false }
        return !oldTabs.contains(editing)
      }
      let newID = store.editingTab!
      created.append(newID)
      let (_, input) = try liveEditor("new tab") {
        descendants(window.contentView!, of: TabDragView.self).first { $0.tabID == newID }?.editor
      }
      if index == 0 {
        enter(input, name: "Fresh tab")
      } else {
        input.doCommand(by: #selector(NSResponder.insertNewline(_:)))
      }
      try wait("terminal focus after naming") {
        store.editingTab == nil && (window.firstResponder as? TerminalNSView)?.pane == store.focus
      }
      if index == 0 {
        try wait("new tab name saved") { store.currentTab?.name == "Fresh tab" }
      } else {
        assert(store.currentTab?.name.hasPrefix("Tab ") == true)
      }
    }
    for id in created {
      if let t = store.current?.tabs.first(where: { $0.id == id }) { store.closeTab(t) }
    }
    try wait("new tab cleanup") {
      !store.workspaces.flatMap(\.tabs).contains { created.contains($0.id) }
    }
    store.select(workspace, tab: tab)
    try wait("original tab restored") { store.currentTab?.id == tab }
    let tabView = descendants(window.contentView!, of: TabDragView.self).first { $0.tabID == tab }!
    // The context menu sets this same published action; its field must survive SwiftUI updates.
    store.editingTab = tab
    let (_, contextText) = try liveEditor("tab context menu") { tabView.editor }
    contextText.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
    try wait("tab rename cancellation") { tabView.editor == nil && store.editingTab == nil }
    let tabPoint = NSPoint(x: tabView.bounds.midX, y: tabView.bounds.midY)
    tabView.mouseDown(with: mouse(.leftMouseDown, in: tabView, point: tabPoint))
    tabView.mouseUp(with: mouse(.leftMouseUp, in: tabView, point: tabPoint))
    let (_, doubleText) = try liveEditor("tab double-click") { tabView.editor }
    let beforeTabRename = rpc("State")["State"] as! [String: Any]
    enter(doubleText, name: "Renamed tab")
    assert(
      tabView.label.stringValue == "Renamed tab", "Committing must not reveal the old native label")
    assert(
      store.projectedWorkspaces.flatMap(\.tabs).first { $0.id == tab }?.name == "Renamed tab",
      "Tab rename must be immediate")
    store.apply(beforeTabRename)
    assert(
      store.projectedWorkspaces.flatMap(\.tabs).first { $0.id == tab }?.name == "Renamed tab",
      "An in-flight poll must not undo a pending rename")
    try wait("tab rename acknowledgement") { store.pendingRenames[tab] == nil }
    store.apply(beforeTabRename)
    assert(
      store.projectedWorkspaces.flatMap(\.tabs).first { $0.id == tab }?.name == "Renamed tab",
      "A stale poll after acknowledgement must not undo the rename")
    try wait("tab rename commit") {
      store.projectedWorkspaces.flatMap(\.tabs).first { $0.id == tab }?.name == "Renamed tab"
    }
    let child = store.workspaces[1].id
    for (id, label) in [(workspace, "workspace"), (child, "worktree")] {
      if id == child, let index = store.workspaces.firstIndex(where: { $0.id == child }) {
        // Exercise the indented worktree row with its secondary branch label.
        store.workspaces[index].parent = workspace
        store.workspaces[index].branch = "feature/test"
        store.workspaces[index].repository = "rename-fixture"
      }
      let outline = sidebar(in: window)
      outline.coordinator!.update(outline)
      let item = outline.coordinator!.activeItems.first { $0.workspace?.id == id }!
      let menu = outline.coordinator!.menu(item)
      let rename = menu.items.first { $0.title == "Rename" }!
      assert(NSApp.sendAction(rename.action!, to: rename.target, from: rename))
      func field() -> InlineNameEditor? {
        outline.subviews.compactMap { $0 as? InlineNameEditor }.first
      }
      let (_, menuText) = try liveEditor(label + " context menu", field)
      menuText.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
      try wait("sidebar rename cancellation") { field() == nil && store.editingWorkspace == nil }
      let row = outline.row(forItem: item)
      let rect = outline.rect(ofRow: row)
      let point = NSPoint(x: rect.midX, y: rect.midY)
      outline.mouseDown(with: mouse(.leftMouseDown, in: outline, point: point))
      outline.mouseUp(with: mouse(.leftMouseUp, in: outline, point: point))
      let (_, text) = try liveEditor(label + " double-click", field)
      let beforeRename = rpc("State")["State"] as! [String: Any]
      enter(text, name: "Renamed " + label)
      assert(
        store.projectedWorkspaces.first { $0.id == id }?.name == "Renamed " + label,
        "Sidebar rename must be immediate")
      let committedCell =
        outline.view(atColumn: 0, row: outline.row(forItem: item), makeIfNecessary: true)
        as! WorkspaceCell
      assert(
        committedCell.nameField.stringValue == "Renamed " + label,
        "The old label must not reappear after the editor closes")
      store.apply(beforeRename)
      assert(store.projectedWorkspaces.first { $0.id == id }?.name == "Renamed " + label)
      try wait("sidebar rename acknowledgement") { store.pendingRenames[id] == nil }
      store.apply(beforeRename)
      assert(store.projectedWorkspaces.first { $0.id == id }?.name == "Renamed " + label)
      try wait("sidebar rename commit") {
        store.projectedWorkspaces.first { $0.id == id }?.name == "Renamed " + label
      }
    }
    print(
      "Inline rename passed: tab/workspace/worktree double-click, context actions, focus retention, Escape and Enter."
    )
  }

  static func checkMiddleClick() {
    let tab = TabDragView(frame: NSRect(x: 0, y: 0, width: 120, height: 28))
    var closes = 0
    var selects = 0
    tab.close = { closes += 1 }
    tab.click = { selects += 1 }
    func event(_ type: CGEventType, x: CGFloat = 40) -> NSEvent {
      let cg = CGEvent(
        mouseEventSource: nil, mouseType: type,
        mouseCursorPosition: CGPoint(x: x, y: (NSScreen.screens.first?.frame.maxY ?? 0) - 14),
        mouseButton: .center)!
      return NSEvent(cgEvent: cg)!
    }
    tab.otherMouseDown(with: event(.otherMouseDown))
    assert(closes == 0)
    tab.otherMouseUp(with: event(.otherMouseUp))
    assert(closes == 1 && selects == 0, "Middle click closes without switching tabs")
    tab.otherMouseUp(with: event(.otherMouseUp))
    assert(closes == 1, "A release alone must not close a tab")
    tab.otherMouseDown(with: event(.otherMouseDown))
    tab.otherMouseUp(with: event(.otherMouseUp, x: 150))
    assert(closes == 1, "Dragging away cancels the close")
    print("Middle-click tab close passed: close on release, no selection, cancel outside.")
  }

  static func checkCoordinatorControls() throws {
    func views(_ root: NSView) -> [NSView] { [root] + root.subviews.flatMap(views) }
    let store = Store()
    let regular = Tab(id: "regular", name: "Tab 1", root: .leaf("preview-a"))
    let other = Tab(id: "other", name: "Tab 2", root: .leaf("preview-b"))
    assert(CoordinatorMark.image.isTemplate)
    let coordinator = WorkspaceCoordinator(
      id: "agent", pane: "preview-coordinator", provider: "codex")
    let workspace = Workspace(
      id: "workspace", name: "Fixture", path: "/tmp", tabs: [regular, other],
      coordinator: coordinator)
    let worktree = Workspace(
      id: "worktree", name: "Feature", path: "/tmp/feature", tabs: [],
      coordinator: WorkspaceCoordinator(
        id: "child-agent", pane: "preview-child", provider: "claude"), parent: workspace.id)
    store.panes[coordinator.pane] = Pane(
      id: coordinator.pane, title: "Codex", cwd: "/tmp", status: "blocked", exited: false,
      agentProvider: "codex", agentRole: "coordinator")
    store.panes["preview-child"] = Pane(
      id: "preview-child", title: "Claude", cwd: "/tmp/feature", status: "working", exited: false,
      agentProvider: "claude", agentRole: "coordinator")
    store.workspaces = [workspace, worktree]
    store.setTint("#E46B6B", for: workspace.id)
    store.canMoveTabs = true
    store.capabilitiesByEndpoint[""] = ["coordination_v1", "move_tab"]
    store.availableAgents[""] = ["claude", "codex"]
    store.select(workspace.id, tab: other.id)
    let window = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 1100, height: 700),
      styleMask: [.titled, .resizable], backing: .buffered, defer: false)
    window.contentView = NSHostingView(rootView: ContentView().environmentObject(store))
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    try wait("coordinator sidebar children") {
      views(window.contentView!).compactMap { $0 as? WorkspaceOutline }.first?.numberOfRows == 4
    }
    let outline = Self.sidebar(in: window)
    let sidebar = outline.coordinator!
    let item = sidebar.items[":" + workspace.id]!
    let child = sidebar.items["coordinator:" + workspace.id]!
    let nested = sidebar.items["coordinator:" + worktree.id]!
    assert(sidebar.parent(of: child) === item)
    assert(sidebar.parent(of: nested)?.workspace?.id == worktree.id)
    assert(sidebar.status(for: child) == "blocked" && sidebar.status(for: nested) == "working")
    assert(
      store.activityStatus(for: workspace) == .unknown,
      "Workspace status must only aggregate its own tabs")
    assert(
      store.activityStatus(for: worktree) == .unknown,
      "A worktree coordinator does not fill the worktree indicator")
    assert(sidebar.menu(item).item(withTitle: "Add Coordinator")?.isEnabled == false)
    assert(sidebar.menu(item).item(withTitle: "Add Coordinator")?.image == nil)
    assert(sidebar.menu(item).item(withTitle: "Open Coordinator") == nil)
    assert(sidebar.menu(item).item(withTitle: "Tasks and Agents…") == nil)
    assert(sidebar.menu(child).item(withTitle: "Close Coordinator") != nil)
    func coordinatorRow() -> WorkspaceRow {
      outline.rowView(atRow: outline.row(forItem: child), makeIfNecessary: true) as! WorkspaceRow
    }
    assert(
      coordinatorRow().tint == nil,
      "Untinted coordinator must not inherit the workspace's row highlight")
    assert(WorkspaceTint.hex(coordinatorRow().groupTint!) == "#E46B6B")
    let tintChoices = sidebar.menu(child).item(withTitle: "Tint")!.submenu!
    (tintChoices.item(withTitle: "Blue") as! SidebarMenuItem).callback()
    sidebar.update(outline)
    assert(store.workspaceTints[coordinator.id] == "#6EA1E6")
    assert(store.workspaceTints[workspace.id] == "#E46B6B")
    assert(WorkspaceTint.hex(coordinatorRow().tint!) == "#6EA1E6")
    (tintChoices.item(withTitle: "Default") as! SidebarMenuItem).callback()
    sidebar.update(outline)
    assert(coordinatorRow().tint == nil)
    assert(sidebar.menu(child).item(withTitle: "Rename") == nil)
    assert(sidebar.dragSession(child, grabY: 0, outline: outline) == nil)
    outline.selectRowIndexes(
      IndexSet(integer: outline.row(forItem: child)), byExtendingSelection: false)
    try wait("dedicated coordinator surface") {
      store.surface == .coordinator && store.focus == coordinator.pane
        && views(window.contentView!).allSatisfy { !($0 is TabDragView) }
    }
    assert(store.currentTab == nil && store.visiblePaneIDs == [coordinator.pane])
    assert(store.tab == other.id && store.selections[workspace.id]?.0 == other.id)
    assert(!store.canSplit(false))
    sidebar.update(outline)
    assert(outline.item(atRow: outline.selectedRow) as? WorkspaceSidebar.Item === child)
    let cell =
      outline.view(atColumn: 0, row: outline.row(forItem: child), makeIfNecessary: true)
      as! WorkspaceCell
    assert(
      cell.nameField.stringValue == "Coordinator",
      "Coordinator row name: \(cell.nameField.stringValue)")
    assert(
      sidebar.outlineView(outline, heightOfRowByItem: child)
        == SidebarGeometry.rowHeight(hasBranch: false, isMachine: false),
      "Coordinator must use a single-line row")
    // Ordinary tabs and their selection are independent from the coordinator.
    store.select(workspace.id)
    try wait("normal tab strip returns") {
      views(window.contentView!).contains { $0 is TabDragView }
    }
    assert(store.currentTab?.id == other.id && store.focus == "preview-b")
    assert(store.beginTabDrag(regular.id))
    store.previewTabMove(to: 1)
    assert(store.projectedWorkspaces[0].tabs.last?.id == regular.id)
    assert(store.workspaces[0].tabs.first?.id == regular.id)
    store.cancelTabDrag()
    store.openCoordinator(worktree.id)
    assert(store.surface == .coordinator && store.visiblePaneIDs == ["preview-child"])
    // Closing/removing the coordinator falls back to the owner's normal view.
    let entry: [String: Any] = [
      "id": worktree.id, "name": "Feature", "path": worktree.path, "parent": workspace.id,
      "tabs": [],
    ]
    store.apply(["workspace_model": 1, "workspaces": [entry], "panes": []])
    assert(store.surface == .terminals && store.currentTab == nil)
    var plain = workspace
    plain.coordinator = nil
    item.workspace = plain
    assert(sidebar.menu(item).item(withTitle: "Add Coordinator")?.isEnabled == true)
    print(
      "Coordinator surfaces passed: workspace/worktree children, attention status, no tabs, remembered terminal selection, close fallback, duplicate prevention."
    )
  }

  static func checkCoordinatorStateMigration() {
    for endpoint in ["", "fixture-remote"] {
      let store = Store()
      let prefix = endpoint.isEmpty ? "" : endpoint + "|"
      let workspace = prefix + "workspace"
      let pane = prefix + "coordinator-pane"
      let tab = prefix + "terminal-tab"
      let shell = prefix + "shell-pane"
      store.capabilitiesByEndpoint[endpoint] = []
      store.remoteMessages[endpoint] = "Connected"
      let state: [String: Any] = [
        "workspace_model": 1,
        "workspaces": [
          [
            "id": workspace, "name": "Fixture", "path": "/tmp",
            "coordinator": ["id": prefix + "agent", "pane": pane, "provider": "codex"],
            "tabs": [
              [
                "id": prefix + "legacy-tab", "name": "Coordinator", "agent_role": "coordinator",
                "root": ["Leaf": pane],
              ],
              ["id": tab, "name": "Tab 1", "root": ["Leaf": shell]],
            ],
          ]
        ],
        "panes": [
          [
            "id": pane, "workspace": workspace, "agent_provider": "codex",
            "agent_role": "coordinator", "cwd": "/tmp",
          ], ["id": shell, "workspace": workspace, "cwd": "/tmp"],
        ],
      ]
      store.apply(state, endpoint: endpoint)
      store.select(workspace, tab: tab)
      store.openCoordinator(workspace)
      for _ in 0..<2 { store.apply(state, endpoint: endpoint) }
      assert(
        store.current?.tabs.map(\.id) == [tab], "Legacy tab must appear only as a Coordinator child"
      )
      assert(store.surface == .coordinator && store.visiblePaneIDs == [pane])
      store.select(workspace)
      assert(store.currentTab?.id == tab && store.focus == shell)
    }
    print(
      "Coordinator state migration passed: legacy local/remote tab filtering, stable polls, and ordinary tab selection."
    )
  }

  static func checkSettings(store: Store) throws {
    let previousTheme = store.theme
    store.theme = .warm
    assert(Store().theme == .warm, "Theme selection should survive reopening the app")
    store.theme = previousTheme
    let dark = NSAppearance(named: .darkAqua)!
    dark.performAsCurrentDrawingAppearance {
      let border = JamTheme.midnight.paneBorder(focused: false).usingColorSpace(.sRGB)!
      assert(border.blueComponent > border.redComponent)
      assert(JamTheme.midnight.background != JamTheme.midnight.terminalBackground)
    }
    let configPath = ProcessInfo.processInfo.environment["XDG_CONFIG_HOME"]! + "/jam/config.toml"
    defer { try? FileManager.default.removeItem(atPath: configPath) }
    let draft = SettingsDraft()
    try wait("settings load") { draft.loaded }
    assert(draft.config.maxAgentsPerCoordinator == 8)
    assert(
      WorktreePathPreview(
        template: "~/trees/{{worktree}}/{{repo}}", repo: "jam", worktree: "feature/login",
        home: "/home/test"
      ).path == "/home/test/trees/feature-login/jam")
    assert(WorktreePathPreview(template: "~/{{unknown}}").error != nil)
    let original = draft.config
    try FileManager.default.createDirectory(
      atPath: (configPath as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
    try "max_agents_per_coordinator = 3\n".write(
      toFile: configPath, atomically: true, encoding: .utf8)
    try wait("external settings change") { draft.conflict }
    var saved = false
    draft.save { saved = true }
    assert(!saved && !draft.saving, "Conflicted settings must not save")
    let reopened = SettingsDraft()
    try wait("reopened settings") { reopened.loaded }
    assert(reopened.config.maxAgentsPerCoordinator == 3)
    reopened.config = original
    reopened.config.agentModels["claude"] = AgentModelSettings(
      coordinator: "coordinator-test", worker: "worker-test", reviewer: "reviewer-test")
    reopened.save { saved = true }
    try wait("settings save") { saved }
    let persisted = try JamConfig.load()
    assert(persisted.maxAgentsPerCoordinator == 8)
    assert(persisted.agentModels["claude"]?.reviewer == "reviewer-test")
    let modelsWindow = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 860, height: 600),
      styleMask: [.titled], backing: .buffered, defer: false)
    let modelsView = NSHostingView(
      rootView: AppSettingsView(store: store, initialSection: "Coordination", close: {}))
    modelsWindow.contentView = modelsView
    modelsWindow.appearance = NSAppearance(named: .darkAqua)
    modelsWindow.orderFront(nil)
    func modelFields(_ view: NSView) -> [NSTextField] {
      (view as? NSTextField).map {
        $0.placeholderString?.hasPrefix("Jam default") == true ? [$0] : []
      }
        ?? view.subviews.flatMap(modelFields)
    }
    try wait("coordination model fields") {
      modelFields(modelsView).count == 4 && modelFields(modelsView).allSatisfy(\.isEnabled)
    }
    modelsView.layoutSubtreeIfNeeded()
    for field in modelFields(modelsView) {
      assert(field.bounds.width >= 180, "Model fields need enough room for full IDs")
    }
    if let bitmap = modelsView.bitmapImageRepForCachingDisplay(in: modelsView.bounds) {
      modelsView.cacheDisplay(in: modelsView.bounds, to: bitmap)
      try bitmap.representation(using: .png, properties: [:])?.write(
        to: URL(fileURLWithPath: "/tmp/jam-coordination-settings.png"))
    }
    modelsWindow.orderOut(nil)
    print("Coordination settings passed: separate section and four readable model fields.")
    let parent = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 1000, height: 750),
      styleMask: [.titled, .resizable], backing: .buffered, defer: false)
    parent.contentView = NSHostingView(rootView: SettingsSheetFixture(store: store))
    parent.orderFront(nil)
    defer { parent.orderOut(nil) }
    store.openSettings()
    try wait("settings sheet presentation") { parent.attachedSheet != nil }
    let sheet = parent.attachedSheet!
    assert(sheet.sheetParent === parent, "Settings must be modal to its Jam window")
    assert(store.showSettings && TerminalInputGate.blocked, "Settings must block terminal input")
    if let view = sheet.contentView {
      view.layoutSubtreeIfNeeded()
      RunLoop.current.run(until: Date().addingTimeInterval(0.1))
      if let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) {
        view.cacheDisplay(in: view.bounds, to: bitmap)
        try bitmap.representation(using: .png, properties: [:])?.write(
          to: URL(fileURLWithPath: "/tmp/jam-settings-preview.png"))
      }
    }
    store.showSettings = false
    try wait("settings sheet dismissal") { parent.attachedSheet == nil }

  }
  static func checkUsagePanel(store: Store) throws {
    let oldUsage = store.agentUsage
    let oldConnected = store.connected
    defer {
      store.agentUsage = oldUsage
      store.connected = oldConnected
    }
    store.connected = true
    let reports: [[String: Any]] = ["codex", "claude"].map { provider in
      [
        "provider": provider, "profile_key": provider, "account_key": provider + "-account",
        "account_label": "test@example.invalid", "active_sessions": 2, "checking": false,
        "last_success_at": Int(Date().timeIntervalSince1970),
        "windows": [
          ["key": "five_hour", "label": "5h", "percent": 37],
          ["key": "weekly", "label": "7d", "percent": 68],
        ],
      ]
    }
    let data = try JSONSerialization.data(withJSONObject: reports)
    store.agentUsage = ["": try JSONDecoder().decode([AgentUsageReport].self, from: data)]
    let window = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 240, height: 260),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.appearance = NSAppearance(named: .darkAqua)
    let host = NSHostingView(rootView: UsagePanel().environmentObject(store))
    window.contentView = host
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    try wait("usage panel layout") { host.fittingSize.height > 80 }
    assert(host.fittingSize.height < 240, "Usage panel is not compact")
    func containsScrollView(_ view: NSView) -> Bool {
      view is NSScrollView || view.subviews.contains(where: containsScrollView)
    }
    assert(!containsScrollView(host), "Usage should fit its content without a scroll view")
    if let path = ProcessInfo.processInfo.environment["JAM_UI_SCREENSHOT"],
      let bitmap = host.bitmapImageRepForCachingDisplay(in: host.bounds)
    {
      host.cacheDisplay(in: host.bounds, to: bitmap)
      try bitmap.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
    }
    let unavailable: [String: Any] = [
      "provider": "claude", "profile_key": "keychain", "active_sessions": 1,
      "windows": [], "checking": false,
      "error": "Claude login is missing or Keychain access is unavailable",
    ]
    store.agentUsage = [
      "": [
        try JSONDecoder().decode(
          AgentUsageReport.self,
          from: JSONSerialization.data(withJSONObject: unavailable))
      ]
    ]
    try wait("usage permission action fits") { host.fittingSize.height >= 100 }
    assert(host.fittingSize.height < 180, "The permission prompt should remain compact")
    store.agentUsage = [:]
    try wait("unused providers hidden") { host.fittingSize.height < 1 }
    assert(
      (rpc("AgentUsage")["AgentUsage"] as? [Any])?.isEmpty == true,
      "Private test shells should not query any provider credentials")
    print(
      "Usage panel passed: compact bars, hidden unused providers, and nonblocking empty-agent snapshot."
    )
  }

  static func checkWorktreeNavigation() throws {
    let state = WorktreeListFixtureState()
    let window = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 560, height: 300),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.contentView = NSHostingView(rootView: WorktreeListFixture(state: state))
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    func find(_ view: NSView) -> WorktreeTable? {
      if let table = view as? WorktreeTable { return table }
      return view.subviews.lazy.compactMap(find).first
    }
    try wait("worktree table") { find(window.contentView!)?.selectedRow == 1 }
    let table = find(window.contentView!)!
    let coordinator = table.coordinator!
    let reloads = coordinator.reloadCount
    assert(table.numberOfRows == 1000 && table.bounds.width > 500)
    let row = table.view(atColumn: 0, row: 1, makeIfNecessary: true)!
    let visible = table.visibleRect
    coordinator.move(by: 1)
    assert(table.selectedRow == 3 && state.selection == "/remote/worktree/3")
    coordinator.move(by: -1)
    assert(table.selectedRow == 1 && table.visibleRect == visible)
    RunLoop.current.run(until: Date().addingTimeInterval(0.03))
    assert(table.view(atColumn: 0, row: 1, makeIfNecessary: true) === row)
    var durations: [Double] = []
    for _ in 0..<100 {
      let start = Date()
      coordinator.move(by: 1)
      table.layoutSubtreeIfNeeded()
      durations.append(Date().timeIntervalSince(start) * 1000)
    }
    RunLoop.current.run(until: Date().addingTimeInterval(0.03))
    assert(coordinator.reloadCount == reloads, "Selection rebuilt the checkout list")
    assert(table.visibleRect.intersects(table.rect(ofRow: table.selectedRow)))
    let scroll = table.visibleRect
    coordinator.update(checkouts: state.checkouts, opened: state.opened, selection: state.selection)
    assert(coordinator.reloadCount == reloads && table.visibleRect == scroll)
    coordinator.openSelected()
    assert(state.opens == 1)
    state.selection = "/remote/worktree/150"
    state.checkouts = Array(state.checkouts[150..<182])
    try wait("filtered worktree table") { table.numberOfRows == 32 && table.selectedRow == 0 }
    assert(coordinator.reloadCount == reloads + 1)
    durations.sort()
    print(
      "Native worktree navigation: median \(durations[durations.count / 2]) ms, p95 \(durations[95]) ms; 1,000 rows, no selection reloads."
    )
  }

  static func checkReviewBadge(store: Store, window: NSWindow) throws {
    let workspace = store.workspaces.first!.id
    let outline = sidebar(in: window)
    func button() -> NSButton? {
      guard let item = outline.coordinator!.items[":" + workspace] else { return nil }
      let row = outline.row(forItem: item)
      guard row >= 0,
        let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? WorkspaceCell
      else { return nil }
      return cell.titleRow.subviews.compactMap { $0 as? NSButton }.first
    }
    for provider in ["github", "gitlab"] {
      let review = ReviewRequest(
        provider: provider, number: 42, url: "https://example.com/review/42", title: "Review title",
        draft: true)
      store.gitSummaries[workspace] = GitSummary(added: 20000, removed: 4000, review: review)
      try wait("\(provider) review badge updates") { button()?.title == review.label }
      let badge = button()!
      assert(!badge.isHidden && badge.toolTip == review.tooltip)
      let point = outline.superview!.convert(
        NSPoint(x: badge.bounds.midX, y: badge.bounds.midY), from: badge)
      assert(
        outline.hitTest(point) === badge,
        "Review badge must receive clicks instead of starting a row drag")
    }
    store.gitSummaries.removeValue(forKey: workspace)
    try wait("review badge disappears") { button() == nil }
    print(
      "Review badges passed: GitHub/GitLab labels, row updates, tooltips, click targets, and removal."
    )
  }

  static func checkToastTimeouts() throws {
    var dismissed: [String] = []
    func toast(_ message: String) -> WarningToast {
      WarningToast(
        message: message, retry: nil, dismiss: { dismissed.append(message) },
        duration: .milliseconds(500))
    }
    let window = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 400, height: 180),
      styleMask: [.titled], backing: .buffered, defer: false)
    let host = NSHostingView(rootView: toast("First warning"))
    window.contentView = host
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    RunLoop.current.run(until: Date().addingTimeInterval(0.30))
    host.rootView = toast("Second warning")
    RunLoop.current.run(until: Date().addingTimeInterval(0.30))
    assert(dismissed.isEmpty, "An earlier timer dismissed a newer warning")
    try wait("warning automatically dismissed") { dismissed == ["Second warning"] }

    var copied: QuickToastMessage? = QuickToastMessage(text: "Copied to clipboard")
    let quick = QuickToast(message: Binding(get: { copied }, set: { copied = $0 }))
    window.contentView = NSHostingView(rootView: quick)
    try wait("quick toast automatically dismissed") { copied == nil }

    var upgradeDismissed = false
    let upgrade = UpgradeNoticeToast(
      message: "Jam was upgraded to 0.0.9.", openReleaseNotes: {},
      dismiss: { upgradeDismissed = true }, duration: .milliseconds(500))
    window.contentView = NSHostingView(rootView: upgrade)
    try wait("upgrade success toast automatically dismissed") { upgradeDismissed }

    let interrupted = AppUpgradeRecovery(
      journal: "interrupted", version: "0.0.9", phase: "started", error: nil)
    let controller = AppUpgradeController { _ in Data("{}".utf8) }
    controller.pendingRecoveries = [interrupted]
    let recovery = UpgradeRecoveryToast(
      recovery: interrupted, later: { controller.showRecoveryNotice = false },
      recover: { controller.recovery = interrupted }, duration: .milliseconds(500))
    window.contentView = NSHostingView(rootView: recovery)
    try wait("upgrade recovery toast automatically dismissed") { !controller.showRecoveryNotice }
    assert(controller.pendingRecoveries.first?.id == interrupted.id && controller.recovery == nil)
  }

  static func checkUpgradeChip(store: Store) throws {
    let release = AppRelease(
      schema: 1, version: "9.0.0", protocol: "jam/test", minMacOS: "14.0", assets: [])
    JamDefaults.shared.removeObject(forKey: "appUpgradeState")
    let controller = AppUpgradeController { request in
      assert(request["action"] as? String == "check")
      return try JSONEncoder().encode(
        AppUpgradeCheck(current: "0.0.9", available: true, release: release))
    }
    controller.managed = true
    let previousError = store.error
    defer {
      store.error = previousError
      JamDefaults.shared.removeObject(forKey: "appUpgradeState")
    }
    let serverWarning = "incompatible jam daemon; update the server from Machines"
    store.error = ""
    let window = NSWindow(
      contentRect: NSRect(x: -3000, y: -3000, width: 700, height: 600),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.appearance = NSAppearance(named: .darkAqua)
    var toolbarWidth: CGFloat = 0
    var overlayHeight: CGFloat = 0
    let anchorWidth: CGFloat = 1
    let host = NSHostingView(
      rootView: VStack(alignment: .trailing) {
        HStack(spacing: 0) {
          Color.clear.frame(width: anchorWidth, height: 24)
          UpgradeAvailableChip(controller: controller)
        }
        .fixedSize()
        .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { toolbarWidth = $0 }
        Spacer()
        AppStatusOverlay(store: store, controller: controller)
          .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { overlayHeight = $0 }
      }.padding(8).background(Color(nsColor: .windowBackgroundColor)))
    window.contentView = host
    window.orderFront(nil)
    defer { window.orderOut(nil) }
    try wait("upgrade toolbar laid out with no chip") {
      toolbarWidth == anchorWidth && overlayHeight > 0
    }
    let emptyHeight = overlayHeight
    store.error = serverWarning
    try wait("server warning occupies the overlay") { overlayHeight > emptyHeight + 30 }
    let warningHeight = overlayHeight
    controller.check()
    try wait("manual check finds update") { controller.available?.version == release.version }
    try wait("upgrade chip occupies space in the toolbar") { toolbarWidth - anchorWidth > 60 }
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    assert(
      overlayHeight == warningHeight && store.error == serverWarning,
      "The toolbar chip must leave the server warning visible on its own")
    controller.managed = false
    controller.notice = "Jam 9.0.0 is available"
    store.error = serverWarning
    try wait("unmanaged builds leave the toolbar empty") {
      toolbarWidth == anchorWidth
    }
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    assert(
      overlayHeight == warningHeight && store.error == serverWarning,
      "The server warning must also stay visible on its own in development builds")
    print(
      "Manual update checks show the upgrade chip alongside errors in release builds."
    )
  }

  static func checkUpdateSettingsControls(store: Store) throws {
    JamDefaults.shared.removeObject(forKey: "appUpgradeState")
    let previousSettings = store.showSettings
    defer {
      store.showSettings = previousSettings
      JamDefaults.shared.removeObject(forKey: "appUpgradeState")
    }
    let release = AppRelease(
      schema: 1, version: "9.0.0", protocol: "jam/test", minMacOS: "14.0", assets: [])
    for managed in [false, true] {
      JamDefaults.shared.removeObject(forKey: "appUpgradeState")
      let controller = AppUpgradeController { request in
        switch request["action"] as? String {
        case "eligibility":
          if !managed { throw NSError(domain: "Development build", code: 1) }
          return Data("{}".utf8)
        case "pending": return Data("[]".utf8)
        case "check":
          return try JSONEncoder().encode(
            AppUpgradeCheck(current: "0.0.3", available: true, release: release))
        case "stage":
          return try JSONEncoder().encode(
            AppUpgradeStage(
              staged: .init(
                directory: "/tmp/fixture", release: release, archive: "/tmp/fixture.zip"),
              plan: .init(
                installation: "/tmp/Jam.app", localRunning: false, targets: [], offline: [],
                appPIDs: [])))
        default: throw NSError(domain: "Unexpected settings updater action", code: 1)
        }
      }
      controller.configure(automatic: false)
      assert(!controller.canPrepareUpdate, "Upgrade was enabled before checking")
      controller.check()
      try wait("settings update check") {
        controller.settingsRelease == release && controller.managed == managed
      }
      assert(controller.canPrepareUpdate == managed)
      controller.requiredMinimum = "10.0.0"
      assert(controller.settingsRelease == nil && !controller.canPrepareUpdate)
      controller.requiredMinimum = nil
      assert(
        controller.notice.isEmpty, "The available version must not become an explanatory notice")
      if !managed {
        assert(controller.available == nil, "Development builds must not show the upgrade chip")
        controller.prepareFromSettings()
        assert(!controller.showSheet, "Development builds must not start an upgrade")
        continue
      }
      controller.skip()
      assert(controller.available == nil && controller.canPrepareUpdate)
      assert(
        controller.settingsRelease == release,
        "Settings must allow explicitly upgrading a skipped release")
      let parent = NSWindow(
        contentRect: NSRect(x: -3000, y: -3000, width: 800, height: 600),
        styleMask: [.titled], backing: .buffered, defer: false)
      let settings = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 700, height: 400),
        styleMask: [.titled], backing: .buffered, defer: false)
      store.showSettings = true
      parent.contentView = NSHostingView(
        rootView: AppStatusOverlay(store: store, controller: controller))
      settings.contentView = NSHostingView(
        rootView: AppUpdateSettings(
          store: store, controller: controller, automatic: .constant(true)))
      parent.orderFront(nil)
      parent.beginSheet(settings)
      defer {
        parent.endSheet(settings)
        settings.orderOut(nil)
        parent.orderOut(nil)
      }
      controller.prepareFromSettings()
      try wait("upgrade confirmation inside settings") {
        controller.phase == .ready && settings.attachedSheet != nil
      }
      assert(settings.attachedSheet?.sheetParent === settings)
      controller.cancel()
      try wait("cancel settings upgrade") { settings.attachedSheet == nil }
      assert(parent.attachedSheet === settings, "Cancelling upgrade dismissed the settings draft")
    }
    print(
      "Settings upgrade passed: disabled for development, enabled after release check, nested confirmation preserves Settings on Cancel."
    )
  }

  static func checkServerRecovery(
    store: Store, destinations: [(String, String, [String])], window: NSWindow
  ) throws {
    let (workspace, tab, visible) = destinations[0]
    store.select(workspace, tab: tab)
    let host = rpc("HostInfo")["HostInfo"] as! [String: Any]
    store.observeServer(host, endpoint: "")
    apply(rpc("State"), to: store)
    let oldIncarnation = host["incarnation"] as! String
    let marker = "jam-recovery-must-not-replay"
    _ = rpc(["Send": ["pane": visible[0], "data": Array("printf '\(marker)\\n'\n".utf8)]])
    try wait {
      (rpc(["Capture": ["pane": visible[0]]])["Capture"] as? String ?? "").contains(marker)
    }

    // Restart only this fixture's daemon. Its socket and state live in a private
    // temporary directory, never in the user's running Jam instance.
    // Exercise the same Store action as the Machines confirmation, including
    // recovery when an incompatible initial connection never became ready.
    store.ready = false
    store.connected = false
    store.connectionError = "incompatible jam daemon; update the server from Machines"
    store.error = store.connectionError
    store.paneErrors[visible[0]] = "Reconnecting to terminal…"
    store.performServerAction("", kind: .restart, force: true, interactive: false)
    try wait("confirmed machine restart") { !store.serverBusy.contains("") }
    assert(store.ready && store.connected, "A verified update left startup blocked")
    try wait("obsolete connection warning clears") { store.error.isEmpty }
    let newHost = rpc("HostInfo")["HostInfo"] as! [String: Any]
    assert(newHost["incarnation"] as! String != oldIncarnation)
    try wait("visible panes recover after server restart") {
      visible.allSatisfy { store.panes[$0]?.exited == false && !store.restartingPanes.contains($0) }
    }
    let hidden = destinations.dropFirst().flatMap { $0.2 }
    assert(hidden.allSatisfy { store.panes[$0]?.exited == true }, "Hidden tabs started eagerly")
    try wait("terminal mirrors reattach") {
      visible.allSatisfy { id in
        guard let view = store.terminalViews.existing(id) else { return false }
        return view.window === window && view.meta["ready"] as? Bool == true
          && store.paneErrors[id] == nil && matchesGeometry(view)
          && !(view.accessibilityValue() as? String ?? "").contains(marker)
      }
    }
    // Wait for the fresh interactive shell to finish its terminal setup before
    // sending input; its startup can flush keystrokes queued before the prompt.
    try wait("fresh shell prompt") {
      let text = rpc(["Capture": ["pane": visible[0]]])["Capture"] as? String ?? ""
      return text.contains("jam-test>")
    }
    // Exiting a shell on the same daemon must stay stopped after a routine poll
    // or transport reconnect; only a changed incarnation triggers recovery.
    let sent = rpc(["Send": ["pane": visible[0], "data": Array("exit\n".utf8)]])
    assert(sent["error"] == nil, "First input after recovery failed: \(sent)")
    try wait("shell exits") {
      apply(rpc("State"), to: store)
      return store.panes[visible[0]]?.exited == true
    }
    store.observeServer(newHost, endpoint: "")
    apply(rpc("State"), to: store)
    assert(!store.restartingPanes.contains(visible[0]))
    RunLoop.current.run(until: Date().addingTimeInterval(0.15))
    assert(store.panes[visible[0]]?.exited == true)
    assert((store.paneSizes[visible[0]]?.height ?? 0) > 500, "Stopped pane collapsed vertically")
    print(
      "Server recovery passed: visible shells restart, hidden tabs wait, mirrors reattach, intentional exits stay stopped."
    )
  }

  static func checkReconnectingMachine(store: Store, window: NSWindow) {
    let remote = "empty-test"
    store.workspaces.append(
      Workspace(id: remote + "|cached", name: "Cached", path: "/cached", tabs: []))
    store.loadedRemotes.insert(remote)
    store.remoteMessages[remote] = "Connection timed out"
    store.remoteConnecting.insert(remote)
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    let outline = sidebar(in: window)
    let machine = outline.coordinator!.items["machine:" + remote]!
    let row = outline.row(forItem: machine)
    let count = outline.numberOfRows
    let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as! WorkspaceCell
    assert(
      cell.titleRow.subviews.compactMap { ($0 as? NSTextField)?.stringValue }.contains(
        "Reconnecting…"))
    let connecting = cell.titleRow.subviews.compactMap { $0 as? NSTextField }.first {
      $0.stringValue == "Reconnecting…"
    }!
    assert(
      abs(connecting.frame.maxX - (cell.titleRow.bounds.maxX - 8)) < 1,
      "Connection label should be trailing-aligned")
    assert(machine.connectionAttempt && !machine.children.isEmpty)
    assert(outline.frameOfOutlineCell(atRow: row).isEmpty)
    let rowView = outline.rowView(atRow: row, makeIfNecessary: true)!
    assert(rowView.subviews.contains { $0 is NSProgressIndicator && !$0.isHidden })
    store.remoteConnecting.remove(remote)
    store.pausedRemotes.insert(remote)
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    let offlineCell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as! WorkspaceCell
    assert(
      offlineCell.titleRow.subviews.compactMap { ($0 as? NSTextField)?.stringValue }.contains(
        "Offline"))
    assert(!machine.connectionAttempt && machine.offline)
    let selected = store.selected
    let selection = outline.selectedRowIndexes
    let point = NSPoint(x: outline.rect(ofRow: row).midX, y: outline.rect(ofRow: row).midY)
    let click = NSEvent.mouseEvent(
      with: .leftMouseDown, location: outline.convert(point, to: nil), modifierFlags: [],
      timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
      context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
    let generation = store.remoteConnectionGenerations[remote, default: 0]
    outline.mouseDown(with: click)
    assert(
      store.remoteConnecting.contains(remote),
      "Clicking an offline machine must immediately reconnect")
    assert(store.remoteConnectionGenerations[remote, default: 0] == generation + 1)
    assert(
      store.selected == selected && outline.selectedRowIndexes == selection,
      "A reconnect click must not select the machine")
    outline.mouseDown(with: click)
    assert(
      store.remoteConnectionGenerations[remote, default: 0] == generation + 1,
      "Repeated clicks must not start duplicate attempts")
    // Invalidate this fake remote's asynchronous completion before continuing.
    store.remoteConnectionGenerations[remote, default: 0] &+= 1
    store.remoteConnecting.remove(remote)
    store.remoteLoading.remove(remote)
    store.remoteMessages[remote] = "Connected"
    store.remoteLoading.insert(remote)  // A healthy background poll is not a reconnect.
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    assert(outline.numberOfRows == count && !machine.connectionAttempt && !machine.offline)
    assert(!outline.frameOfOutlineCell(atRow: row).isEmpty)
    assert(!rowView.subviews.contains { $0 is NSProgressIndicator && !$0.isHidden })
    store.remoteLoading.remove(remote)
    store.workspaces.removeAll { $0.id == remote + "|cached" }
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    print(
      "Reconnect UI passed: Connecting/Offline labels, active spinner, stable cached workspaces, and quiet healthy polls."
    )
  }

  static func checkSidebarAlignment(window: NSWindow, store: Store) {
    let id = "alignment-workspace"
    let other = "alignment-other"
    func apply(branch: Bool?) {
      var workspace: [String: Any] = [
        "id": id, "name": "Project", "path": "/repo", "tabs": [], "repository": "/repo/.git",
        "branch": "main",
      ]
      if let branch {
        workspace["context"] =
          branch
          ? ["path": "/repo", "ready": true, "repository": "/repo/.git", "branch": "main"]
          : ["path": "/repo", "ready": false]
      }
      store.selected = other
      store.apply(
        [
          "workspace_model": 1, "revision": 300,
          "workspaces": [["id": other, "name": "Other", "path": "/tmp", "tabs": []], workspace],
          "panes": [],
        ],
        endpoint: "")
      RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    }
    for branch in [nil, false, true, false, true] as [Bool?] {
      apply(branch: branch)
      let outline = sidebar(in: window)
      let item = outline.coordinator!.items[":" + id]!
      let row = outline.row(forItem: item)
      let rowView = outline.rowView(atRow: row, makeIfNecessary: false)!
      let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: false) as! WorkspaceCell
      let icon = cell.subviews.compactMap { $0 as? NSImageView }.first!
      let center = rowView.convert(icon.bounds, from: icon).midY
      print(
        "Alignment: branch=\(String(describing: branch)), row=\(rowView.bounds.height), cell=\(cell.bounds.height), icon=\(center), expected=\(rowView.bounds.midY)"
      )
      assert(
        abs(center - rowView.bounds.midY) <= 1, "Sidebar icon is not centered before selection")
      store.select(id)
      RunLoop.current.run(until: Date().addingTimeInterval(0.05))
      assert(
        abs(rowView.convert(icon.bounds, from: icon).midY - center) <= 1,
        "Selection moved sidebar contents")
    }
    print("Sidebar alignment remains stable when repository metadata arrives after restart.")
  }

  static func checkWorktreeMembership(window: NSWindow, store: Store) {
    let parent = "container-test"
    let other = "other-container-test"
    func workspace(_ id: String, _ repository: String?, _ owner: String?) -> [String: Any] {
      var row: [String: Any] = ["id": id, "name": id, "path": "/" + id, "tabs": []]
      if let repository {
        row["repository"] = repository + "/.git"
        row["main_path"] = repository
        row["branch"] = "feature"
      }
      if let owner { row["parent"] = owner }
      return row
    }
    var roots = [workspace(parent, nil, nil), workspace(other, nil, nil)]
    var children = [
      workspace("a-worktree", "/repo-a", parent),
      workspace("other-worktree", "/repo-a", other),
    ]
    func apply() {
      store.apply(
        ["workspace_model": 1, "revision": 200, "panes": [], "workspaces": roots + children],
        endpoint: "")
      RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    }
    apply()
    let outline = sidebar(in: window)
    let root = outline.coordinator!.items[":" + parent]!
    assert(root.children.count == 1 && root.children[0].workspace?.id == "a-worktree")
    assert(!root.children[0].repositoryGroup)
    children.insert(workspace("b-worktree", "/repo-b", parent), at: 1)
    apply()
    assert(root.children.count == 2 && root.children.allSatisfy(\.repositoryGroup))
    assert(
      Set(root.children.flatMap { $0.children.compactMap { $0.workspace?.id } }) == [
        "a-worktree", "b-worktree",
      ])
    let identities = root.children
    roots[0]["name"] = "Pinned"
    roots[0]["context"] = [
      "path": "/repo-a", "ready": true, "repository": "/repo-a/.git", "branch": "main",
    ]
    apply()
    roots[0]["context"] = [
      "path": "/repo-b", "ready": true, "repository": "/repo-b/.git", "branch": "main",
    ]
    apply()
    assert(outline.coordinator!.items[":" + parent] === root)
    assert(root.workspace?.context?.repository == "/repo-b/.git")
    assert(zip(root.children, identities).allSatisfy { $0 === $1 })
    roots[0]["name"] = "~"
    roots[0]["context"] = ["path": "/home/test", "ready": true]
    apply()
    assert(root.children.count == 2)
    assert(store.openedCheckoutPaths(in: parent).contains("/a-worktree"))
    assert(!store.openedCheckoutPaths(in: other).contains("/a-worktree"))
    for group in root.children {
      let row = outline.row(forItem: group)
      outline.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
      assert(outline.selectedRow != row, "Repository headings must not select a workspace")
    }
    children.removeAll { $0["id"] as? String == "b-worktree" }
    apply()
    assert(root.children.count == 1 && root.children[0].workspace?.id == "a-worktree")
    assert(!root.children[0].repositoryGroup)
    print(
      "Workspace membership UI passed: two repositories, stable groups through cd, and independent containers."
    )
  }

  static func checkEmptyGroups(store: Store, window: NSWindow) {
    store.remotes = [RemoteMachine(id: "empty-test", name: "Empty", destination: "unused")]
    store.loadedRemotes.insert("empty-test")
    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
    let outline = sidebar(in: window)
    let item = outline.coordinator!.items["machine:empty-test"]!
    func check(expandable: Bool) {
      let row = outline.row(forItem: item)
      assert(row >= 0 && outline.isExpandable(item) == expandable)
      assert(outline.frameOfOutlineCell(atRow: row).isEmpty == !expandable)
      assert(outline.disclosureHitRect(atRow: row).isEmpty == !expandable)
    }
    check(expandable: false)
    store.workspaces.append(
      Workspace(id: "empty-test|workspace", name: "Workspace", path: "/unused", tabs: []))
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    check(expandable: true)
    let row = outline.row(forItem: item)
    let icon = outline.frameOfOutlineCell(atRow: row)
    let hit = outline.disclosureHitRect(atRow: row)
    for point in [NSPoint(x: hit.minX + 1, y: hit.midY), NSPoint(x: icon.midX, y: icon.midY)] {
      assert(outline.hitTest(outline.convert(point, to: outline.superview)) === outline)
      let wasExpanded = outline.isItemExpanded(item)
      let event = NSEvent.mouseEvent(
        with: .leftMouseDown, location: outline.convert(point, to: nil), modifierFlags: [],
        timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0,
        clickCount: 1, pressure: 1)!
      outline.mouseDown(with: event)
      RunLoop.current.run(until: Date().addingTimeInterval(0.3))
      assert(outline.isItemExpanded(item) != wasExpanded)
    }
    store.workspaces.removeAll { $0.id == "empty-test|workspace" }
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    check(expandable: false)
    print("Native sidebar disclosure passed: empty, populated, and last workspace removed.")
  }

  static func sidebar(in window: NSWindow) -> WorkspaceOutline {
    func find(in view: NSView) -> WorkspaceOutline? {
      if let outline = view as? WorkspaceOutline { return outline }
      return view.subviews.lazy.compactMap { find(in: $0) }.first
    }
    return find(in: window.contentView!)!
  }

  static func checkWorkspaceHandoff(store: Store, window: NSWindow) {
    let remote = "empty-test"
    store.remoteMessages[remote] = "Connected"
    store.capabilitiesByEndpoint[remote] = []
    var creation = WorkspaceCreation(
      endpoint: remote, previous: store.selected, action: .new, name: "~", path: "/home/test")
    store.workspaceCreations[creation.id] = creation
    store.selected = creation.id
    store.tab = ""
    store.focus = ""
    RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    let outline = sidebar(in: window)
    let pending = outline.coordinator!.items[remote + ":" + creation.id]!
    let row = outline.row(forItem: pending)
    let rect = outline.rect(ofRow: row)
    let count = outline.numberOfRows
    let pollEpoch = store.workspaceCreationEpochs[remote, default: 0]
    let created = creation.proposedID
    let state: [String: Any] = [
      "revision": 100,
      "workspaces": [["id": created, "name": "~", "path": "/home/test", "tabs": []]],
      "panes": [],
    ]
    // A fast poll must not insert the real row alongside the pending one.
    assert(store.applyRefresh(state, endpoint: remote, creationEpoch: pollEpoch))
    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
    assert(!store.displayedWorkspaces.contains { $0.id == created })
    assert(outline.numberOfRows == count && outline.row(forItem: pending) == row)

    creation.result = created
    store.finishWorkspaceCreation(creation, state: state)
    let deadline = Date().addingTimeInterval(0.35)
    repeat {
      RunLoop.current.run(until: Date().addingTimeInterval(0.002))
      assert(outline.numberOfRows == count, "Workspace handoff removed or duplicated a row")
      assert(outline.row(forItem: pending) == row && outline.rect(ofRow: row) == rect)
    } while Date() < deadline
    assert(outline.coordinator!.items[remote + ":" + created] === pending)
    assert(pending.workspace?.id == created && store.selected == created)
    assert(!store.isCreatingWorkspace(created))
    assert(store.displayedWorkspaces.filter { $0.id == created }.count == 1)
    // An older poll arriving after completion must not remove the new workspace.
    assert(
      !store.applyRefresh(
        ["workspaces": [], "panes": []], endpoint: remote, creationEpoch: pollEpoch))
    assert(store.workspaces.contains { $0.id == created })
    // Finish another action after navigation, while a workspace drag is active.
    var later = WorkspaceCreation(
      endpoint: remote, previous: created, action: .open("/demo"), name: "demo", path: "/demo")
    store.workspaceCreations[later.id] = later
    store.select(later.id)
    store.select(created)
    later.result = later.proposedID
    let newest: [String: Any] = [
      "revision": 101, "panes": [],
      "workspaces": [
        ["id": created, "name": "~", "path": "/home/test", "tabs": []],
        ["id": later.proposedID, "name": "demo", "path": "/demo", "tabs": []],
        ["id": remote + "|other", "name": "Other", "path": "/other", "tabs": []],
      ],
    ]
    store.reorderingWorkspaces = true
    store.finishWorkspaceCreation(later, state: newest)
    assert(store.isCreatingWorkspace(later.id))
    store.reorderingWorkspaces = false
    assert(store.selected == created && !store.isCreatingWorkspace(later.id))
    assert(store.displayedWorkspaces.contains { $0.id == remote + "|other" })

    var failed = WorkspaceCreation(
      endpoint: remote, previous: created, action: .open("/missing"), name: "missing",
      path: "/missing")
    store.workspaceCreations[failed.id] = failed
    store.select(failed.id)
    failed.failure = "Missing directory"
    store.finishWorkspaceCreation(failed, state: nil)
    assert(store.selected == failed.id && store.workspaceCreations[failed.id]?.failure != nil)
    store.dismissCreation(failed)
    assert(store.selected == created && !store.isCreatingWorkspace(failed.id))
    print("Workspace handoff, navigation, concurrent layout, and inline failure checks passed.")
  }

  static func checkActivityIndicators() {
    let store = Store()
    store.apply(
      [
        "workspaces": [],
        "panes": [
          [
            "id": "manual-codex", "title": "Confirm yes | jam", "cwd": "/tmp",
            "status": "done", "exited": false, "agent_provider": "codex",
            "agent_source": "reported",
          ]
        ],
      ], endpoint: "")
    assert(store.panes["manual-codex"]?.isAIPane == true)
    assert(store.panes["manual-codex"]?.agentRole == nil)
    assert(store.panes["manual-codex"]?.activityStatus == .done)
    let shell = Pane(id: "shell", title: "Shell", cwd: "/tmp", status: "error", exited: false)
    var panes = (0..<11).map {
      Pane(
        id: "ai-\($0)", title: "Agent", cwd: "/tmp", status: "done", exited: false,
        agentProvider: "codex")
    }
    panes[10].status = "error"
    store.panes = Dictionary(uniqueKeysWithValues: ([shell] + panes).map { ($0.id, $0) })
    let tabs = panes.map { Tab(id: "tab-" + $0.id, name: "Agent", root: .leaf($0.id)) }
    let workspace = Workspace(id: "activity", name: "Activity", path: "/tmp", tabs: tabs)
    assert(store.activityStatus(for: workspace) == .error)
    assert(store.activityStatus(for: tabs[0]) == .done)
    assert(store.activityStatus(for: tabs[10]) == .error)
    store.panes["ai-10"]?.status = "blocked"
    assert(store.activityStatus(for: workspace) == .blocked)
    store.panes["ai-10"]?.status = "working"
    assert(store.activityStatus(for: workspace) == .working)
    for pane in panes { store.panes[pane.id]?.status = "idle" }
    assert(store.activityStatus(for: workspace) == .idle)
    assert(!store.activityStatus(for: workspace).filled)
    let ordinary = Tab(id: "ordinary", name: "Shell", root: .leaf("shell"))
    assert(store.activityStatus(for: ordinary) == nil)
    let mixed = Tab(
      id: "mixed", name: "Mixed", root: .split(false, 0.5, .leaf("shell"), .leaf("ai-0")))
    assert(store.activityStatus(for: mixed) == .idle)
    assert(!ActivityStatus.unknown.filled && !ActivityStatus.stopped.filled)
    for status in [ActivityStatus.done, .blocked, .error, .idle] {
      let tab = TabDragView(
        frame: NSRect(
          x: 0, y: 0, width: TabDragView.preferredWidth(for: "Tab 1", showsStatus: true), height: 28
        ))
      tab.label.stringValue = "Tab 1"
      tab.activityStatus = status
      tab.layout()
      assert(!tab.statusIndicator.isHidden)
      assert(tab.statusIndicator.frame.maxX < tab.icon.frame.minX)
      assert(tab.label.bounds.width >= ceil(tab.label.intrinsicContentSize.width))
      tab.activityStatus = nil
      tab.layout()
      assert(tab.statusIndicator.isHidden)
    }
    print(
      "Activity indicators passed: pane-to-tab-to-workspace priority, hollow cleared status, mixed tabs, and label geometry."
    )
  }

  static func checkUpgradeConsent() throws {
    precondition(ProcessInfo.processInfo.environment["JAM_DEFAULTS_SUITE"] != nil)
    JamDefaults.shared.removeObject(forKey: "appUpgradeState")
    defer { JamDefaults.shared.removeObject(forKey: "appUpgradeState") }
    let lock = NSLock()
    var actions: [(String, Bool)] = []
    var interrupted = true
    let expectedSessions = ["live-pane": "generation-before-confirmation"]
    var launchedSessions: [String: String]?
    let release = AppRelease(
      schema: 1, version: "0.0.3", protocol: "jam/test", minMacOS: "14.0", assets: [])
    func recorded(_ name: String) -> [(String, Bool)] {
      lock.lock()
      defer { lock.unlock() }
      return actions.filter { $0.0 == name }
    }
    let controller = AppUpgradeController { request in
      let action = request["action"] as? String ?? ""
      lock.lock()
      actions.append((action, request["confirmed"] as? Bool == true))
      let pending = interrupted
      if action == "recover" { interrupted = false }
      if action == "launch" {
        let plan = request["plan"] as? [String: Any]
        let targets = plan?["targets"] as? [[String: Any]]
        launchedSessions = targets?.first?["sessions"] as? [String: String]
        assert(targets?.first?["panes"] as? [String] == ["live-pane"])
      }
      lock.unlock()
      let reply: Any
      switch action {
      case "eligibility": reply = ["installation": "/tmp/Jam.app"]
      case "check":
        reply = [
          "current": "0.0.2", "available": true,
          "release": try JSONSerialization.jsonObject(with: JSONEncoder().encode(release)),
        ]
      case "pending":
        reply =
          pending
          ? [
            ["journal": "/tmp/journal", "version": "0.0.3", "phase": "prepared", "error": NSNull()]
          ] : []
      case "recover", "launch": reply = ["journal": "/tmp/journal"]
      case "status":
        reply = [
          "version": "0.0.3", "phase": recorded("launch").isEmpty ? "recovered" : "complete",
          "error": "Local installation recovered. Remote updates require fresh confirmation.",
        ]
      case "stage":
        reply = [
          "staged": [
            "directory": "/tmp/stage", "archive": "/tmp/stage/Jam.zip",
            "release": try JSONSerialization.jsonObject(with: JSONEncoder().encode(release)),
          ],
          "plan": [
            "installation": "/tmp/Jam.app", "local_running": true,
            "targets": [
              [
                "endpoint": "", "name": "Local", "panes": ["live-pane"],
                "sessions": expectedSessions, "incarnation": "fixture-incarnation",
                "build": "fixture-build", "version": "0.0.1",
              ]
            ],
            "offline": ["Build server"], "app_pids": [],
          ],
        ]
      default: throw NSError(domain: "Unexpected updater action: " + action, code: 1)
      }
      return try AppUpgradeResponse.validate(
        JSONSerialization.data(withJSONObject: reply), action: action)
    }
    controller.configure(automatic: false)
    try wait("interrupted upgrade discovery") { controller.pendingRecoveries.count == 1 }
    assert(
      recorded("recover").isEmpty && recorded("launch").isEmpty,
      "Startup discovery must never repair or interrupt")
    controller.recovery = controller.pendingRecoveries.first
    assert(recorded("recover").isEmpty, "Opening recovery details is not interruption approval")
    controller.recovery = nil
    assert(recorded("recover").isEmpty, "Cancelling recovery must leave its transaction untouched")
    controller.recovery = controller.pendingRecoveries.first
    controller.recover()
    try wait("confirmed recovery") {
      controller.phase == .idle && controller.pendingRecoveries.isEmpty
    }
    assert(
      recorded("recover").count == 1 && recorded("recover")[0].1,
      "Only the explicit recovery action sends confirmed=true")
    controller.available = release
    controller.prepare()
    try wait("verified upgrade staging") { controller.phase == .ready }
    assert(
      recorded("launch").isEmpty,
      "Downloading and displaying affected machines must not start installation")
    assert(controller.staged?.plan.offline == ["Build server"])
    assert(controller.staged?.plan.targets.first?.sessions == expectedSessions)
    controller.cancel()
    assert(recorded("launch").isEmpty, "Cancelling the staged upgrade must not launch the helper")
    controller.prepare()
    try wait("second verified staging") { controller.phase == .ready }
    controller.launch()
    try wait("confirmed upgrade") {
      controller.phase == .idle && !controller.notice.isEmpty && recorded("launch").count == 1
    }
    assert(recorded("launch")[0].1)
    assert(
      controller.releaseNotesURL?.absoluteString
        == "https://github.com/saffronjam/jam/releases/tag/v0.0.3")
    controller.dismissNotice()
    assert(controller.notice.isEmpty && controller.releaseNotesURL == nil)
    lock.lock()
    let preservedSessions = launchedSessions
    lock.unlock()
    assert(
      preservedSessions == expectedSessions,
      "Confirmed launch must preserve the staged live-session identities")
    controller.requireClientUpgrade(endpoint: "newer", minimum: "0.0.10", manual: true)
    try wait("unsatisfied newer remote") { controller.phase == .idle && !controller.error.isEmpty }
    assert(controller.available == nil)
    controller.forgetRequirement(endpoint: "newer")
    assert(controller.requiredMinimum == nil && controller.error.isEmpty)
    assert(
      controller.available == release, "Removed remote must not hide an eligible cached update")
    controller.skip()
    controller.requireClientUpgrade(endpoint: "newer", minimum: "0.0.10", manual: true)
    try wait("second newer remote check") { controller.phase == .idle && !controller.error.isEmpty }
    controller.forgetRequirement(endpoint: "newer")
    assert(controller.available == nil, "Removing a requirement must preserve exact-version Skip")

    let heldScan = DispatchSemaphore(value: 0)
    let raceStage = controller.staged!
    var raceLaunches = 0
    let racing = AppUpgradeController { request in
      switch request["action"] as? String {
      case "eligibility": return Data("{}".utf8)
      case "pending":
        guard heldScan.wait(timeout: .now() + 5) == .success else {
          throw NSError(domain: "Timed out waiting for recovery race", code: 1)
        }
        return try JSONSerialization.data(withJSONObject: [
          [
            "journal": "/tmp/earlier-cli-journal", "version": "0.0.3",
            "phase": "recovering_previous", "error": NSNull(),
          ]
        ])
      case "stage": return try JSONEncoder().encode(raceStage)
      case "launch":
        lock.lock()
        raceLaunches += 1
        lock.unlock()
        return Data("{\"journal\":\"/tmp/unexpected\"}".utf8)
      default: throw NSError(domain: "Unexpected recovery race action", code: 1)
      }
    }
    racing.configure(automatic: false)
    try wait("recovery race eligibility") { racing.managed }
    racing.available = release
    racing.prepare()
    try wait("recovery race staging") { racing.phase == .ready }
    assert(!racing.canLaunch, "Unfinished discovery must block final launch")
    heldScan.signal()
    try wait("late interrupted transaction") { racing.pendingRecoveries.count == 1 }
    assert(racing.phase == .idle && racing.staged == nil && !racing.showSheet)
    racing.launch()
    lock.lock()
    let unexpectedLaunches = raceLaunches
    lock.unlock()
    assert(
      unexpectedLaunches == 0, "Late discovery must revoke readiness and reject launch in code")
    print(
      "Upgrade consent passed: read-only crash discovery, cancel without actions, explicit recovery approval, staged-before-consent, and lossless confirmed launch with a live session."
    )
  }

  static func checkContextRequestRouting() {
    func snapshot(endpoint: String, ids: [String], coordinator: Bool, revision: Int) -> [String:
      Any]
    {
      let prefix = endpoint.isEmpty ? "" : endpoint + "|"
      var entries: [[String: Any]] = ids.map {
        ["id": prefix + $0, "name": $0, "path": "/tmp/" + $0, "tabs": [] as [Any]]
      }
      var panes: [[String: Any]] = []
      if coordinator, let first = entries.indices.first {
        entries[first]["coordinator"] = [
          "id": prefix + "agent", "pane": prefix + "coordinator-pane", "provider": "codex",
        ]
        panes = [
          [
            "id": prefix + "coordinator-pane", "workspace": prefix + ids[first],
            "agent_role": "coordinator", "agent_provider": "codex", "exited": true,
          ]
        ]
      }
      return ["workspace_model": 1, "revision": revision, "workspaces": entries, "panes": panes]
    }
    for targetEndpoint in ["remote-context", ""] {
      for closeCoordinator in [false, true] {
        let store = Store()
        store.remotes = [RemoteMachine(id: "remote-context", name: "Remote", destination: "unused")]
        store.remoteMessages["remote-context"] = "Connected"
        store.loadedRemotes.insert("remote-context")
        store.apply(
          snapshot(endpoint: "", ids: ["a", "b"], coordinator: true, revision: 10), endpoint: "")
        store.apply(
          snapshot(endpoint: "remote-context", ids: ["a", "b"], coordinator: true, revision: 10),
          endpoint: "remote-context")
        let selected = targetEndpoint.isEmpty ? "remote-context|b" : "b"
        store.select(selected)
        let prefix = targetEndpoint.isEmpty ? "" : targetEndpoint + "|"
        let request: [String: Any] =
          closeCoordinator
          ? ["Close": ["pane": prefix + "coordinator-pane"]]
          : ["WorkspaceClose": ["workspace": prefix + "a", "group": true]]
        let endpoint = store.requestEndpoint(request)
        assert(
          endpoint == targetEndpoint, "Context action must use its target, not the selected machine"
        )
        assert(
          store.requestEndpoint(["WorktreeRemove": ["workspace": prefix + "a", "force": false]])
            == targetEndpoint, "Context operation must resolve the same target endpoint")
        store.apply(
          snapshot(
            endpoint: targetEndpoint, ids: closeCoordinator ? ["a", "b"] : ["b"],
            coordinator: false, revision: 11), endpoint: endpoint)
        let ids = store.workspaces.map(\.id)
        let expected = Set(
          closeCoordinator
            ? ["a", "b", "remote-context|a", "remote-context|b"]
            : targetEndpoint.isEmpty
              ? ["b", "remote-context|a", "remote-context|b"]
              : ["a", "b", "remote-context|b"])
        assert(
          Set(ids) == expected && ids.count == expected.count,
          "Wrong machine rows changed or duplicated")
        assert(
          store.selected == selected, "A context close must preserve selection on the other machine"
        )
        if closeCoordinator {
          assert(store.workspaces.first { $0.id == prefix + "a" }?.coordinator == nil)
          assert(store.panes[prefix + "coordinator-pane"] == nil)
          let otherPrefix = targetEndpoint.isEmpty ? "remote-context|" : ""
          assert(store.workspaces.first { $0.id == otherPrefix + "a" }?.coordinator != nil)
          assert(store.panes[otherPrefix + "coordinator-pane"] != nil)
        }
        // The original regression trapped here on duplicate native sidebar keys.
        WorkspaceSidebar.Coordinator(store).update(NSOutlineView())
      }
    }
    let restored = Store()
    restored.apply(
      snapshot(endpoint: "remote-context", ids: ["a"], coordinator: false, revision: 1),
      endpoint: "remote-context")
    restored.select("remote-context|a")
    assert(restored.current != nil, "Fixture must restore the cached remote workspace")
    var unavailable = ServerStatus(
      running: true, compatible: false, build: nil, availableBuild: nil,
      updateAvailable: false, terminals: nil, error: nil)
    unavailable.state = "takeover_required"
    restored.serverStatuses["remote-context"] = unavailable
    assert(restored.selected.isEmpty && restored.current == nil && restored.visiblePaneIDs.isEmpty)
    assert(!restored.isLoadingSelectedRemote)

    let closing = Store()
    let empty = Workspace(id: "empty", name: "Empty", path: "/tmp", tabs: [])
    closing.workspaces = [empty]
    assert(!closing.workspaceCloseNeedsConfirmation(empty, group: true))
    let child = Workspace(
      id: "child", name: "Child", path: "/tmp/child",
      tabs: [Tab(id: "child-tab", name: "Terminal", root: .leaf("child-pane"))], parent: empty.id)
    closing.workspaces.append(child)
    assert(
      closing.workspaceCloseNeedsConfirmation(empty, group: true),
      "Unknown pane metadata must remain conservative")
    assert(!closing.workspaceCloseNeedsConfirmation(empty, group: false))
    closing.panes["child-pane"] = Pane(
      id: "child-pane", title: "Shell", cwd: "/tmp", status: "idle", exited: false)
    assert(
      closing.workspaceCloseNeedsConfirmation(empty, group: true),
      "Running child terminals will also close")
    closing.panes["child-pane"]?.exited = true
    assert(
      !closing.workspaceCloseNeedsConfirmation(empty, group: true),
      "Stopped panes need no confirmation")
    closing.workspaces[0].coordinator = WorkspaceCoordinator(
      id: "agent", pane: "coordinator-pane", provider: "codex")
    closing.panes["coordinator-pane"] = Pane(
      id: "coordinator-pane", title: "Codex", cwd: "/tmp", status: "idle", exited: false)
    assert(
      closing.workspaceCloseNeedsConfirmation(empty, group: true),
      "A running coordinator counts even without tabs")
    print(
      "Context routing passed: remote/local workspace and coordinator closes preserve other machine state and unique sidebar rows."
    )
  }

  static func checkEmptyWorkspaceNames() {
    let store = Store()
    func check(endpoint: String, home: String) {
      let prefix = endpoint.isEmpty ? "" : endpoint + "|"
      let workspace = prefix + "remembered-workspace"
      let pane = prefix + "remembered-pane"
      let directory = home + "/repos/demo-repo"
      var entry: [String: Any] = [
        "id": workspace, "name": "demo-repo", "auto_name": true,
        "path": home, "last_cwd": directory, "active_pane": pane,
        "tabs": [["id": prefix + "tab", "name": "Tab 1", "root": ["Leaf": pane]]],
      ]
      store.apply(
        [
          "workspace_model": 1, "workspaces": [entry],
          "panes": [["id": pane, "workspace": workspace, "cwd": directory]],
        ], endpoint: endpoint)
      assert(store.workspaces.first { $0.id == workspace }?.name == "demo-repo")
      entry["tabs"] = [] as [Any]
      entry.removeValue(forKey: "active_pane")
      for _ in 0..<2 {
        store.apply(["workspace_model": 1, "workspaces": [entry], "panes": []], endpoint: endpoint)
        assert(
          store.workspaces.first { $0.id == workspace }?.name == "demo-repo",
          "Closing the last tab must not alias the original home directory")
      }
      entry["name"] = "home"
      entry["last_cwd"] = home
      store.apply(["workspace_model": 1, "workspaces": [entry], "panes": []], endpoint: endpoint)
      assert(store.workspaces.first { $0.id == workspace }?.name == "~")
      entry["auto_name"] = false
      entry["name"] = "Custom name"
      store.apply(["workspace_model": 1, "workspaces": [entry], "panes": []], endpoint: endpoint)
      assert(store.workspaces.first { $0.id == workspace }?.name == "Custom name")
    }
    check(endpoint: "", home: FileManager.default.homeDirectoryForCurrentUser.path)
    store.remoteHomes["name-regression"] = "/home/test"
    check(endpoint: "name-regression", home: "/home/test")
    print(
      "Empty workspace names passed: remembered local/remote paths, repeated refresh, home aliases, and custom names."
    )
  }

  static func apply(_ reply: [String: Any], to store: Store) {
    guard let state = reply["State"] as? [String: Any] else {
      fatalError("Test daemon request failed: \(reply)")
    }
    store.apply(state, endpoint: "")
  }

  static func matchesGeometry(_ view: TerminalNSView) -> Bool {
    let size = view.cellSize
    return view.meta["cols"] as? Int == min(1000, max(1, Int(view.bounds.width / size.width)))
      && view.meta["rows"] as? Int == min(1000, max(1, Int(view.bounds.height / size.height)))
  }

  static func wait(_ stage: String = "terminal presentation", _ condition: () -> Bool) throws {
    let deadline = Date().addingTimeInterval(5)
    while !condition() {
      guard Date() < deadline else {
        throw NSError(domain: "Timed out: " + stage, code: 1)
      }
      let application = NSApplication.shared
      if let event = application.nextEvent(
        matching: .any, until: Date().addingTimeInterval(0.002), inMode: .default, dequeue: true)
      {
        application.sendEvent(event)
      }
      application.updateWindows()
    }
  }
}
