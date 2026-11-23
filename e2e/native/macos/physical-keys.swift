import AppKit
import Carbon
import CoreGraphics
import Foundation

// XCTest's text synthesizer temporarily selects ABC, bypassing a real IME.
// Send virtual-key down/up events through the window server instead. Never set
// Unicode strings, change the input source, or call the target's text methods.
let bundle = URL(fileURLWithPath: CommandLine.arguments[1]).resolvingSymlinksInPath()
let pid = Int32(CommandLine.arguments[2])!
let expectedSource = CommandLine.arguments[3]
let request = try JSONSerialization.jsonObject(
  with: Data(CommandLine.arguments[4].utf8)) as! [String: String]
guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated,
  app.bundleURL?.resolvingSymlinksInPath().path == bundle.path,
  NSWorkspace.shared.frontmostApplication?.processIdentifier == pid, app.isActive
else { fatalError("Physical keyboard input requires the same owned foreground application") }
precondition(CGPreflightPostEventAccess(),
  "The native automation environment lacks permission to post physical key events")

func selectedSource() -> String {
  let source = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
  let pointer = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)!
  return Unmanaged<CFString>.fromOpaque(pointer).takeUnretainedValue() as String
}
precondition(selectedSource() == expectedSource, "The requested native input method is not selected")
guard let source = CGEventSource(stateID: .privateState) else {
  fatalError("Cannot create the native keyboard event source")
}
var events: [CGEvent] = []
func event(_ code: CGKeyCode, _ down: Bool, _ flags: CGEventFlags = []) {
  guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down) else {
    fatalError("Cannot create a physical keyboard event")
  }
  event.flags = flags
  events.append(event)
}
func press(_ code: CGKeyCode, shifted: Bool = false) {
  if shifted { event(CGKeyCode(kVK_Shift), true, .maskShift) }
  event(code, true, shifted ? .maskShift : [])
  event(code, false, shifted ? .maskShift : [])
  if shifted { event(CGKeyCode(kVK_Shift), false) }
}
if let text = request["text"], request.count == 1 {
  let layoutSource = TISCopyCurrentKeyboardLayoutInputSource().takeRetainedValue()
  guard let pointer = TISGetInputSourceProperty(layoutSource, kTISPropertyUnicodeKeyLayoutData) else {
    fatalError("The current physical keyboard layout has no translation table")
  }
  let data = Unmanaged<CFData>.fromOpaque(pointer).takeUnretainedValue()
  let layout = UnsafeRawPointer(CFDataGetBytePtr(data)!).assumingMemoryBound(to: UCKeyboardLayout.self)
  var keys: [String: (CGKeyCode, Bool)] = [:]
  for flags in [0, shiftKey] {
    for code in UInt16(0)..<UInt16(128) {
      var state: UInt32 = 0
      var count = 0
      var characters = [UniChar](repeating: 0, count: 8)
      let status = UCKeyTranslate(
        layout, code, UInt16(kUCKeyActionDown), UInt32(flags >> 8), UInt32(LMGetKbdType()),
        OptionBits(kUCKeyTranslateNoDeadKeysMask), &state, characters.count, &count, &characters)
      let translated = String(utf16CodeUnits: characters, count: Int(count))
      if status == noErr, translated.count == 1, keys[translated] == nil {
        keys[translated] = (code, flags != 0)
      }
    }
  }
  for character in text {
    guard character.isASCII, let (code, shifted) = keys[String(character)] else {
      fatalError("Physical IME input requires a key available on the native keyboard layout")
    }
    press(code, shifted: shifted)
  }
} else if let key = request["key"], request.count == 1 {
  let keys: [String: Int] = [
    "ENTER": kVK_Return, "ESCAPE": kVK_Escape, "F6": kVK_F6,
    "TAB": kVK_Tab, "BACKSPACE": kVK_Delete, "DELETE": kVK_ForwardDelete,
    "LEFT": kVK_LeftArrow, "RIGHT": kVK_RightArrow,
    "UP": kVK_UpArrow, "DOWN": kVK_DownArrow,
  ]
  guard let code = keys[key] else { fatalError("Unsupported physical IME control key") }
  press(CGKeyCode(code))
} else {
  fatalError("Physical input needs exactly one text or control-key request")
}
for event in events {
  precondition(!app.isTerminated && NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
    "The owned application lost foreground focus during physical input")
  precondition(selectedSource() == expectedSource,
    "The native input source changed before a physical key event")
  event.post(tap: .cghidEventTap)
  RunLoop.current.run(until: Date().addingTimeInterval(0.015))
}
precondition(selectedSource() == expectedSource, "Physical input changed the native input method")
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: [
  "pid": pid, "source": selectedSource(), "request": request,
  "key_events": events.count, "backend": "CGEvent virtual-key down/up",
]))
