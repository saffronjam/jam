import Carbon
import Foundation

// XCTest maps ordinary/Shift keys but silently drops characters requiring
// Option on the current layout. Discover the actual physical combination;
// callers still send real native key-down/up events, never committed text.
let source = TISCopyCurrentKeyboardLayoutInputSource().takeRetainedValue()
guard let pointer = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) else {
  fatalError("The native keyboard layout has no physical translation table")
}
let data = Unmanaged<CFData>.fromOpaque(pointer).takeUnretainedValue()
let layout = UnsafeRawPointer(CFDataGetBytePtr(data)!).assumingMemoryBound(to: UCKeyboardLayout.self)
func translated(_ key: UInt16, _ flags: Int) -> String {
  var state: UInt32 = 0
  var count = 0
  var characters = [UniChar](repeating: 0, count: 8)
  let result = UCKeyTranslate(
    layout, key, UInt16(kUCKeyActionDown), UInt32(flags >> 8), UInt32(LMGetKbdType()),
    OptionBits(kUCKeyTranslateNoDeadKeysMask), &state, characters.count, &count, &characters)
  guard result == noErr else { return "" }
  return String(utf16CodeUnits: characters, count: Int(count))
}
func printable(_ text: String) -> Bool {
  text.unicodeScalars.count == 1 && (32...126).contains(Int(text.unicodeScalars.first!.value))
}
var ordinary: Set<String> = []
for flags in [0, shiftKey] {
  for key in UInt16(0)..<UInt16(128) {
    let text = translated(key, flags)
    if printable(text) { ordinary.insert(text) }
  }
}
var keys: [String: [String: Any]] = [:]
for flags in [optionKey, optionKey | shiftKey] {
  for code in UInt16(0)..<UInt16(128) {
    let text = translated(code, flags)
    let base = translated(code, 0)
    guard printable(text), printable(base), !ordinary.contains(text), keys[text] == nil else { continue }
    keys[text] = ["key": base, "key_code": code, "shift": flags & shiftKey != 0]
  }
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: keys, options: [.sortedKeys]))
