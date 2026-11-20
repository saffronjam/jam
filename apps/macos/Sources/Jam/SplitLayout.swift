import Foundation

/// Keep the entire split tree inside its viewport, including oversized saved
/// layouts. Preferred minimums yield proportionally when the window is smaller.
enum SplitLayout {
  static let leaf = CGSize(width: 180, height: 120)
  static let divider: CGFloat = 4
  static func minimum(_ a: CGSize, _ b: CGSize, vertical: Bool) -> CGSize {
    vertical
      ? CGSize(width: max(a.width, b.width), height: a.height + b.height + divider)
      : CGSize(width: a.width + b.width + divider, height: max(a.height, b.height))
  }
  static func canvas(viewport: CGSize) -> CGSize {
    CGSize(width: max(0, viewport.width), height: max(0, viewport.height))
  }
  static func canSplit(_ size: CGSize, vertical: Bool) -> Bool {
    vertical
      ? size.height >= leaf.height * 2 + divider && size.width >= leaf.width
      : size.width >= leaf.width * 2 + divider && size.height >= leaf.height
  }
  static func first(total: CGFloat, ratio: Double, minimumA: CGFloat, minimumB: CGFloat) -> CGFloat
  {
    let available = max(0, total - divider)
    let scale = min(1, available / max(1, minimumA + minimumB))
    return min(available - minimumB * scale, max(minimumA * scale, available * ratio))
  }
}
