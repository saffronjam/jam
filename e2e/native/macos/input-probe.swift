import AppKit
import CoreGraphics
import Darwin
import Foundation

// A native automation-tool control, independent of Jam and its daemon.
final class NativeInputProbe: NSObject, NSApplicationDelegate {
  var window: NSWindow!
  var monitor: Any?
  var children: [Process] = []
  var fieldTimer: Timer?
  var lastFieldState: Data?
  var menuButton: NSButton!
  var menuActions = 0

  func applicationDidFinishLaunching(_ notification: Notification) {
    record(["type": "capability", "post_event_access": CGPreflightPostEventAccess()])
    // Ordinary AppKit field editors route Command-A/C/V through the Edit menu.
    let menu = NSMenu()
    let appItem = NSMenuItem()
    let appMenu = NSMenu(title: "NativeProbe")
    appMenu.addItem(withTitle: "Quit NativeProbe", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    appItem.submenu = appMenu
    menu.addItem(appItem)
    let editItem = NSMenuItem()
    let editMenu = NSMenu(title: "Edit")
    for (title, action, key) in [("Cut", "cut:", "x"), ("Copy", "copy:", "c"),
                                 ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")] {
      editMenu.addItem(withTitle: title, action: Selector(action), keyEquivalent: key)
    }
    editItem.submenu = editMenu
    menu.addItem(editItem)
    NSApplication.shared.mainMenu = menu
    window = NSWindow(
      contentRect: NSRect(x: 80, y: 100, width: 600, height: 240),
      styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
    window.title = "Native Mac input probe"
    let field = NSTextField(frame: NSRect(x: 24, y: 140, width: 550, height: 28))
    field.setAccessibilityLabel("Probe text")
    window.contentView?.addSubview(field)
    let button = NSButton(
      title: "Open accessory prompt", target: self, action: #selector(openPrompt))
    button.frame = NSRect(x: 24, y: 80, width: 220, height: 32)
    window.contentView?.addSubview(button)
    menuButton = NSButton(title: "Open native menu", target: self, action: #selector(openMenu))
    menuButton.frame = NSRect(x: 24, y: 30, width: 220, height: 32)
    window.contentView?.addSubview(menuButton)
    monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .keyUp]) { event in
      self.record([
        "type": event.type == .keyDown ? "down" : "up",
        "key_code": event.keyCode, "characters": event.characters ?? "",
        "unmodified": event.charactersIgnoringModifiers ?? "",
        "modifiers": event.modifierFlags.rawValue,
      ])
      return event
    }
    window.makeKeyAndOrderFront(nil)
    window.makeFirstResponder(field)
    NSApplication.shared.activate(ignoringOtherApps: true)
    fieldTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
      guard let self else { return }
      let editor = field.currentEditor() as? NSTextView
      let state: [String: Any] = ["type": "field", "value": field.stringValue,
                                 "editor": editor?.string ?? field.stringValue,
                                 "marked": editor?.hasMarkedText() ?? false,
                                 "input_source": editor?.inputContext?.selectedKeyboardInputSource ?? ""]
      let data = try! JSONSerialization.data(withJSONObject: state, options: [.sortedKeys])
      if data != self.lastFieldState {
        self.record(state)
        self.lastFieldState = data
      }
    }
  }

  func record(_ value: [String: Any]) {
    guard let path = ProcessInfo.processInfo.environment["JAM_PROBE_EVENTS"],
      let data = try? JSONSerialization.data(withJSONObject: value) else { return }
    let descriptor = open(path, O_WRONLY | O_CREAT | O_APPEND, 0o600)
    guard descriptor >= 0 else { fatalError("Cannot retain native input events") }
    let line = data + Data([10])
    let written = line.withUnsafeBytes { bytes in
      write(descriptor, bytes.baseAddress!, bytes.count)
    }
    close(descriptor)
    precondition(written == line.count, "Cannot retain a complete native input event")
  }

  @objc func openPrompt() {
    let child = Process()
    child.executableURL = Bundle.main.executableURL!
    child.arguments = ["--prompt"]
    child.terminationHandler = { process in
      self.record(["helper_pid": process.processIdentifier, "helper_status": process.terminationStatus])
    }
    do { try child.run() } catch { fatalError("Cannot start owned accessory helper: \(error)") }
    children.append(child)
  }

  @objc func openMenu() {
    let menu = NSMenu(title: "Native menu control")
    let item = NSMenuItem(title: "Set native marker", action: #selector(selectMenu), keyEquivalent: "")
    item.target = self
    menu.addItem(item)
    menu.popUp(positioning: nil, at: .zero, in: menuButton)
  }

  @objc func selectMenu() {
    menuActions += 1
    menuButton.title = "Native menu selected"
    record(["type": "menu_action", "count": menuActions])
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

let application = NSApplication.shared
if CommandLine.arguments.contains("--prompt") {
  application.setActivationPolicy(.accessory)
  application.activate(ignoringOtherApps: true)
  let alert = NSAlert()
  alert.messageText = "Native accessory prompt"
  alert.informativeText = "Owned same-bundle helper."
  alert.addButton(withTitle: "Continue")
  alert.addButton(withTitle: "Cancel")
  exit(alert.runModal() == .alertFirstButtonReturn ? 0 : 1)
} else {
  let delegate = NativeInputProbe()
  application.delegate = delegate
  application.setActivationPolicy(.regular)
  withExtendedLifetime(delegate) { application.run() }
}
