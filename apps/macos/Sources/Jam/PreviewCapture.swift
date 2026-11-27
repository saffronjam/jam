import AppKit
import ScreenCaptureKit
import SwiftUI

/// Documentation-only capture of the real, isolated preview window.
enum PreviewCapture {
  private static var started = false
  private static var captureWindow: NSWindow?
  static func installIfRequested() {
    let args = CommandLine.arguments
    guard !started, args.contains("--preview"),
      let index = args.firstIndex(of: "--capture-demo"), index + 1 < args.count
    else { return }
    started = true
    DispatchQueue.main.asyncAfter(deadline: .now() + 20) {
      fputs("Documentation capture timed out\n", stderr)
      NSApp.terminate(nil)
    }
    let destination = URL(fileURLWithPath: args[index + 1])
    DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
      let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 1200, height: 760),
        styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
        backing: .buffered, defer: false)
      window.titleVisibility = .hidden
      window.titlebarAppearsTransparent = true
      window.toolbarStyle = .unifiedCompact
      window.appearance = NSAppearance(named: .darkAqua)
      let store = Store()
      for other in NSApp.windows { other.orderOut(nil) }
      window.contentView = NSHostingView(
        rootView: ContentView().environmentObject(store).jamTheme(store.theme)
          .preferredColorScheme(.dark))
      captureWindow = window
      window.center()
      window.makeKeyAndOrderFront(nil)
      NSApp.activate(ignoringOtherApps: true)
      DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
        Task { @MainActor in
          do {
            guard store.error.isEmpty else {
              throw NSError(domain: store.error, code: 1)
            }
            let deadline = Date().addingTimeInterval(12)
            while !store.visiblePaneIDs.allSatisfy({ id in
              id.hasPrefix("preview")
                || store.terminalViews.existing(id)?.meta["ready"] as? Bool == true
            }) {
              guard Date() < deadline else {
                throw NSError(
                  domain: "Native demo terminals did not prepare their first frames", code: 1)
              }
              try await Task.sleep(for: .milliseconds(100))
            }
            try await Task.sleep(for: .milliseconds(500))
            guard #available(macOS 14.4, *) else {
              throw NSError(domain: "Demo capture requires macOS 14.4+", code: 1)
            }
            // This API exposes only the current app's captureable windows without TCC.
            let content = try await SCShareableContent.currentProcess
            guard
              let own = content.windows.first(where: {
                $0.windowID == CGWindowID(window.windowNumber)
              })
            else { throw NSError(domain: "Documentation window is not captureable", code: 1) }
            let config = SCStreamConfiguration()
            config.width = 2400
            config.height = 1520
            config.showsCursor = false
            config.ignoreShadowsSingleWindow = true
            let image = try await SCScreenshotManager.captureImage(
              contentFilter: SCContentFilter(desktopIndependentWindow: own), configuration: config)
            guard
              let context = CGContext(
                data: nil, width: image.width, height: image.height,
                bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { throw NSError(domain: "Image context failed", code: 1) }
            let rect = CGRect(x: 0, y: 0, width: image.width, height: image.height)
            context.addPath(
              CGPath(roundedRect: rect, cornerWidth: 28, cornerHeight: 28, transform: nil))
            context.clip()
            context.draw(image, in: rect)
            guard let rounded = context.makeImage(),
              let png = NSBitmapImageRep(cgImage: rounded).representation(
                using: .png, properties: [:])
            else { throw NSError(domain: "PNG encoding failed", code: 1) }
            try png.write(to: destination, options: .atomic)
          } catch { fputs("Screenshot failed: \(error)\n", stderr) }
          NSApp.terminate(nil)
        }
      }
    }
  }
}

extension PreviewCapture {
  /// Capture-only snapshots use real pane UUIDs and the private daemon's native
  /// terminal mirrors. The ordinary --preview fixture remains fully offline.
  static func loadStateIfRequested(into store: Store) -> Bool {
    let args = CommandLine.arguments
    guard let index = args.firstIndex(of: "--capture-state") else { return false }
    do {
      guard args.contains("--capture-demo"), index + 1 < args.count,
        let stateDirectory = ProcessInfo.processInfo.environment["JAM_STATE_DIR"],
        ProcessInfo.processInfo.environment["JAM_CAPTURE_ISOLATED"] == "1",
        ProcessInfo.processInfo.environment["JAM_DEFAULTS_SUITE"] != nil,
        let socket = ProcessInfo.processInfo.environment["JAM_SOCKET"],
        URL(fileURLWithPath: socket).deletingLastPathComponent().standardizedFileURL
          == URL(fileURLWithPath: stateDirectory).deletingLastPathComponent().standardizedFileURL
      else {
        throw NSError(domain: "Live capture requires an isolated documentation profile", code: 1)
      }
      let path = URL(fileURLWithPath: args[index + 1]).standardizedFileURL
      guard
        path.deletingLastPathComponent()
          == URL(fileURLWithPath: stateDirectory).standardizedFileURL,
        let value = try JSONSerialization.jsonObject(with: Data(contentsOf: path))
          as? [String: Any],
        let state = value["State"] as? [String: Any],
        let panes = state["panes"] as? [[String: Any]], !panes.isEmpty,
        panes.allSatisfy({ UUID(uuidString: $0["id"] as? String ?? "") != nil })
      else { throw NSError(domain: "Invalid private documentation snapshot", code: 1) }
      store.ready = false
      store.connected = true
      store.apply(state, endpoint: "")
      guard let workspace = value["selected_workspace"] as? String,
        let selected = store.workspaces.first(where: { $0.id == workspace }),
        let tab = selected.tabs.first
      else { throw NSError(domain: "Documentation workspace has no terminal tab", code: 1) }
      store.select(workspace, tab: tab.id, pane: tab.root.ids.first)
      if let diffs = value["git_summaries"] as? [String: [String: Int]] {
        for (id, diff) in diffs {
          store.gitSummaries[id] = GitSummary(
            added: diff["added"] ?? 0, removed: diff["removed"] ?? 0)
        }
      }
      // Public demo labels and sample quota values; never discover real accounts.
      store.agentUsage[""] = [
        AgentUsageReport(
          provider: "codex", profileKey: "demo-codex", accountKey: "demo-codex",
          accountLabel: "Demo account", activeSessions: 1,
          windows: [AgentUsageWindow(key: "weekly", label: "7d", percent: 24, resetsAt: nil)],
          extra: nil, checking: false, checkedAt: nil, lastSuccessAt: nil, error: nil),
        AgentUsageReport(
          provider: "claude", profileKey: "demo-claude", accountKey: "demo-claude",
          accountLabel: "Demo account", activeSessions: 1,
          windows: [
            AgentUsageWindow(key: "five_hour", label: "5h", percent: 31, resetsAt: nil),
            AgentUsageWindow(key: "weekly", label: "7d", percent: 18, resetsAt: nil),
          ],
          extra: nil, checking: false, checkedAt: nil, lastSuccessAt: nil, error: nil),
      ]
    } catch {
      store.error = "Could not load documentation state: \(error)"
      fputs(store.error + "\n", stderr)
    }
    return true
  }
}
