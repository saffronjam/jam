import AppKit
import Foundation

let target = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath().path
let pid = Int32(CommandLine.arguments[2])!
guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated,
  app.bundleURL?.resolvingSymlinksInPath().path == target
else { fatalError("Normal quit requires the exact owned application process") }
precondition(app.terminate(), "The owned application rejected normal native termination")
