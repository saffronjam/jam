import AppKit
import ImageIO
import SwiftUI

actor PreviewImage {
  private let source: CGImageSource
  let type: String
  let pixels: CGSize
  let frames: Int
  private var still: CGImage?

  init(_ data: Data, allowedTypes: Set<String>) throws {
    guard
      let source = CGImageSourceCreateWithData(
        data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
      let type = CGImageSourceGetType(source) as String?, allowedTypes.contains(type),
      let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
      let w = props[kCGImagePropertyPixelWidth] as? Int,
      let h = props[kCGImagePropertyPixelHeight] as? Int,
      w > 0, h > 0
    else { throw PreviewFailure("Unable to decode this image.") }
    guard w <= 64_000_000 / h else {
      throw PreviewFailure("Images up to 64 megapixels can be previewed.")
    }
    self.source = source
    self.type = type
    let rotated = (5...8).contains(props[kCGImagePropertyOrientation] as? Int ?? 1)
    self.pixels = CGSize(width: rotated ? h : w, height: rotated ? w : h)
    self.frames = type == "com.compuserve.gif" ? CGImageSourceGetCount(source) : 1
  }

  func frame(_ index: Int, size: Int) -> (CGImage, Double)? {
    if let still, max(still.width, still.height) >= size { return (still, 0) }
    let options: [CFString: Any] = [
      kCGImageSourceCreateThumbnailFromImageAlways: true,
      kCGImageSourceCreateThumbnailWithTransform: true, kCGImageSourceThumbnailMaxPixelSize: size,
      kCGImageSourceShouldCacheImmediately: true,
    ]
    guard let image = CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary)
    else {
      return nil
    }
    if frames == 1 {
      if image.width * image.height <= 4_000_000 { still = image }
      return (image, 0)
    }
    let props = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any]
    let gif = props?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
    let seconds =
      (gif?[kCGImagePropertyGIFUnclampedDelayTime] ?? gif?[kCGImagePropertyGIFDelayTime]) as? Double
      ?? 0.1
    return (image, max(0.02, min(60, seconds)))
  }
}

private struct FileImageCanvas: View {
  let image: PreviewImage
  let actualSize: Bool
  let model: FilePreview
  let expanded: Bool
  let visible: Bool
  @Environment(\.displayScale) private var scale
  @State private var bitmap: CGImage?
  @State private var failed = false
  @State private var windowVisible = true

  var body: some View {
    GeometryReader { g in
      let ratio =
        actualSize
        ? 1 / scale
        : min(g.size.width / image.pixels.width, g.size.height / image.pixels.height, 1 / scale)
      let size = CGSize(
        width: max(1, image.pixels.width * ratio), height: max(1, image.pixels.height * ratio))
      let dimension = max(1, Int(ceil(max(size.width, size.height) * scale)))
      Group {
        if let bitmap {
          PreviewImageViewport(
            bitmap: bitmap, size: size, viewport: g.size, model: model, expanded: expanded)
        } else if failed {
          Text("Unable to decode this image.").foregroundStyle(.secondary)
        } else {
          ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
      }
      .background(PreviewVisibility(visible: $windowVisible))
      .task(id: visible && windowVisible ? dimension : 0) {
        guard visible && windowVisible else { return }
        var index = 0
        repeat {
          guard !Task.isCancelled else { return }
          let decoded = await image.frame(index, size: dimension)
          guard !Task.isCancelled else { return }
          guard let (frame, delay) = decoded else {
            failed = true
            return
          }
          bitmap = frame
          if image.frames <= 1 { return }
          do { try await Task.sleep(for: .seconds(delay)) } catch { return }
          index = (index + 1) % image.frames
        } while !Task.isCancelled
      }
    }
  }
}

private struct PreviewImageViewport: NSViewRepresentable {
  let bitmap: CGImage
  let size: CGSize
  let viewport: CGSize
  let model: FilePreview
  let expanded: Bool

  final class Canvas: NSView {
    override var isFlipped: Bool { true }
    let picture = CALayer()
    var initialPosition: CGPoint?
    var drag: CGPoint?
    var observer: NSObjectProtocol?
    override init(frame: NSRect) {
      super.init(frame: frame)
      wantsLayer = true
      picture.contentsGravity = .resize
      picture.actions = ["contents": NSNull(), "position": NSNull(), "bounds": NSNull()]
      layer?.addSublayer(picture)
    }
    required init?(coder: NSCoder) { nil }
    override func mouseDown(with event: NSEvent) {
      guard let clip = enclosingScrollView?.contentView else { return }
      drag = event.locationInWindow
      initialPosition = clip.bounds.origin
    }
    override func mouseDragged(with event: NSEvent) {
      guard let drag, let initialPosition, let clip = enclosingScrollView?.contentView else {
        return
      }
      let now = event.locationInWindow
      let wanted = CGRect(
        origin: CGPoint(
          x: initialPosition.x - (now.x - drag.x), y: initialPosition.y + (now.y - drag.y)),
        size: clip.bounds.size)
      clip.scroll(to: clip.constrainBoundsRect(wanted).origin)
      enclosingScrollView?.reflectScrolledClipView(clip)
    }
    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }
  }
  func makeNSView(context: Context) -> NSScrollView {
    let scroll = NSScrollView()
    scroll.drawsBackground = false
    scroll.hasHorizontalScroller = true
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.scrollerStyle = .overlay
    let canvas = Canvas(frame: .zero)
    scroll.documentView = canvas
    let saved = model.imageOrigin
    DispatchQueue.main.async {
      let clip = scroll.contentView
      clip.scroll(
        to: clip.constrainBoundsRect(CGRect(origin: saved, size: clip.bounds.size)).origin)
      scroll.reflectScrolledClipView(clip)
      if !expanded {
        clip.postsBoundsChangedNotifications = true
        canvas.observer = NotificationCenter.default.addObserver(
          forName: NSView.boundsDidChangeNotification, object: clip, queue: .main
        ) { [weak model, weak clip] _ in
          if let point = clip?.bounds.origin { model?.imageOrigin = point }
        }
      }
    }
    return scroll
  }
  func updateNSView(_ scroll: NSScrollView, context: Context) {
    guard let canvas = scroll.documentView as? Canvas else { return }
    let document = CGSize(
      width: max(viewport.width, size.width), height: max(viewport.height, size.height))
    canvas.setFrameSize(document)
    canvas.picture.frame = CGRect(
      x: (document.width - size.width) / 2, y: (document.height - size.height) / 2,
      width: size.width, height: size.height)
    canvas.picture.contents = bitmap
  }
}

private struct PreviewVisibility: NSViewRepresentable {
  @Binding var visible: Bool
  final class View: NSView {
    var change: ((Bool) -> Void)?
    var observer: NSObjectProtocol?
    override func viewDidMoveToWindow() {
      super.viewDidMoveToWindow()
      if let observer {
        NotificationCenter.default.removeObserver(observer)
        self.observer = nil
      }
      func update() { change?(window?.occlusionState.contains(.visible) == true) }
      if let window {
        observer = NotificationCenter.default.addObserver(
          forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main
        ) { [weak self] _ in
          self?.change?(self?.window?.occlusionState.contains(.visible) == true)
        }
      }
      update()
    }
    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }
  }
  func makeNSView(context: Context) -> View {
    let view = View()
    view.change = { value in DispatchQueue.main.async { visible = value } }
    return view
  }
  func updateNSView(_ view: View, context: Context) {}
}

private struct FileTextCanvas: NSViewRepresentable {
  let page: PreviewPage
  let model: FilePreview
  let expanded: Bool

  final class Coordinator {
    var id: UUID?
    var observer: NSObjectProtocol?
    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }
  }
  func makeCoordinator() -> Coordinator { Coordinator() }
  func makeNSView(context: Context) -> NSScrollView {
    let scroll = NSTextView.scrollableTextView()
    let text = scroll.documentView as! NSTextView
    text.isEditable = false
    text.isSelectable = true
    text.isRichText = false
    text.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
    text.textContainerInset = NSSize(width: 6, height: 8)
    text.drawsBackground = false
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    return scroll
  }
  func updateNSView(_ scroll: NSScrollView, context: Context) {
    let coordinator = context.coordinator
    guard coordinator.id != page.id, let text = scroll.documentView as? NSTextView else { return }
    coordinator.id = page.id
    if let observer = coordinator.observer {
      NotificationCenter.default.removeObserver(observer)
      coordinator.observer = nil
    }
    let position = model.drawerScroll
    text.string = page.text
    text.textColor = .labelColor
    DispatchQueue.main.async {
      guard coordinator.id == page.id else { return }
      if let position {
        scroll.contentView.scroll(to: position)
        scroll.reflectScrolledClipView(scroll.contentView)
      } else {
        let target = NSRange(location: min(page.anchor, (page.text as NSString).length), length: 0)
        text.setSelectedRange(target)
        text.scrollRangeToVisible(target)
      }
      if !expanded {
        scroll.contentView.postsBoundsChangedNotifications = true
        coordinator.observer = NotificationCenter.default.addObserver(
          forName: NSView.boundsDidChangeNotification, object: scroll.contentView, queue: .main
        ) { [weak scroll, weak model] _ in
          model?.drawerScroll = scroll?.contentView.bounds.origin
        }
      }
    }
  }
}

struct FilePreviewView: View {
  @ObservedObject var model: FilePreview
  @ObservedObject var store: Store
  var expanded = false
  @State private var overlayActualSize = false
  @State private var markdownError: String?
  private var actualSize: Bool { expanded ? overlayActualSize : model.drawerActualSize }
  var body: some View {
    VStack(spacing: 0) {
      HStack(spacing: 8) {
        Text(((model.info?.path ?? model.location.path) as NSString).lastPathComponent)
          .fontWeight(.medium).lineLimit(1).truncationMode(.middle)
          .help(model.info?.path ?? model.location.path)
        Spacer(minLength: 0)
        if case .text = model.content, model.info?.canFormatMarkdown == true {
          Picker("Markdown view", selection: $model.markdownMode) {
            Text("Raw").tag(MarkdownMode.raw)
            Text("Formatted").tag(MarkdownMode.formatted)
          }.pickerStyle(.segmented).labelsHidden().fixedSize().controlSize(.small)
        }
        if case .image = model.content {
          Button {
            if expanded { overlayActualSize.toggle() } else { model.drawerActualSize.toggle() }
          } label: {
            Text(actualSize ? "Fit" : "100%")
          }
          .help(actualSize ? "Fit image" : "View actual pixels")
        }
        if case .text = model.content {
          Button {
            store.editPreview(model)
          } label: {
            Image(systemName: "square.and.pencil")
          }
          .accessibilityLabel("Edit in terminal").help("Edit in terminal")
        }
        if !expanded {
          Button {
            store.expandedPreview = model
          } label: {
            Image(systemName: "arrow.up.left.and.arrow.down.right")
          }
          .accessibilityLabel("Expand preview").help("Expand preview")
        }
        Button {
          if expanded { store.expandedPreview = nil } else { store.closePreview() }
        } label: {
          Image(systemName: "xmark")
        }.accessibilityLabel("Close preview").help("Close preview")
      }.buttonStyle(.plain).padding(10)
      Divider()
      if let error = model.error {
        VStack(spacing: 8) {
          Text(error).foregroundStyle(.secondary)
          if store.capabilitiesByEndpoint[WorkspaceSelection.endpoint(of: model.reader.pane)]?
            .contains("file_preview") == true
          {
            Button("Reload") { model.load() }
          }
        }.padding().frame(maxWidth: .infinity, maxHeight: .infinity)
      } else if let content = model.content {
        switch content {
        case .text(let page):
          if model.info?.canFormatMarkdown == true, model.markdownMode == .formatted {
            if let markdownError {
              Text(markdownError).foregroundStyle(.secondary).padding()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let file = model.info, expanded || store.expandedPreview == nil {
              PreviewMarkdown(page: page, file: file, reader: model.reader) { path in
                store.preview(PreviewLocation(path: path), from: model.reader.pane)
                if expanded, let key = store.previewSurface {
                  store.expandedPreview = store.filePreviews[key]
                }
              } onError: {
                markdownError = $0
              }
            } else {
              Color.clear
            }
          } else {
            if model.info?.isMarkdown == true, model.info?.canFormatMarkdown == false {
              Text("Markdown over 2 MiB opens in Raw view.").font(.caption).foregroundStyle(
                .secondary)
            }
            FileTextCanvas(page: page, model: model, expanded: expanded)
          }
          if let size = model.info?.size, page.bytes.count < size {
            HStack {
              Button("Earlier") {
                model.load(
                  page: max(
                    0, page.bytes.lowerBound - PreviewReader.pageSize)..<page.bytes.lowerBound)
              }
              .disabled(page.bytes.lowerBound == 0)
              Spacer()
              Button("Later") {
                model.load(
                  page: page.bytes
                    .upperBound..<min(size, page.bytes.upperBound + PreviewReader.pageSize))
              }
              .disabled(page.bytes.upperBound >= size)
            }.disabled(model.busy).padding(8)
          }
        case .image(let image):
          FileImageCanvas(
            image: image, actualSize: actualSize, model: model, expanded: expanded,
            visible: expanded || store.expandedPreview == nil
          )
          .id(ObjectIdentifier(image))
        case .movie(let movie):
          if expanded || store.expandedPreview == nil {
            PreviewMovieView(movie: movie)
          } else {
            Color.clear
          }
        }
      } else {
        ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
      }
    }
    .background(Color(nsColor: store.theme.background))
    .onChange(of: model.markdownMode) { _, _ in markdownError = nil }
    .onDisappear {
      if !expanded, store.expandedPreview !== model, case .movie(let movie) = model.content {
        movie.player.pause()
      }
    }
    .onAppear {
      if !expanded, model.needsRefresh {
        model.needsRefresh = false
        model.load(ifChanged: true)
      }
    }
  }
}
