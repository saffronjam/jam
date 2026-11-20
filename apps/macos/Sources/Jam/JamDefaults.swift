import Foundation

/// Explicit profiles keep isolated app/upgrade runs out of normal UI preferences.
enum JamDefaults {
  static let shared: UserDefaults = {
    guard let suite = ProcessInfo.processInfo.environment["JAM_DEFAULTS_SUITE"] else {
      return .standard
    }
    precondition(!suite.isEmpty, "JAM_DEFAULTS_SUITE must name an isolated preferences suite")
    guard let defaults = UserDefaults(suiteName: suite) else {
      preconditionFailure("Could not open JAM_DEFAULTS_SUITE")
    }
    return defaults
  }()
}
