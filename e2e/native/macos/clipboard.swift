import AppKit
import Foundation

struct Representation: Codable {
  let type: String
  let data: Data
}

enum ClipboardError: Error {
  case unavailableRepresentation(String)
  case snapshotTooLarge
  case writeFailed
}

let pasteboard = NSPasteboard.general
if CommandLine.arguments.dropFirst().first == "capture" {
  var total = 0
  let items = try (pasteboard.pasteboardItems ?? []).map { item in
    try item.types.map { type in
      guard let data = item.data(forType: type) else {
        throw ClipboardError.unavailableRepresentation(type.rawValue)
      }
      total += data.count
      guard total <= 16 * 1024 * 1024 else { throw ClipboardError.snapshotTooLarge }
      return Representation(type: type.rawValue, data: data)
    }
  }
  FileHandle.standardOutput.write(try JSONEncoder().encode(items))
} else {
  let snapshot = try JSONDecoder().decode(
    [[Representation]].self, from: FileHandle.standardInput.readDataToEndOfFile())
  let items = snapshot.map { representations in
    let item = NSPasteboardItem()
    for value in representations {
      item.setData(value.data, forType: NSPasteboard.PasteboardType(value.type))
    }
    return item
  }
  pasteboard.clearContents()
  if !items.isEmpty && !pasteboard.writeObjects(items) { throw ClipboardError.writeFailed }
}
