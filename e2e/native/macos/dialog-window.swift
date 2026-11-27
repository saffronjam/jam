import AppKit
import CoreGraphics
import Darwin
import Foundation

// Read only the visible window geometry of this private bundle's accessory
// helper. Unlike XCTest's bundle lookup, process ownership is unambiguous.
let target = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath().path
let helpers = Set(NSWorkspace.shared.runningApplications.compactMap { app -> Int32? in
  guard !app.isTerminated, kill(app.processIdentifier, 0) == 0,
    app.activationPolicy == .accessory,
    app.bundleURL?.resolvingSymlinksInPath().path == target
  else { return nil }
  return app.processIdentifier
})
guard let windows = CGWindowListCopyWindowInfo(
  [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]]
else { fatalError("The native dialog requires its graphical window-server session") }
let matches: [[String: Any]] = windows.compactMap { window in
  guard let pid = window[kCGWindowOwnerPID as String] as? Int32, helpers.contains(pid),
    let bounds = window[kCGWindowBounds as String] as? [String: Double],
    let x = bounds["X"], let y = bounds["Y"],
    let width = bounds["Width"], let height = bounds["Height"], width > 0, height > 0
  else { return nil }
  return ["pid": pid, "rect": ["x": x, "y": y, "width": width, "height": height]]
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: matches))
