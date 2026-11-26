import AppKit

enum JamTheme: String, CaseIterable, Identifiable {
  case midnight, warm

  var id: String { rawValue }
  var title: String { self == .midnight ? "Midnight" : "Warm" }

  var background: NSColor {
    self == .midnight ? Self.midnightBackground : .windowBackgroundColor
  }
  var terminalBackground: NSColor {
    self == .midnight ? Self.midnightTerminal : Self.warmTerminal
  }
  var accent: NSColor {
    self == .midnight ? Self.midnightAccent : Self.warmAccent
  }
  var selectionFill: NSColor {
    self == .midnight ? Self.midnightSelection : Self.warmSelection
  }
  func paneBorder(focused: Bool) -> NSColor {
    if self == .warm {
      return focused ? accent.withAlphaComponent(0.55) : .separatorColor
    }
    return focused ? Self.midnightFocusedBorder : Self.midnightBorder
  }

  private static let midnightBackground = adaptive(dark: 0x10141d, light: 0xf0f3fa)
  private static let midnightTerminal = adaptive(dark: 0x0b0e15, light: 0xf8faff)
  private static let midnightAccent = adaptive(dark: 0x8bb5f2, light: 0x285db1)
  private static let warmAccent = adaptive(dark: 0xff9b45, light: 0xa94f0a)
  private static let midnightSelection = adaptive(dark: 0x344f76, light: 0x285db1)
  private static let warmSelection = adaptive(dark: 0x78421e, light: 0x934d20)
  private static let midnightBorder = adaptive(dark: 0x263249, light: 0xc9d3e6)
  private static let midnightFocusedBorder = adaptive(dark: 0x435981, light: 0x6888bd)

  private static func adaptive(dark: UInt32, light: UInt32) -> NSColor {
    NSColor(name: nil) { appearance in
      let hex = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
      return NSColor(
        srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
        green: CGFloat((hex >> 8) & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: 1)
    }
  }

  private static let warmTerminal = NSColor(name: nil) { appearance in
    var color = NSColor.windowBackgroundColor
    appearance.performAsCurrentDrawingAppearance {
      color =
        NSColor.windowBackgroundColor.blended(withFraction: 0.12, of: .black)
        ?? .windowBackgroundColor
    }
    return color
  }
}
