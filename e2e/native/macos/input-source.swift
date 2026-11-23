import AppKit
import Carbon
import CoreGraphics
import Foundation

func property(_ source: TISInputSource, _ key: CFString) -> Any? {
  guard let pointer = TISGetInputSourceProperty(source, key) else { return nil }
  return Unmanaged<AnyObject>.fromOpaque(pointer).takeUnretainedValue()
}
func identifier(_ source: TISInputSource) -> String {
  property(source, kTISPropertyInputSourceID) as! String
}
func pump(until deadline: Date) {
  if let app = NSApp {
    while Date() < deadline {
      if let event = app.nextEvent(matching: .any, until: deadline, inMode: .default, dequeue: true) {
        app.sendEvent(event)
      }
      app.updateWindows()
    }
  } else {
    RunLoop.current.run(until: deadline)
  }
}
let requested = CommandLine.arguments[1]
let sources = TISCreateInputSourceList(nil, true).takeRetainedValue() as! [TISInputSource]
func source(_ id: String) -> TISInputSource {
  let current = TISCreateInputSourceList(nil, true).takeRetainedValue() as! [TISInputSource]
  let matches = current.filter { identifier($0) == id }
  precondition(matches.count == 1, "The requested native input source must be installed")
  return matches[0]
}
let original = identifier(TISCopyCurrentKeyboardInputSource().takeRetainedValue())
let target = source(requested)
precondition(property(target, kTISPropertyInputSourceIsSelectCapable) as? Bool == true)
let enabled = property(target, kTISPropertyInputSourceIsEnabled) as? Bool == true
let inputMode = property(target, kTISPropertyInputSourceType) as? String == kTISTypeKeyboardInputMode as String
let previousMode = property(source(original), kTISPropertyInputSourceType) as? String == kTISTypeKeyboardInputMode as String
var parents: [TISInputSource] = []
if inputMode {
  parents = sources.filter {
    property($0, kTISPropertyInputSourceType) as? String
      == kTISTypeKeyboardInputMethodModeEnabled as String
      && requested.hasPrefix(identifier($0) + ".")
  }
  precondition(parents.count == 1, "The native input mode needs its exact parent input method")
}
// A mode's enabled preference can remain true while its parent is disabled.
// TIS requires both enabled before selection. Record every change first so
// the caller can restore it even if the selection reply is lost.
let enabling = (parents + [target]).filter {
  property($0, kTISPropertyInputSourceIsEnabled) as? Bool != true
}
var observation: [String: Any] = [
  "before": original, "selected": requested, "was_enabled": enabled,
  "restore_disable": enabling.map(identifier),
  "post_event_access": CGPreflightPostEventAccess(),
]
if CommandLine.arguments.last == "--inspect" {
  FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: observation))
  exit(0)
}
for item in enabling {
  precondition(TISEnableInputSource(item) == noErr,
    "Cannot enable native input source \(identifier(item))")
}
// Selecting an input method from a background CLI can update the menu source
// while the focused application's input context remains on its previous mode.
// Give this OS-preference helper its own real field/key window for the switch,
// then return to the same application. No product text or callbacks are used.
let foreground = NSWorkspace.shared.frontmostApplication
var handoff: NSWindow?
var editor: NSTextField?
let restoringMethod = CommandLine.arguments.dropFirst(2).contains {
  let kind = property(source($0), kTISPropertyInputSourceType) as? String
  return kind == kTISTypeKeyboardInputMethodModeEnabled as String
    || kind == kTISTypeKeyboardInputMode as String
}
if inputMode || previousMode || restoringMethod {
  let app = NSApplication.shared
  app.setActivationPolicy(.accessory)
  let window = NSWindow(
    contentRect: NSRect(x: 40, y: 40, width: 200, height: 60),
    styleMask: [.titled], backing: .buffered, defer: false)
  window.title = "Native input source handoff"
  window.isReleasedWhenClosed = false
  let field = NSTextField(frame: NSRect(x: 12, y: 16, width: 176, height: 28))
  window.contentView?.addSubview(field)
  window.makeKeyAndOrderFront(nil)
  app.activate(ignoringOtherApps: true)
  window.makeFirstResponder(field)
  let ready = Date().addingTimeInterval(5)
  while !window.isKeyWindow || field.currentEditor() == nil {
    precondition(Date() < ready, "The native input-source helper did not obtain keyboard focus")
    pump(until: Date().addingTimeInterval(0.05))
  }
  handoff = window
  editor = field
}
let selected = TISSelectInputSource(target)
precondition(selected == noErr, "Cannot select native input source: OSStatus \(selected)")
let deadline = Date().addingTimeInterval(5)
while identifier(TISCopyCurrentKeyboardInputSource().takeRetainedValue()) != requested {
  precondition(Date() < deadline,
    "The native input source did not change from \(identifier(TISCopyCurrentKeyboardInputSource().takeRetainedValue())) to \(requested)")
  pump(until: Date().addingTimeInterval(0.05))
}
if let editor {
  let ready = Date().addingTimeInterval(5)
  while editor.currentEditor()?.inputContext?.selectedKeyboardInputSource != requested {
    precondition(Date() < ready, "The native field input context did not select the requested source")
    pump(until: Date().addingTimeInterval(0.05))
  }
  // Drain the input-method activation messages before releasing its key window.
  pump(until: Date().addingTimeInterval(0.15))
  observation["native_context"] = editor.currentEditor()?.inputContext?.selectedKeyboardInputSource ?? ""
}
for disabled in CommandLine.arguments.dropFirst(2) {
  precondition(disabled != requested)
  precondition(TISDisableInputSource(source(disabled)) == noErr, "Cannot restore input source enabled state")
  precondition(property(source(disabled), kTISPropertyInputSourceIsEnabled) as? Bool == false,
    "The native input source did not return to its disabled state")
}
if let handoff {
  handoff.orderOut(nil)
  precondition(foreground?.activate(options: [.activateAllWindows, .activateIgnoringOtherApps]) == true,
    "Cannot return keyboard focus after the native source handoff")
  let ready = Date().addingTimeInterval(5)
  while NSWorkspace.shared.frontmostApplication?.processIdentifier != foreground?.processIdentifier {
    precondition(Date() < ready, "The original application did not regain native focus")
    pump(until: Date().addingTimeInterval(0.05))
  }
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: observation))
