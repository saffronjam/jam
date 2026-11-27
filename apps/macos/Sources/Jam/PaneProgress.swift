import SwiftUI

struct PaneProgressStep: Equatable {
  let label: String
  let percent: Int

  init?(record: [String: Any]) {
    guard let label = record["label"] as? String, !label.isEmpty,
      let percent = record["percent"] as? Int, (0...100).contains(percent)
    else { return nil }
    self.label = label
    self.percent = percent
  }
}

struct PaneProgress: View {
  let steps: [PaneProgressStep]
  @State private var showsSteps = false

  static let minimumPaneWidth: CGFloat = 360
  var average: Double {
    guard !steps.isEmpty else { return 0 }
    return Double(steps.reduce(0) { $0 + $1.percent }) / Double(steps.count)
  }

  var body: some View {
    if steps.count > 1 {
      Button {
        showsSteps = true
      } label: {
        ProgressTrack(percent: average).frame(width: 64, height: 24)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Task progress")
      .accessibilityValue("\(Int(average.rounded())) percent across \(steps.count) steps")
      .help("Show task progress")
      .popover(isPresented: $showsSteps, arrowEdge: .bottom) {
        VStack(alignment: .leading, spacing: 10) {
          ForEach(steps, id: \.label) { step in
            HStack(spacing: 14) {
              Text(step.label).font(.caption).foregroundStyle(.secondary).lineLimit(1)
              Spacer(minLength: 0)
              ProgressTrack(percent: Double(step.percent)).frame(width: 76, height: 12)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(step.label)
            .accessibilityValue("\(step.percent) percent")
          }
        }.padding(12).frame(width: 270)
      }
    } else if let step = steps.first {
      HStack(spacing: 6) {
        Text(step.label).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
          .frame(maxWidth: 90, alignment: .trailing)
        ProgressTrack(percent: Double(step.percent)).frame(width: 64, height: 24)
      }
      .accessibilityElement(children: .ignore)
      .accessibilityLabel(step.label)
      .accessibilityValue("\(step.percent) percent")
    }
  }
}

private struct ProgressTrack: View {
  let percent: Double
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.colorScheme) private var colorScheme
  var body: some View {
    GeometryReader { geometry in
      ZStack(alignment: .leading) {
        Capsule().fill(colorScheme == .dark ? Color.white.opacity(0.17) : Color.black.opacity(0.42))
        Capsule().fill(.white).frame(width: geometry.size.width * percent / 100)
          .animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: percent)
      }.frame(height: 3).frame(maxHeight: .infinity)
    }
  }
}
