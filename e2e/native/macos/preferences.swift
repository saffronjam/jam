import AppKit
import Foundation

FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: [
  "source": "NSWorkspace.accessibilityDisplayShouldReduceMotion",
  "reduced_motion": NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
], options: [.sortedKeys]))
