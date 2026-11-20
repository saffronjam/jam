import Foundation

struct AgentModelSettingsTests {
  static func run() throws {
    var settings = AgentModelSettings()
    assert(settings.effective(\.coordinator, fallback: "provider-default") == "provider-default")
    settings.coordinator = "coordinator-model"
    settings.worker = "worker-model"
    assert(settings.effective(\.worker, fallback: "fallback") == "worker-model")
    assert(settings.effective(\.complexWork, fallback: "fallback") == "worker-model")
    settings.complexWork = "complex-model"
    assert(settings.effective(\.complexWork, fallback: "fallback") == "complex-model")
    settings.worker = "  "
    assert(settings.effective(\.worker, fallback: nil) == "Provider default")
    let encoded = try JSONEncoder().encode(settings)
    let decoded = try JSONDecoder().decode(AgentModelSettings.self, from: encoded)
    assert(decoded == settings)
    print("Coordination models passed: role overrides, complex-work fallback, and blank defaults.")
  }
}
