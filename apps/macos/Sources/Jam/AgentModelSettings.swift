import Foundation

struct AgentModelSettings: Codable, Equatable {
  var coordinator = ""
  var worker = ""
  var reviewer = ""
  var complexWork = ""
  enum CodingKeys: String, CodingKey {
    case coordinator, worker, reviewer
    case complexWork = "complex_work"
  }
  init(
    coordinator: String = "", worker: String = "", reviewer: String = "", complexWork: String = ""
  ) {
    self.coordinator = coordinator
    self.worker = worker
    self.reviewer = reviewer
    self.complexWork = complexWork
  }
  init(from decoder: Decoder) throws {
    let values = try decoder.container(keyedBy: CodingKeys.self)
    coordinator = try values.decodeIfPresent(String.self, forKey: .coordinator) ?? ""
    worker = try values.decodeIfPresent(String.self, forKey: .worker) ?? ""
    reviewer = try values.decodeIfPresent(String.self, forKey: .reviewer) ?? ""
    complexWork = try values.decodeIfPresent(String.self, forKey: .complexWork) ?? ""
  }
  static let fields: [(String, WritableKeyPath<Self, String>)] = [
    ("Coordinator", \.coordinator), ("Worker", \.worker), ("Reviewer", \.reviewer),
    ("Complex work", \.complexWork),
  ]
  func effective(_ field: WritableKeyPath<Self, String>, fallback: String?) -> String {
    let choice = self[keyPath: field].trimmingCharacters(in: .whitespacesAndNewlines)
    if !choice.isEmpty { return choice }
    if field == \.complexWork, !worker.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return worker.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    return fallback ?? "Provider default"
  }
}
struct AgentModelCatalog: Decodable, Equatable {
  struct Model: Decodable, Equatable, Identifiable {
    let id: String
    let name: String
    let efforts: [String]
    enum CodingKeys: String, CodingKey {
      case id = "model"
      case name, efforts
    }
  }
  let provider: String
  let models: [Model]
  let defaultModel: String?
  enum CodingKeys: String, CodingKey {
    case provider, models
    case defaultModel = "default_model"
  }
}
