import AppKit
import Carbon
import CoreGraphics
import Foundation

func property(_ source: TISInputSource, _ key: CFString) -> Any? {
  guard let pointer = TISGetInputSourceProperty(source, key) else { return nil }
  return Unmanaged<AnyObject>.fromOpaque(pointer).takeUnretainedValue()
}

let sources = TISCreateInputSourceList(nil, true).takeRetainedValue() as! [TISInputSource]
let current = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
let inputs = sources.map { source -> [String: Any] in
  [
    "id": property(source, kTISPropertyInputSourceID) as? String ?? "",
    "name": property(source, kTISPropertyLocalizedName) as? String ?? "",
    "selectable": property(source, kTISPropertyInputSourceIsSelectCapable) as? Bool ?? false,
    "enabled": property(source, kTISPropertyInputSourceIsEnabled) as? Bool ?? false,
    "type": property(source, kTISPropertyInputSourceType) as? String ?? "",
  ]
}
let display = CGMainDisplayID()
let modes = (CGDisplayCopyAllDisplayModes(display, nil) as? [CGDisplayMode] ?? []).map {
  ["width": $0.width, "height": $0.height, "pixel_width": $0.pixelWidth,
   "pixel_height": $0.pixelHeight, "id": Int($0.ioDisplayModeID)]
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: [
  "input_source": property(current, kTISPropertyInputSourceID) as? String ?? "",
  "input_sources": inputs,
  "display": ["id": Int(display), "modes": modes,
              "backing_scale": NSScreen.main?.backingScaleFactor ?? 0],
  "reduced_motion": NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
  "post_event_access": CGPreflightPostEventAccess(),
], options: [.prettyPrinted, .sortedKeys]))
