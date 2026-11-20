import Foundation
// swift-tools-version: 6.0
import PackageDescription

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
  .deletingLastPathComponent().path
let package = Package(
  name: "Jam", platforms: [.macOS(.v14)],
  targets: [
    .target(name: "CJam", publicHeadersPath: "include"),
    .executableTarget(
      name: "Jam", dependencies: ["CJam"],
      linkerSettings: [
        .unsafeFlags([
          "-L\(ProcessInfo.processInfo.environment["JAM_NATIVE_LIB_DIR"] ?? "\(root)/target/debug")",
          "-ljam_native", "-lc++", "-framework", "Metal", "-framework",
          "QuartzCore", "-framework", "CoreGraphics", "-framework", "CoreText", "-framework",
          "Security", "-framework", "SystemConfiguration",
        ])
      ]),
  ], swiftLanguageModes: [.v5])
