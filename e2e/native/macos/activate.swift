import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import Foundation

// Activate only the already-running process in this private installation.
// XCTest still selects the AUT; this verifies actual OS foreground ownership.
let target = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath().path
let pid = Int32(CommandLine.arguments[2])!
let inspect = CommandLine.arguments.last == "--inspect"
guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated,
  app.bundleURL?.resolvingSymlinksInPath().path == target
else { fatalError("The owned native client is no longer running") }
let before = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
if !inspect {
  guard app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps]) else {
    fatalError("The window server rejected native client activation")
  }
  let deadline = Date().addingTimeInterval(5)
  while NSWorkspace.shared.frontmostApplication?.processIdentifier != pid || !app.isActive {
    guard !app.isTerminated, Date() < deadline else {
      fatalError("The owned native client did not become the foreground application")
    }
    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
  }
}
// Raise the already-running client's main window without clicking the workspace
// toolbar, which can create a tab or change the active tab during a client switch.
let accessibility = AXUIElementCreateApplication(pid)
if !inspect {
  var mainWindow: CFTypeRef?
  if AXUIElementCopyAttributeValue(accessibility, kAXMainWindowAttribute as CFString, &mainWindow)
    == .success, let mainWindow, CFGetTypeID(mainWindow) == AXUIElementGetTypeID()
  {
    let window = mainWindow as! AXUIElement
    _ = AXUIElementPerformAction(window, kAXRaiseAction as CFString)
  }
}
let windows = (CGWindowListCopyWindowInfo(
  [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? [])
  .filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid }
  .compactMap { $0[kCGWindowBounds as String] }
var observation: [String: Any] = [
  "before": before,
  "foreground": NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0,
  "active": app.isActive, "windows": windows, "ax_trusted": AXIsProcessTrusted(),
]
// Observe the OS focused window independently of XCTest's same-bundle cache.
// Inspection never activates a window or repairs focus after a physical click.
var focused: CFTypeRef?
let status = AXUIElementCopyAttributeValue(
  accessibility, kAXFocusedWindowAttribute as CFString, &focused)
observation["focused_window_status"] = status.rawValue
if status == .success, let focused, CFGetTypeID(focused) == AXUIElementGetTypeID() {
  let window = focused as! AXUIElement
  var position: CFTypeRef?
  var size: CFTypeRef?
  let positionStatus = AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &position)
  let sizeStatus = AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &size)
  if positionStatus == .success, sizeStatus == .success, let position, let size,
    CFGetTypeID(position) == AXValueGetTypeID(), CFGetTypeID(size) == AXValueGetTypeID()
  {
    var point = CGPoint.zero
    var extent = CGSize.zero
    if AXValueGetValue(position as! AXValue, .cgPoint, &point),
      AXValueGetValue(size as! AXValue, .cgSize, &extent)
    {
      observation["focused_window"] = [
        "x": point.x, "y": point.y, "width": extent.width, "height": extent.height,
      ]
    }
  }
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: observation))
