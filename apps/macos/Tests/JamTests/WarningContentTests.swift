import Foundation

struct WarningContentTests {
  static func run() throws {
    let short = WarningContent("Remote is unavailable")
    assert(short.summary == "Remote is unavailable")
    assert(short.details == nil, "Short warnings must not offer an empty Details or Copy action")
    assert(WarningContent("Warning\n  \n").details == nil)
    let diagnostic = WarningContent("Connection failed\nPermission denied (publickey).")
    assert(diagnostic.summary.contains("SSH authentication failed"))
    assert(diagnostic.details == "Connection failed\nPermission denied (publickey).")
    let long = String(repeating: "detail ", count: 80)
    assert(WarningContent(long).details == long, "Long messages must remain available in full")
    let mismatch =
      "incompatible jam daemon; finish active work, then explicitly run jam daemon stop before upgrading"
    assert(
      WarningContent(mismatch).summary == "The Jam server needs an update. Update it from Machines."
    )
    assert(!WarningContent(mismatch).summary.contains("Finish active work"))
    assert(WarningContent(mismatch).details == mismatch)
    let timeout = "SSH setup check: ssh: connect to host dev-server port 22:\nOperation timed out"
    assert(WarningContent(timeout).summary.contains("network or VPN"))
    assert(WarningContent(timeout).details == timeout)
    let resume = "Could not resume the saved Codex conversation: no such thread abc"
    assert(WarningContent(resume).summary.contains("could not be resumed"))
    assert(WarningContent(resume).details == resume)
    print("Warning content tests passed: short messages, real diagnostics, and long details.")
  }
}
