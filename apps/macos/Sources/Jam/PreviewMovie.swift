import AVKit
import SwiftUI
import UniformTypeIdentifiers

/// AVFoundation asks for container metadata and media ranges independently, including after a seek.
private final class MovieBytes: NSObject, AVAssetResourceLoaderDelegate {
  let queue = DispatchQueue(label: "jam.preview.movie", qos: .userInitiated)
  let reader: PreviewReader
  let file: PreviewInfo
  let url: URL
  let type: UTType

  init(reader: PreviewReader, file: PreviewInfo, quickTime: Bool) {
    self.reader = reader
    self.file = file
    type = quickTime ? .quickTimeMovie : .mpeg4Movie
    url = URL(string: "jam-movie://preview/\(UUID().uuidString).\(quickTime ? "mov" : "mp4")")!
  }

  func resourceLoader(
    _ resourceLoader: AVAssetResourceLoader,
    shouldWaitForLoadingOfRequestedResource request: AVAssetResourceLoadingRequest
  ) -> Bool {
    guard request.request.url == url else { return false }
    if let info = request.contentInformationRequest {
      info.contentType = type.identifier
      info.contentLength = Int64(file.size)
      info.isByteRangeAccessSupported = true
    }
    guard let data = request.dataRequest else {
      request.finishLoading()
      return true
    }
    guard let start = Int(exactly: data.requestedOffset), (0...file.size).contains(start),
      data.requestedLength >= 0
    else {
      request.finishLoading(with: PreviewFailure("Invalid movie byte range."))
      return true
    }
    let end =
      data.requestsAllDataToEndOfResource
      ? file.size : start + min(data.requestedLength, file.size - start)
    send(request, data: data, end: end)
    return true
  }

  private func send(
    _ request: AVAssetResourceLoadingRequest, data: AVAssetResourceLoadingDataRequest, end: Int
  ) {
    guard !request.isCancelled, !request.isFinished else { return }
    let offset = Int(data.currentOffset)
    guard offset < end else {
      request.finishLoading()
      return
    }
    do {
      let bytes = try reader.bytes(
        file, offset: offset, count: min(PreviewReader.chunkSize, end - offset))
      guard !request.isCancelled else { return }
      data.respond(with: bytes)
      // Yield between RPCs so cancellation and metadata/seek requests get a turn.
      queue.async { self.send(request, data: data, end: end) }
    } catch {
      if !request.isCancelled { request.finishLoading(with: error) }
    }
  }
}

final class PreviewMovie: ObservableObject {
  let player: AVPlayer
  private let source: MovieBytes
  private var observation: NSKeyValueObservation?
  @Published private(set) var error: String?

  init(reader: PreviewReader, file: PreviewInfo, quickTime: Bool) {
    source = MovieBytes(reader: reader, file: file, quickTime: quickTime)
    let asset = AVURLAsset(
      url: source.url,
      options: [
        AVURLAssetReferenceRestrictionsKey: AVAssetReferenceRestrictions.forbidAll.rawValue
      ])
    asset.resourceLoader.setDelegate(source, queue: source.queue)
    let item = AVPlayerItem(asset: asset)
    player = AVPlayer(playerItem: item)
    observation = item.observe(\.status, options: [.initial, .new]) { [weak self] item, _ in
      guard item.status == .failed else { return }
      let message = item.error?.localizedDescription ?? "This movie cannot be played on this Mac."
      DispatchQueue.main.async { self?.error = message }
    }
  }
}

struct PreviewMovieView: View {
  @ObservedObject var movie: PreviewMovie

  var body: some View {
    if let error = movie.error {
      Text(error).foregroundStyle(.secondary).padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    } else {
      MovieControls(player: movie.player)
    }
  }
}

private struct MovieControls: NSViewRepresentable {
  let player: AVPlayer
  func makeNSView(context: Context) -> AVPlayerView {
    let view = AVPlayerView()
    view.controlsStyle = .floating
    view.videoGravity = .resizeAspect
    view.player = player
    return view
  }
  func updateNSView(_ view: AVPlayerView, context: Context) {
    if view.player !== player { view.player = player }
  }
  static func dismantleNSView(_ view: AVPlayerView, coordinator: ()) { view.player = nil }
}
