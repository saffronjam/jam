import AVFoundation
import ImageIO
import SwiftUI

private enum PreviewFeature: String, CaseIterable {
  case text = "Text"
  case images = "Images"
  case markdown = "Formatted Markdown"
  case mermaid = "Mermaid diagrams"
  case mp4 = "MP4 playback"
  case mov = "MOV playback"
}

struct PreviewIntegrations: View {
  @ObservedObject var store: Store
  @State private var endpoint = ""
  @State private var refresh = 0
  @State private var failuresByMachine: [String: [PreviewFeature: String]] = [:]
  @State private var checking = false
  @State private var showSpinner = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        if !store.remotes.isEmpty {
          Picker("Machine", selection: $endpoint) {
            Text("Local").tag("")
            ForEach(store.remotes) { remote in Text(remote.name).tag(remote.id) }
          }.fixedSize()
        }
        Spacer()
        Color.clear.frame(width: 16, height: 16).overlay {
          if showSpinner {
            ProgressView().controlSize(.small).accessibilityLabel("Checking availability")
          }
        }
        Button {
          refresh += 1
        } label: {
          Image(systemName: "arrow.clockwise")
        }.help("Check again").accessibilityLabel("Check again")
      }
      let failures = failuresByMachine[endpoint]
      ForEach(PreviewFeature.allCases, id: \.self) { feature in
        HStack {
          Text(feature.rawValue)
          Spacer()
          ZStack(alignment: .trailing) {
            if let reason = failures?[feature] {
              Text(reason).italic().foregroundStyle(.secondary).lineLimit(1)
                .id(reason).transition(.opacity)
            } else if failures != nil {
              Image(systemName: "checkmark").foregroundStyle(.green).accessibilityLabel("Available")
                .transition(.opacity)
            }
          }
          .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: failures)
        }
      }
    }
    .task(id: "\(endpoint)/\(refresh)") {
      let machine = endpoint
      checking = true
      let failures = await Task.detached(priority: .utility) { Self.check(machine) }.value
      guard !Task.isCancelled else { return }
      failuresByMachine[machine] = failures
      checking = false
    }
    .task(id: checking) {
      if checking {
        do { try await Task.sleep(for: .milliseconds(250)) } catch { return }
      }
      showSpinner = checking
    }
  }

  nonisolated private static func check(_ machine: String) -> [PreviewFeature: String] {
    let capabilities = rpc("Capabilities", endpoint: machine)["Capabilities"] as? [String]
    guard capabilities?.contains("file_preview") == true else {
      let reason = capabilities == nil ? "Machine unavailable" : "Update the Jam server"
      return Dictionary(uniqueKeysWithValues: PreviewFeature.allCases.map { ($0, reason) })
    }
    let resources = PreviewMarkdown.document?.deletingLastPathComponent()
    func installed(_ name: String) -> Bool {
      guard let url = resources?.appendingPathComponent(name) else { return false }
      return FileManager.default.isReadableFile(atPath: url.path)
    }
    let markdown = ["preview.html", "preview.css", "preview.js", "markdown-it.min.js"].allSatisfy(
      installed)
    let imageTypes = CGImageSourceCopyTypeIdentifiers() as? [String] ?? []
    let movies = AVURLAsset.audiovisualTypes()
    let reasons: [PreviewFeature: String?] = [
      .images: imageTypes.contains("public.png") ? nil : "Image decoder unavailable",
      .markdown: markdown ? nil : "Markdown renderer missing",
      .mermaid: markdown && installed("mermaid.min.js") ? nil : "Diagram renderer missing",
      .mp4: movies.contains(.mp4) ? nil : "MP4 playback unavailable",
      .mov: movies.contains(.mov) ? nil : "MOV playback unavailable",
    ]
    return reasons.compactMapValues { $0 }
  }
}
