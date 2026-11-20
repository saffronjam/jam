import Foundation

struct WorktreeCheckout: Decodable, Identifiable, Equatable {
  let path: String
  let branch: String
  let main: Bool
  let locked: Bool
  var id: String { path }
}

struct WorktreeChoices {
  let checkouts: [WorktreeCheckout]
  let query: String
  let opened: Set<String>
  let filtered: [WorktreeCheckout]
  let available: [WorktreeCheckout]
  private let openedIDs: Set<String>

  init(checkouts: [WorktreeCheckout], query: String, opened: Set<String>) {
    self.checkouts = checkouts
    self.query = query
    self.opened = opened
    let openedIDs = Set(checkouts.filter { opened.contains(Self.normalized($0.path)) }.map(\.id))
    self.openedIDs = openedIDs
    let words = query.split(whereSeparator: \.isWhitespace)
    let filtered =
      words.isEmpty
      ? checkouts
      : checkouts.filter { checkout in
        let text = checkout.branch + " " + checkout.path
        return words.allSatisfy { text.localizedCaseInsensitiveContains($0) }
      }
    self.filtered = filtered
    available = filtered.filter { !openedIDs.contains($0.id) }
  }

  static func normalized(_ path: String) -> String {
    URL(fileURLWithPath: path).standardizedFileURL.path
  }
  func isOpen(_ checkout: WorktreeCheckout) -> Bool { openedIDs.contains(checkout.id) }
  func selection(_ current: String?) -> String? {
    available.first { $0.id == current }?.id ?? available.first?.id
  }
  func move(_ current: String?, by direction: Int) -> String? {
    let choices = available
    guard !choices.isEmpty else { return nil }
    guard let index = choices.firstIndex(where: { $0.id == current }) else {
      return direction < 0 ? choices.last?.id : choices.first?.id
    }
    return choices[min(choices.count - 1, max(0, index + direction))].id
  }
}
