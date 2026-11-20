import AppKit

enum TerminalAppearance {
  static let background = NSColor(name: nil) { appearance in
    var color = NSColor.windowBackgroundColor
    appearance.performAsCurrentDrawingAppearance {
      color =
        NSColor.windowBackgroundColor.blended(withFraction: 0.12, of: .black)
        ?? .windowBackgroundColor
    }
    return color
  }
}
