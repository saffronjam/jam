import Foundation

struct GitSummary: Equatable {
  let added: Int
  let removed: Int
  var review: ReviewRequest? = nil
  static func compact(_ n: Int) -> String {
    let unit = n >= 1_000_000 ? 1_000_000 : (n >= 1_000 ? 1_000 : 1)
    guard unit > 1 else { return String(n) }
    let value = Double(n) / Double(unit)
    let number = value < 10 ? String(format: "%.1f", value) : String(format: "%.0f", value)
    return number.replacingOccurrences(of: ".0", with: "") + (unit == 1_000 ? "K" : "M")
  }
}

struct ReviewRequest: Decodable, Equatable {
  let provider: String
  let number: Int
  let url: String
  let title: String
  let draft: Bool
  var label: String { (provider == "github" ? "#" : "!") + String(number) }
  var displaySignature: String {
    [provider, String(number), url, title, String(draft)].joined(separator: "\u{1F}")
  }
  var tooltip: String {
    "\(provider == "github" ? "Pull request" : "Merge request") \(label)\(draft ? " (draft)" : "") — \(title)"
  }
  var link: URL? {
    guard ["github", "gitlab"].contains(provider), number > 0,
      let result = URL(string: url), ["https", "http"].contains(result.scheme),
      result.host != nil
    else { return nil }
    return result
  }
}
