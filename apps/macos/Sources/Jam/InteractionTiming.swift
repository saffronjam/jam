import Foundation
import os

enum InteractionTiming {
  private static let enabled = ProcessInfo.processInfo.environment["JAM_TRACE_INTERACTIONS"] == "1"
  private static let log = OSSignposter(subsystem: "dev.saffronjam.jam", category: "interaction")

  static func event(_ name: StaticString) {
    if enabled { log.emitEvent(name) }
  }

  static func measure<T>(_ name: StaticString, _ operation: () -> T) -> T {
    guard enabled else { return operation() }
    let interval = log.beginInterval(name, id: log.makeSignpostID())
    defer { log.endInterval(name, interval) }
    return operation()
  }
}
