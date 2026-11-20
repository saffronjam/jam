import AppKit
import Foundation
let args = CommandLine.arguments
let source = URL(fileURLWithPath: args[1])
guard let image = NSImage(contentsOf: source) else { fatalError("Cannot render SVG") }
let directory = URL(fileURLWithPath: args[2])
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
for size in [16, 32, 64, 128, 256, 512, 1024] {
  guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { fatalError("Bitmap") }
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
  image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
  NSGraphicsContext.restoreGraphicsState()
  let data = bitmap.representation(using: .png, properties: [:])!
  if size <= 512 { try data.write(to: directory.appendingPathComponent("icon_\(size)x\(size).png")) }
  if size >= 32 { try data.write(to: directory.appendingPathComponent("icon_\(size / 2)x\(size / 2)@2x.png")) }
}
