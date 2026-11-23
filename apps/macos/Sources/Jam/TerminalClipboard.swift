import AppKit
import ImageIO

enum TerminalClipboard {
  static let maximumImageBytes = 16 * 1024 * 1024

  static func hasImage(_ pasteboard: NSPasteboard) -> Bool {
    pasteboard.availableType(from: [.png, .tiff]) != nil
  }

  static func image(_ pasteboard: NSPasteboard) throws -> Data? {
    guard hasImage(pasteboard) else { return nil }
    if let png = pasteboard.data(forType: .png) {
      guard png.count <= maximumImageBytes else { throw ImageError.tooLarge }
      return png
    }
    guard let tiff = pasteboard.data(forType: .tiff) else { throw ImageError.invalid }
    // Retina screenshots can have a large uncompressed TIFF representation
    // even when their PNG is small enough to transfer.
    guard tiff.count <= 256 * 1024 * 1024 else { throw ImageError.tooLarge }
    guard let source = CGImageSourceCreateWithData(tiff as CFData, nil),
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [String: Any],
      let width = properties[kCGImagePropertyPixelWidth as String] as? Int,
      let height = properties[kCGImagePropertyPixelHeight as String] as? Int,
      width > 0, height > 0
    else { throw ImageError.invalid }
    guard width <= 64_000_000 / height else { throw ImageError.dimensions }
    guard let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
      let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    else { throw ImageError.invalid }
    guard png.count <= maximumImageBytes else { throw ImageError.tooLarge }
    return png
  }

  static func quotedPath(_ path: String) -> String {
    "'" + path.replacingOccurrences(of: "'", with: "'\\''") + "' "
  }

  enum ImageError: LocalizedError {
    case tooLarge, dimensions, invalid
    var errorDescription: String? {
      switch self {
      case .tooLarge: return "Clipboard image is too large. Maximum size is 16 MB"
      case .dimensions: return "Clipboard image dimensions are too large"
      case .invalid: return "Could not read the clipboard image"
      }
    }
  }
}

extension TerminalNSView {
  func pasteImage(_ png: Data) {
    inputQueue.async { [weak self] in
      guard let self else { return }
      let endpoint = WorkspaceSelection.endpoint(of: self.pane)
      let capabilities = rpc("Capabilities", endpoint: endpoint)
      let result: [String: Any]
      if (capabilities["Capabilities"] as? [String])?.contains("clipboard_images") == true {
        result = rpc(["StoreClipboardImage": ["pane": self.pane, "png": png.base64EncodedString()]])
      } else {
        result = [
          "error": capabilities["error"] as? String
            ?? "Image pasting needs an updated Jam server on this machine"
        ]
      }
      var error = result["error"] as? String
      if error == nil {
        if let image = result["ClipboardImage"] as? [String: Any],
          let path = image["path"] as? String
        {
          // Use the same input stream as typing, so the path keeps its place
          // among keystrokes queued before and after this upload.
          error =
            rpc(["Paste": ["pane": self.pane, "text": TerminalClipboard.quotedPath(path)]])["error"]
            as? String
        } else {
          error = "The Jam server did not return a clipboard image path"
        }
      }
      let message = error
      DispatchQueue.main.async { [weak self] in self?.onError?(message) }
    }
  }
}
