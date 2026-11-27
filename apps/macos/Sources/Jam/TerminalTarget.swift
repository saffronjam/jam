import AppKit

struct PreviewLocation: Equatable {
  let path: String
  var line: Int?
  var column: Int?

  var explicit: Bool {
    path.contains("/") || path.contains(".") || line != nil
      || ["README", "LICENSE", "Makefile", "Dockerfile", "justfile"].contains(path)
  }
}

struct TerminalTarget: Equatable {
  enum Destination: Equatable {
    case web(URL)
    case file(PreviewLocation)
  }
  let destination: Destination
  let cells: Range<Int>
  let columns: Int
  var fallbackURL: URL?

  private static let links = try? NSDataDetector(
    types: NSTextCheckingResult.CheckingType.link.rawValue)
  // Match quoted names (and any line suffix) as one token. Unquoted backslash-space is literal.
  private static let tokens = try! NSRegularExpression(
    pattern: #"(?:\"[^\"\r\n]+\"|'[^'\r\n]+')(?:\:[0-9]+){0,2}|(?:\\ |[^\s\"'<>\x60])+"#)
  private static let suffix = try! NSRegularExpression(
    pattern: #":([1-9][0-9]*)(?::([1-9][0-9]*))?$"#)

  static func web(_ url: URL?) -> URL? {
    guard let url, ["http", "https"].contains(url.scheme?.lowercased()), url.host?.isEmpty == false
    else { return nil }
    return url
  }

  static func hit(_ context: [String: Any]) -> Self? {
    guard let base = context["start"] as? Int, let cols = context["cols"] as? Int, cols > 0 else {
      return nil
    }
    if let target = context["url"] as? String {
      guard let end = context["end"] as? Int, end > base, let url = URL(string: target) else {
        return nil
      }
      if let web = web(url) {
        return Self(destination: .web(web), cells: base..<end, columns: cols)
      }
      guard url.isFileURL, url.path.hasPrefix("/") else { return nil }
      return Self(
        destination: .file(PreviewLocation(path: url.path)), cells: base..<end, columns: cols)
    }
    guard let line = context["line"] as? String, let offset = context["offset"] as? Int,
      let offsets = context["offsets"] as? [Int]
    else { return nil }
    let text = line as NSString
    let full = NSRange(location: 0, length: text.length)
    func target(_ dest: Destination, _ range: NSRange) -> Self? {
      guard NSLocationInRange(offset, range),
        let first = offsets.firstIndex(where: { $0 >= range.location }),
        let last = offsets.lastIndex(where: { $0 < NSMaxRange(range) }), first <= last
      else { return nil }
      return Self(destination: dest, cells: (base + first)..<(base + last + 1), columns: cols)
    }
    var inferred: (URL, NSRange)?
    for match in links?.matches(in: line, range: full) ?? []
    where NSLocationInRange(offset, match.range) {
      // NSDataDetector also guesses links for filenames. Explicit schemes always win.
      if let url = web(match.url),
        text.substring(with: match.range).lowercased().hasPrefix("http")
          || text.substring(with: match.range).lowercased().hasPrefix("www.")
      {
        return target(.web(url), match.range)
      } else if let url = web(match.url) {
        inferred = (url, match.range)
      }
    }
    guard
      let match = tokens.matches(in: line, range: full).first(where: {
        NSLocationInRange(offset, $0.range)
      })
    else { return nil }
    var range = match.range
    var token = text.substring(with: range)
    if token.first != "\"" && token.first != "'" {
      let starts: Set<Character> = ["(", "[", "{"]
      let ends: Set<Character> = [")", "]", "}", ",", ";", ":"]
      while let c = token.first, starts.contains(c) {
        token.removeFirst()
        range.location += 1
        range.length -= 1
      }
      while let c = token.last, ends.contains(c) {
        token.removeLast()
        range.length -= 1
      }
    }
    var location = PreviewLocation(path: token)
    let raw = token as NSString
    if let m = suffix.firstMatch(in: token, range: NSRange(location: 0, length: raw.length)) {
      location.line = Int(raw.substring(with: m.range(at: 1)))
      if m.range(at: 2).location != NSNotFound {
        location.column = Int(raw.substring(with: m.range(at: 2)))
      }
      token = raw.substring(to: m.range.location)
    }
    if token.count >= 2, token.first == "'" || token.first == "\"", token.first == token.last {
      token.removeFirst()
      token.removeLast()
    }
    if token.lowercased().hasPrefix("file://"), let url = URL(string: token), url.isFileURL {
      token = url.path
    }
    guard !token.isEmpty, token != ".", token != "..", !token.contains("://") else { return nil }
    location = PreviewLocation(
      path: token.replacingOccurrences(of: "\\ ", with: " "), line: location.line,
      column: location.column)
    var result = target(.file(location), range)
    if let (url, matchedRange) = inferred, matchedRange == range { result?.fallbackURL = url }
    return result
  }
}
