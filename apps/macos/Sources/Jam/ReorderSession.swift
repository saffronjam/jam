import CoreGraphics

/// Freeze sibling centers for one drag, on either axis. Animated items must not
/// become their own next drop targets when their positions or sizes change.
struct ReorderSession {
  private let otherCenters: [CGFloat]
  private let grabOffset: CGFloat
  private let hysteresis: CGFloat = 3
  private(set) var index: Int

  init(centers: [CGFloat], source: Int, grabPosition: CGFloat) {
    precondition(centers.indices.contains(source))
    index = source
    grabOffset = centers[source] - grabPosition
    otherCenters = centers.enumerated().compactMap { $0.offset == source ? nil : $0.element }
  }

  mutating func destination(at pointerPosition: CGFloat) -> Int {
    let center = pointerPosition + grabOffset
    while index < otherCenters.count && center > otherCenters[index] + hysteresis { index += 1 }
    while index > 0 && center < otherCenters[index - 1] - hysteresis { index -= 1 }
    return index
  }
}
