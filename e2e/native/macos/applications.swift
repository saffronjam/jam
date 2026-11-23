import AppKit
import Foundation
import Darwin

// Read-only process ownership through the operating system. Canonical paths
// distinguish private installations sharing the same bundle identifier.
let target = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath()
let pids = NSWorkspace.shared.runningApplications.compactMap { app -> Int32? in
  guard !app.isTerminated, kill(app.processIdentifier, 0) == 0,
    app.bundleURL?.resolvingSymlinksInPath() == target else { return nil }
  return app.processIdentifier
}
FileHandle.standardOutput.write(try JSONEncoder().encode(pids.sorted()))
