import CoreGraphics
import Foundation

struct SplitLayoutTests {
  static func run() throws {
    // Alternating nested splits must fit all leaves, including very skewed ratios.
    let pair = SplitLayout.minimum(SplitLayout.leaf, SplitLayout.leaf, vertical: false)
    let nested = SplitLayout.minimum(pair, SplitLayout.leaf, vertical: true)
    assert(nested == CGSize(width: 364, height: 244))
    let canvas = nested
    assert(
      SplitLayout.canvas(viewport: CGSize(width: 100, height: 80)) == CGSize(width: 100, height: 80)
    )
    for ratio in [0.1, 0.5, 0.9] {
      let first = SplitLayout.first(
        total: canvas.height, ratio: ratio, minimumA: pair.height, minimumB: SplitLayout.leaf.height
      )
      assert(first >= pair.height)
      assert(canvas.height - SplitLayout.divider - first >= SplitLayout.leaf.height)
      let width = SplitLayout.first(total: canvas.width, ratio: ratio, minimumA: 180, minimumB: 180)
      assert(width >= 180 && canvas.width - SplitLayout.divider - width >= 180)
    }
    assert(
      SplitLayout.canvas(viewport: CGSize(width: 1000, height: 800))
        == CGSize(width: 1000, height: 800))
    // Undersized saved layouts and small windows must never produce out-of-bounds children.
    for total in [0.0, 2, 20, 80, 120, 500] {
      for ratio in [0.1, 0.5, 0.9] {
        let first = SplitLayout.first(total: total, ratio: ratio, minimumA: 120, minimumB: 364)
        let second = max(0, total - SplitLayout.divider - first)
        assert(first >= 0 && first <= total)
        assert(second >= 0 && second <= total)
        assert(first + second <= total)
      }
    }
    assert(SplitLayout.canSplit(CGSize(width: 400, height: 240), vertical: false))
    assert(!SplitLayout.canSplit(CGSize(width: 400, height: 240), vertical: true))
    assert(SplitLayout.canSplit(CGSize(width: 180, height: 244), vertical: true))
    assert(!SplitLayout.canSplit(CGSize(width: 360, height: 120), vertical: false))
    print(
      "Split layout checks passed: nested minima, bounded small viewports, skewed ratios, and split limits."
    )
  }
}
