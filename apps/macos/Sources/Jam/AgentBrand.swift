import AppKit
import SwiftUI

/// Vector provider marks; templates follow the current macOS appearance.
enum AgentBrand {
  static func name(_ provider: String) -> String { provider == "claude" ? "Claude" : "Codex" }
  private static let images: [String: NSImage] = {
    var result: [String: NSImage] = [:]
    for (provider, file) in [("claude", "claude"), ("codex", "openai")] {
      var source = URL(fileURLWithPath: #filePath)
      for _ in 0..<5 { source.deleteLastPathComponent() }
      let url =
        Bundle.main.url(forResource: file, withExtension: "pdf", subdirectory: "agent-icons")
        ?? source.appendingPathComponent("assets/agent-icons/\(file).pdf")
      if let image = NSImage(contentsOf: url) {
        image.isTemplate = true
        image.size = NSSize(width: 14, height: 14)
        result[provider] = image
      }
    }
    return result
  }()
  static func image(_ provider: String) -> NSImage? { images[provider] }
}

struct AgentBrandIcon: View {
  let provider: String?
  var body: some View {
    Group {
      if let provider, let image = AgentBrand.image(provider) {
        Image(nsImage: image).resizable().scaledToFit()
      } else {
        Image(systemName: "terminal").resizable().scaledToFit()
      }
    }.frame(width: 14, height: 14).foregroundStyle(.secondary)
  }
}

/// A hub with three spokes and smaller signals between them. Draw as vectors
/// so the sidebar mark stays crisp at every display scale and appearance.
enum CoordinatorMark {
  static let image: NSImage = {
    let image = NSImage(size: NSSize(width: 24, height: 24), flipped: false) { rect in
      NSGraphicsContext.saveGraphicsState()
      defer { NSGraphicsContext.restoreGraphicsState() }
      let transform = NSAffineTransform()
      transform.translateX(by: rect.minX, yBy: rect.minY)
      transform.scaleX(by: rect.width / 24, yBy: rect.height / 24)
      transform.concat()
      NSColor.black.setStroke()
      func point(_ radius: CGFloat, _ degrees: CGFloat) -> NSPoint {
        let angle = degrees * .pi / 180
        return NSPoint(x: 12 + radius * cos(angle), y: 12 + radius * sin(angle))
      }
      let branches = NSBezierPath()
      branches.lineWidth = 2.8
      for angle: CGFloat in [90, 210, 330] {
        branches.move(to: point(0, angle))
        branches.line(to: point(10, angle))
      }
      branches.stroke()
      let signals = NSBezierPath()
      signals.lineWidth = 2
      for angle: CGFloat in [30, 150, 270] {
        for radius: CGFloat in [5, 9] {
          signals.move(to: point(radius - 1, angle))
          signals.line(to: point(radius + 1, angle))
        }
      }
      signals.stroke()
      return true
    }
    image.isTemplate = true
    image.size = NSSize(width: 14, height: 14)
    image.accessibilityDescription = "Coordinator"
    return image
  }()
}
