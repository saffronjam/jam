import AppKit
import ImageIO
import SwiftUI

struct PreviewFailure: LocalizedError {
  let message: String
  init(_ message: String) { self.message = message }
  var errorDescription: String? { message }
}

struct PreviewReader {
  let pane: String
  var call: ([String: Any]) throws -> [String: Any] = { request in
    let result = rpc(request)
    guard let file = result["FilePreview"] as? [String: Any] else {
      throw PreviewFailure(
        result["error"] as? String ?? result["Error"] as? String ?? "Unable to read file.")
    }
    return file
  }

  func read(_ path: String, _ operation: Any) throws -> [String: Any] {
    try call(["FilePreview": ["pane": pane, "path": path, "read": operation]])
  }

  func inspect(_ path: String) throws -> PreviewInfo {
    let file = try read(path, "Inspect")
    guard let resolved = file["path"] as? String, let size = file["size"] as? Int, size >= 0,
      let version = file["version"] as? String
    else { throw PreviewFailure("Invalid file information.") }
    return PreviewInfo(path: resolved, size: size, version: version)
  }

  func bytes(_ file: PreviewInfo, offset: Int, count: Int) throws -> Data {
    let result = try read(
      file.path, ["Chunk": ["version": file.version, "offset": offset, "length": count]])
    guard let value = result["data"] as? String, let bytes = Data(base64Encoded: value),
      bytes.count == min(count, file.size - offset)
    else { throw PreviewFailure("Incomplete file transfer.") }
    return bytes
  }

  static let pageSize = 64 * 1024
  static let chunkSize = 512 * 1024
  static let markdownLimit = 2 * 1024 * 1024

  static let imageTypes: Set<String> = [
    "public.png", "public.jpeg", "org.webmproject.webp", "com.compuserve.gif", "public.heic",
    "public.heif", "public.avif", "public.tiff", "com.microsoft.bmp",
  ]
  static let imageExtensions: Set<String> = [
    "png", "jpg", "jpeg", "gif", "webp", "heif", "heic", "avif", "tif", "tiff", "bmp",
  ]

  func remainingBytes(
    _ file: PreviewInfo, after prefix: Data,
    checkCancellation: () throws -> Void = { try Task.checkCancellation() }
  ) throws -> Data {
    var data = prefix
    while data.count < file.size {
      try checkCancellation()
      data.append(
        try bytes(file, offset: data.count, count: min(Self.chunkSize, file.size - data.count)))
    }
    return data
  }

  func load(_ location: PreviewLocation, file: PreviewInfo, page: Range<Int>?) throws
    -> PreviewContent
  {
    let ext = (file.path as NSString).pathExtension.lowercased()
    if ext == "mp4" || ext == "mov" {
      return .movie(PreviewMovie(reader: self, file: file, quickTime: ext == "mov"))
    }
    let sample = try bytes(file, offset: 0, count: min(max(file.size, 1), 8192))
    let sourceType =
      CGImageSourceCreateWithData(sample as CFData, nil).flatMap { CGImageSourceGetType($0) }
      as String?
    if sourceType.map(Self.imageTypes.contains) == true || Self.imageExtensions.contains(ext) {
      guard file.size <= 32 * 1024 * 1024 else {
        throw PreviewFailure("Images up to 32 MiB can be previewed.")
      }
      let data = try remainingBytes(file, after: sample)
      return .image(try PreviewImage(data, allowedTypes: Self.imageTypes))
    }
    guard !sample.contains(0), !sample.contains(where: { $0 < 8 || (14...26).contains($0) }) else {
      throw PreviewFailure("This binary file has no preview.")
    }
    var target: Int?
    let start: Int
    if let page, !file.canFormatMarkdown {
      start = page.lowerBound
    } else if let line = location.line {
      let info = try read(file.path, ["Line": ["version": file.version, "number": line]])
      guard let offset = info["offset"] as? Int, (0...file.size).contains(offset) else {
        throw PreviewFailure("Invalid line position.")
      }
      target = offset
      start = file.canFormatMarkdown ? 0 : max(0, offset - Self.pageSize / 4)
    } else if ext == "log"
      || (file.path as NSString).lastPathComponent.range(
        of: #"\.log\.\d+$"#, options: .regularExpression) != nil
    {
      start = max(0, file.size - Self.pageSize)
      target = file.size
    } else {
      start = 0
    }
    let end =
      file.canFormatMarkdown
      ? file.size : min(file.size, page?.upperBound ?? (start + Self.pageSize))
    var data: Data
    if file.canFormatMarkdown {
      data = try remainingBytes(file, after: sample)
    } else if start == 0 && end <= sample.count {
      data = Data(sample.prefix(end))
    } else {
      data = try bytes(file, offset: start, count: max(1, end - start))
    }
    // A page boundary may bisect a UTF-8 character. Adjacent pages start at the adjusted boundary.
    var lower = start
    if start > 0 {
      while let byte = data.first, byte & 0xc0 == 0x80 {
        data.removeFirst()
        lower += 1
      }
    }
    var text = String(data: data, encoding: .utf8)
    if text == nil && end < file.size {
      for _ in 0..<min(3, data.count) {
        data.removeLast()
        if let decoded = String(data: data, encoding: .utf8) {
          text = decoded
          break
        }
      }
    }
    guard let text else { throw PreviewFailure("This file is not UTF-8 text.") }
    var anchor = 0
    if let target {
      let prefix = String(decoding: data.prefix(max(0, target - lower)), as: UTF8.self)
      let remainder = text.utf16.dropFirst(prefix.utf16.count)
      let lineLength = remainder.prefix(while: { $0 != 10 }).count
      anchor = min(
        text.utf16.count, prefix.utf16.count + min(max(0, (location.column ?? 1) - 1), lineLength))
    }
    return .text(PreviewPage(text: text, bytes: lower..<(lower + data.count), anchor: anchor))
  }
}

struct PreviewInfo: Equatable {
  let path: String
  let size: Int
  let version: String
  var isMarkdown: Bool {
    ["md", "markdown"].contains((path as NSString).pathExtension.lowercased())
  }
  var canFormatMarkdown: Bool { isMarkdown && size <= PreviewReader.markdownLimit }
}
struct PreviewPage {
  let text: String
  let bytes: Range<Int>
  let anchor: Int
  let id = UUID()
}
enum PreviewContent {
  case image(PreviewImage)
  case text(PreviewPage)
  case movie(PreviewMovie)
}

enum MarkdownMode { case raw, formatted }

final class FilePreview: ObservableObject, Identifiable {
  let id = UUID()
  let reader: PreviewReader
  let location: PreviewLocation
  @Published var info: PreviewInfo?
  @Published var content: PreviewContent?
  @Published var error: String?
  @Published var busy = false
  var drawerScroll: NSPoint?
  var imageOrigin: NSPoint = .zero
  @Published var drawerActualSize = false
  @Published var markdownMode: MarkdownMode
  var needsRefresh = false
  private var task: Task<Void, Never>?
  private var generation = UUID()

  init(reader: PreviewReader, location: PreviewLocation) {
    self.reader = reader
    self.location = location
    markdownMode = location.line == nil ? .formatted : .raw
  }
  func cancel() {
    generation = UUID()
    task?.cancel()
    task = nil
    if busy { busy = false }
  }
  deinit { task?.cancel() }

  func load(page: Range<Int>? = nil, ifChanged: Bool = false) {
    cancel()
    let token = generation
    let previous = info
    let reader = reader
    let location = location
    busy = true
    error = nil
    task = Task.detached(priority: .userInitiated) { [weak self] in
      do {
        let file = try reader.inspect(previous?.path ?? location.path)
        try Task.checkCancellation()
        let content =
          ifChanged && previous == file
          ? nil : try reader.load(location, file: file, page: previous == file ? page : nil)
        try Task.checkCancellation()
        await MainActor.run { [weak self] in
          guard let self, self.generation == token else { return }
          self.info = file
          if let content {
            self.drawerScroll = nil
            self.content = content
          }
          self.busy = false
        }
      } catch {
        let message = error.localizedDescription
        await MainActor.run { [weak self] in
          guard let self, self.generation == token else { return }
          self.error = message
          self.busy = false
        }
      }
    }
  }
}

extension Store {
  var previewSurface: String? {
    surface == .coordinator ? current?.coordinator?.pane : currentTab?.id
  }

  func preview(_ location: PreviewLocation, from pane: String) {
    guard visiblePaneIDs.contains(pane), let key = previewSurface else { return }
    filePreviews[key]?.cancel()
    let model = FilePreview(reader: PreviewReader(pane: pane), location: location)
    filePreviews[key] = model
    let endpoint = WorkspaceSelection.endpoint(of: pane)
    if let caps = capabilitiesByEndpoint[endpoint] {
      if caps.contains("file_preview") {
        model.load()
      } else {
        model.error = "Update this machine’s Jam server to preview files."
      }
    } else {
      DispatchQueue.global().async {
        let result = rpc("Capabilities", endpoint: endpoint)
        DispatchQueue.main.async {
          guard self.filePreviews[key] === model else { return }
          if let list = result["Capabilities"] as? [String] {
            self.capabilitiesByEndpoint[endpoint] = Set(list)
            if list.contains("file_preview") {
              model.load()
            } else {
              model.error = "Update this machine’s Jam server to preview files."
            }
          } else {
            model.error = result["error"] as? String ?? "Machine unavailable."
          }
        }
      }
    }
  }

  func closePreview() {
    if let key = previewSurface { filePreviews.removeValue(forKey: key)?.cancel() }
    expandedPreview = nil
    focusTerminal()
  }

  func editPreview(_ model: FilePreview) {
    guard let workspace = current, let path = model.info?.path else { return }
    expandedPreview = nil
    model.needsRefresh = true
    submitNewTab(workspace: workspace, editor: ["pane": model.reader.pane, "path": path])
  }
}
