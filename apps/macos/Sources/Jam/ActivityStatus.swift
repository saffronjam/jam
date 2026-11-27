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

private final class ActivityGlyph: CAShapeLayer {
  @objc func tick(_ displayLink: CADisplayLink) { rotate(at: displayLink.targetTimestamp) }

  func rotate(at time: CFTimeInterval) {
    // A replacement row picks up the same two-second revolution on its first frame.
    let angle = -Double.pi * time.truncatingRemainder(dividingBy: 2)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    setAffineTransform(CGAffineTransform(rotationAngle: angle))
    CATransaction.commit()
  }
}

final class ActivityDotView: NSView {
  private let glyph = ActivityGlyph()
  private var spinClock: CADisplayLink?
  var isSpinning: Bool { spinClock != nil }
  var status: ActivityStatus = .unknown {
    didSet {
      guard status != oldValue else { return }
      setAccessibilityLabel(status.description)
      if window != nil, !bounds.isEmpty, !isHiddenOrHasHiddenAncestor,
        !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
      {
        let fade = CATransition()
        fade.type = .fade
        fade.duration = 0.14
        fade.timingFunction = CAMediaTimingFunction(name: .easeOut)
        layer?.add(fade, forKey: kCATransition)
      }
      updateGlyph()
    }
  }
  override init(frame: NSRect) {
    super.init(frame: frame)
    wantsLayer = true
    glyph.lineCap = .round
    layer?.addSublayer(glyph)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel(status.description)
    NSWorkspace.shared.notificationCenter.addObserver(
      self, selector: #selector(updateGlyph),
      name: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  deinit {
    spinClock?.invalidate()
    NSWorkspace.shared.notificationCenter.removeObserver(self)
  }
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
  override func layout() {
    super.layout()
    updateGlyph()
  }
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    updateGlyph()
  }
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    updateGlyph()
  }
  @objc private func updateGlyph() {
    let working = status == .working
    let size = min(working ? 9 : 6, bounds.width, bounds.height)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    glyph.bounds = CGRect(x: 0, y: 0, width: size, height: size)
    glyph.position = CGPoint(x: bounds.midX, y: bounds.midY)
    glyph.lineWidth = working ? 1.5 : 1
    let inset = glyph.lineWidth / 2
    glyph.path = CGPath(ellipseIn: glyph.bounds.insetBy(dx: inset, dy: inset), transform: nil)
    glyph.strokeEnd = working ? 0.73 : 1
    effectiveAppearance.performAsCurrentDrawingAppearance {
      glyph.strokeColor = status.color.cgColor
      glyph.fillColor = status.filled && !working ? status.color.cgColor : nil
    }
    if working && window != nil && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
      if spinClock == nil {
        glyph.rotate(at: CACurrentMediaTime())
        let clock = displayLink(target: glyph, selector: #selector(ActivityGlyph.tick(_:)))
        clock.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 60, preferred: 30)
        clock.add(to: .main, forMode: .common)
        spinClock = clock
      }
    } else if let spinClock {
      spinClock.invalidate()
      self.spinClock = nil
      glyph.setAffineTransform(.identity)
    }
    CATransaction.commit()
  }
}

struct ActivityIndicator: NSViewRepresentable {
  let status: ActivityStatus
  func makeNSView(context: Context) -> ActivityDotView { ActivityDotView() }
  func updateNSView(_ view: ActivityDotView, context: Context) { view.status = status }
}
