import UniformTypeIdentifiers
import WebKit

/// Expose only raster images beneath the opened document's directory. WebKit never
/// receives filesystem access to that directory, even for a preview on the local Mac.
final class MarkdownImages: NSObject, WKURLSchemeHandler {
  let reader: PreviewReader
  let directory: URL
  private let reads = OperationQueue()
  private var pending: [ObjectIdentifier: Operation] = [:]

  init(reader: PreviewReader, document: String) {
    self.reader = reader
    directory = URL(fileURLWithPath: document).deletingLastPathComponent()
    reads.maxConcurrentOperationCount = 2
    reads.qualityOfService = .utility
  }

  private func image(_ url: URL, operation: Operation) throws -> (Data, String) {
    guard
      let path = URLComponents(url: url, resolvingAgainstBaseURL: false)?
        .queryItems?.first(where: { $0.name == "path" })?.value,
      !path.isEmpty, !path.hasPrefix("/"), !path.contains("\0"),
      PreviewReader.imageExtensions.contains((path as NSString).pathExtension.lowercased())
    else { throw PreviewFailure("Invalid image path.") }
    let prefix = directory.path == "/" ? "/" : directory.path + "/"
    // Inspect resolves dots, aliases and symlinks on the pane's host.
    let file = try reader.inspect(prefix + path)
    guard file.path.hasPrefix(prefix), file.size > 0, file.size <= 8 * 1024 * 1024 else {
      throw PreviewFailure("Image unavailable.")
    }
    let bytes = try reader.remainingBytes(file, after: Data()) {
      if operation.isCancelled { throw CancellationError() }
    }
    let image = try PreviewImage(bytes, allowedTypes: PreviewReader.imageTypes)
    guard let mime = UTType(image.type)?.preferredMIMEType else {
      throw PreviewFailure("Unsupported image.")
    }
    return (bytes, mime)
  }

  func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
    guard let url = urlSchemeTask.request.url else {
      urlSchemeTask.didFailWithError(PreviewFailure("Missing image URL."))
      return
    }
    let key = ObjectIdentifier(urlSchemeTask)
    let read = BlockOperation()
    pending[key] = read
    read.addExecutionBlock { [weak self, weak read] in
      guard let self, let read, !read.isCancelled else { return }
      let result = Result { try self.image(url, operation: read) }
      DispatchQueue.main.async {
        guard self.pending.removeValue(forKey: key) === read else { return }
        switch result {
        case .success(let (bytes, mime)):
          urlSchemeTask.didReceive(
            URLResponse(
              url: url, mimeType: mime, expectedContentLength: bytes.count, textEncodingName: nil))
          urlSchemeTask.didReceive(bytes)
          urlSchemeTask.didFinish()
        case .failure(let error):
          urlSchemeTask.didFailWithError(error)
        }
      }
    }
    reads.addOperation(read)
  }

  func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {
    pending.removeValue(forKey: ObjectIdentifier(urlSchemeTask))?.cancel()
  }
}
