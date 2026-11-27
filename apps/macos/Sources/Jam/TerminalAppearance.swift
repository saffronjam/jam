import AppKit
import SwiftUI

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
  var textHighlight: NSColor {
    self == .midnight ? Self.midnightHighlight : Self.warmHighlight
  }
  func usageFill(percent: Double) -> NSColor {
    if percent >= 100 { return self == .midnight ? Self.midnightLimit : Self.warmLimit }
    if percent > 90 { return self == .midnight ? Self.midnightWarning : Self.warmWarning }
    return Self.usageNormal
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
  private static let midnightHighlight = adaptive(dark: 0x9ac4fa, light: 0xb6d5ff)
  private static let warmHighlight = adaptive(dark: 0xffbd81, light: 0xffcf9e)
  private static let usageNormal = adaptive(dark: 0xffffff, light: 0x252830)
  private static let midnightWarning = adaptive(dark: 0xc3a03a, light: 0x866810)
  private static let midnightLimit = adaptive(dark: 0xc55258, light: 0x982f3b)
  private static let warmWarning = adaptive(dark: 0xc89b32, light: 0x896210)
  private static let warmLimit = adaptive(dark: 0xc65546, light: 0x9b3429)
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

private enum JamThemeKey: EnvironmentKey {
  static let defaultValue = JamTheme.midnight
}

extension EnvironmentValues {
  var jamTheme: JamTheme {
    get { self[JamThemeKey.self] }
    set { self[JamThemeKey.self] = newValue }
  }
}

extension View {
  func jamTheme(_ theme: JamTheme) -> some View {
    let accent = Color(nsColor: theme.accent)
    return environment(\.jamTheme, theme).tint(accent).accentColor(accent)
  }

  func jamSheet<Content: View>(
    isPresented: Binding<Bool>, @ViewBuilder content: @escaping () -> Content
  ) -> some View {
    sheet(isPresented: isPresented) { content().modifier(JamSheetSurface()) }
  }

  func jamSheet<Item: Identifiable, Content: View>(
    item: Binding<Item?>, @ViewBuilder content: @escaping (Item) -> Content
  ) -> some View {
    sheet(item: item) { item in content(item).modifier(JamSheetSurface()) }
  }
}

private struct JamSheetSurface: ViewModifier {
  @Environment(\.jamTheme) private var theme

  func body(content: Content) -> some View {
    content
      .textFieldStyle(JamSheetFieldStyle())
      .presentationBackground(Color(nsColor: theme.background))
  }
}

private struct JamSheetFieldStyle: TextFieldStyle {
  func _body(configuration: TextField<_Label>) -> some View {
    configuration.textFieldStyle(.plain).modifier(JamSheetFieldBorder())
  }
}

private struct JamSheetFieldBorder: ViewModifier {
  @Environment(\.jamTheme) private var theme
  @FocusState private var focused: Bool

  func body(content: Content) -> some View {
    let border = RoundedRectangle(cornerRadius: 6)
    return content.focused($focused)
      .padding(.horizontal, 7).padding(.vertical, 4)
      .background(Color(nsColor: theme.terminalBackground), in: border)
      .overlay {
        border.strokeBorder(
          Color(nsColor: focused ? theme.accent : theme.paneBorder(focused: false)),
          lineWidth: focused ? 2 : 1
        ).allowsHitTesting(false)
      }
  }
}
