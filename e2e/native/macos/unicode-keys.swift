import AppKit
import Carbon
import CoreGraphics
import Foundation

// Ordinary committed text travels through WindowServer keyboard events. IME
// composition uses physical-keys.swift instead of setting a Unicode payload.
// https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring(stringlength:unicodestring:)
let bundle = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath()
let pid = Int32(CommandLine.arguments[2])!
let text = CommandLine.arguments[3]
guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated,
  app.bundleURL?.resolvingSymlinksInPath().path == bundle.path,
  NSWorkspace.shared.frontmostApplication?.processIdentifier == pid, app.isActive
else { fatalError("Unicode keyboard input requires the same owned foreground application") }
precondition(CGPreflightPostEventAccess(), "Native Unicode keyboard input needs event-posting permission")
func selectedSource() -> String {
  let source = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
  let pointer = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)!
  return Unmanaged<CFString>.fromOpaque(pointer).takeUnretainedValue() as String
}
let originalSource = selectedSource()
guard let source = CGEventSource(stateID: .privateState) else {
  fatalError("Cannot create the native Unicode keyboard event source")
}
var count = 0
for scalar in text.unicodeScalars {
  let units = Array(String(scalar).utf16)
  for down in [true, false] {
    precondition(!app.isTerminated && NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
      "The owned application lost foreground focus during Unicode input")
    precondition(selectedSource() == originalSource, "Unicode input changed the native source")
    guard let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down) else {
      fatalError("Cannot create a native Unicode keyboard event")
    }
    event.flags = []
    units.withUnsafeBufferPointer {
      event.keyboardSetUnicodeString(stringLength: $0.count, unicodeString: $0.baseAddress)
    }
    event.post(tap: .cghidEventTap)
    count += 1
    RunLoop.current.run(until: Date().addingTimeInterval(0.015))
  }
}
precondition(selectedSource() == originalSource, "Unicode input changed the native source")
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: [
  "pid": pid, "source": originalSource, "text": text,
  "key_events": count, "backend": "CGEvent Unicode key down/up",
]))
