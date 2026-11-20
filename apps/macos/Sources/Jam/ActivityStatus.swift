import AppKit
import SwiftUI

/// Shared priority and appearance for pane, tab, and workspace activity.
enum ActivityStatus: String {
  case error, blocked, working, done, idle, unknown, stopped

  init(_ value: String) { self = Self(rawValue: value) ?? .unknown }

  static func aggregate(_ values: [Self]) -> Self {
    for state in [Self.error, .blocked, .working, .done, .idle] where values.contains(state) {
      return state
    }
    return !values.isEmpty && values.allSatisfy { $0 == .stopped } ? .stopped : .unknown
  }

  var filled: Bool { [.error, .blocked, .working, .done].contains(self) }
  var color: NSColor {
    switch self {
    case .error: return .systemRed
    case .blocked: return .systemOrange
    case .working: return .systemBlue
    case .done: return .systemGreen
    default: return .tertiaryLabelColor
    }
  }
  var description: String {
    switch self {
    case .unknown, .idle: return "No pending activity"
    case .stopped: return "Stopped"
    case .done: return "Done"
    case .working: return "Working"
    case .blocked: return "Needs attention"
    case .error: return "Error"
    }
  }
}

final class ActivityDotView: NSView {
  var status: ActivityStatus = .unknown {
    didSet {
      needsDisplay = true
      setAccessibilityLabel(status.description)
    }
  }
  override init(frame: NSRect) {
    super.init(frame: frame)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel(status.description)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
  override func draw(_ dirtyRect: NSRect) {
    let dot = NSBezierPath(ovalIn: bounds.insetBy(dx: 0.5, dy: 0.5))
    status.color.setFill()
    status.color.setStroke()
    dot.lineWidth = 1
    if status.filled { dot.fill() } else { dot.stroke() }
  }
}

struct ActivityIndicator: NSViewRepresentable {
  let status: ActivityStatus
  func makeNSView(context: Context) -> ActivityDotView { ActivityDotView() }
  func updateNSView(_ view: ActivityDotView, context: Context) { view.status = status }
}
