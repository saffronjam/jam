import SwiftUI
import WebKit

struct PreviewMarkdown: NSViewRepresentable {
  let page: PreviewPage
  let file: PreviewInfo
  let reader: PreviewReader
  var openFile: (String) -> Void
  var onError: (String) -> Void
  @Environment(\.colorScheme) private var scheme

  nonisolated static let document: URL? = {
    if Bundle.main.bundleURL.pathExtension == "app" {
      return Bundle.main.url(
        forResource: "preview", withExtension: "html", subdirectory: "file-preview")
    }
    var source = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { source.deleteLastPathComponent() }
    let file = source.appendingPathComponent("assets/file-preview/preview.html")
    return FileManager.default.fileExists(atPath: file.path) ? file : nil
  }()

  final class Coordinator: NSObject, WKNavigationDelegate {
    var ready = false
    var page: PreviewPage?
    var path = ""
    var dark = false
    var onError: ((String) -> Void)?
    var openFile: ((String) -> Void)?

    func render(in view: WKWebView) {
      guard ready, let page else { return }
      view.callAsyncJavaScript(
        "await renderMarkdown(source, dark, path); return true",
        arguments: ["source": page.text, "dark": dark, "path": path],
        in: nil, in: .page
      ) { [weak self] result in
        guard self?.page?.id == page.id else { return }
        if case .failure(let error) = result { self?.onError?(error.localizedDescription) }
      }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
      ready = true
      render(in: webView)
    }

    func webView(
      _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
      withError error: Error
    ) {
      onError?(error.localizedDescription)
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
      onError?("The Markdown renderer stopped.")
    }

    func webView(
      _ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
      decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
      if action.navigationType == .linkActivated, let url = action.request.url {
        if let web = TerminalTarget.web(url) {
          NSWorkspace.shared.open(web)
        } else if url.scheme == "jam-file", url.host == "preview", url.path.hasPrefix("/") {
          openFile?(url.path)
        }
      }
      let initialPage =
        !ready && action.targetFrame?.isMainFrame == true
        && action.request.url == PreviewMarkdown.document
      decisionHandler(initialPage ? .allow : .cancel)
    }
  }

  func makeCoordinator() -> Coordinator { Coordinator() }
  func makeNSView(context: Context) -> WKWebView {
    let config = WKWebViewConfiguration()
    config.websiteDataStore = .nonPersistent()
    config.setURLSchemeHandler(
      MarkdownImages(reader: reader, document: file.path), forURLScheme: "jam-image")
    let view = WKWebView(frame: .zero, configuration: config)
    view.setValue(false, forKey: "drawsBackground")
    view.navigationDelegate = context.coordinator
    if let document = Self.document {
      view.loadFileURL(document, allowingReadAccessTo: document.deletingLastPathComponent())
    } else {
      DispatchQueue.main.async { onError("Markdown preview resources are missing.") }
    }
    return view
  }
  func updateNSView(_ view: WKWebView, context: Context) {
    let state = context.coordinator
    state.onError = onError
    state.openFile = openFile
    state.path = file.path
    guard state.page?.id != page.id || state.dark != (scheme == .dark) else { return }
    state.page = page
    state.dark = scheme == .dark
    state.render(in: view)
  }
  static func dismantleNSView(_ view: WKWebView, coordinator: Coordinator) {
    coordinator.onError = nil
    view.navigationDelegate = nil
    view.stopLoading()
  }
}
