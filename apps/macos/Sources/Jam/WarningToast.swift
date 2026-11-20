import AppKit
import SwiftUI

struct WarningContent {
  let summary: String
  let details: String?
  init(_ message: String) {
    let clean = message.trimmingCharacters(in: .whitespacesAndNewlines)
    let lower = clean.lowercased()
    let friendly: String?
    if lower.contains("incompatible jam daemon") || lower.contains("protocol mismatch") {
      friendly = "The Jam server needs an update. Update it from Machines."
    } else if lower.contains("permission denied (publickey")
      || lower.contains("authentication failed")
    {
      friendly = "SSH authentication failed. Check this machine’s SSH user and key."
    } else if lower.contains("timed out") || lower.contains("operation timeout") {
      friendly = "The remote machine did not respond. Check your network or VPN, then retry."
    } else if lower.contains("connection refused") {
      friendly = "The remote machine refused the connection. Check that SSH is running."
    } else if lower.contains("no route to host") || lower.contains("could not resolve hostname") {
      friendly = "The remote machine could not be reached. Check its address and your network."
    } else if lower.contains("previous conversation was not linked")
      || lower.contains("previous session identity is unknown")
    {
      friendly =
        "Jam could not identify the previous conversation. Choose a saved session or start a new conversation."
    } else if lower.contains("could not resume") || lower.contains("no saved session found") {
      friendly =
        "The saved agent conversation could not be resumed. Choose another saved session or start a new conversation."
    } else if lower.contains("codex did not finish") || lower.contains("codex session service") {
      friendly = "Codex could not start. See Details for the reported error."
    } else if lower.contains("not installed or executable") {
      friendly =
        "The selected agent is not installed on this machine. Install it before starting the coordinator."
    } else if lower.contains("already has a coordinator") {
      friendly = "This workspace already has a coordinator. Open its Coordinator in the sidebar."
    } else {
      friendly = nil
    }
    let first =
      clean.split(separator: "\n", omittingEmptySubsequences: false).first.map(String.init) ?? ""
    let headline = first.hasPrefix("Error: ") ? String(first.dropFirst(7)) : first
    summary = friendly ?? (headline.count > 180 ? String(headline.prefix(180)) + "…" : headline)
    // Keep the original diagnostic intact, including its context chain.
    details = clean.isEmpty || clean == summary ? nil : message

  }
}

/// Native text layout determines the height directly; measuring inside a fixed
/// SwiftUI ScrollView creates a circular proposal that can clip multi-line errors.
struct WarningDetails: NSViewRepresentable {
  let text: String

  func makeNSView(context: Context) -> NSScrollView {
    let scroll = NSScrollView()
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.scrollerStyle = .overlay
    let view = NSTextView()
    view.isEditable = false
    view.isSelectable = true
    view.isRichText = false
    view.drawsBackground = false
    view.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    view.textColor = .labelColor
    view.textContainerInset = .zero
    view.textContainer?.lineFragmentPadding = 0
    view.textContainer?.widthTracksTextView = true
    view.isHorizontallyResizable = false
    view.isVerticallyResizable = true
    view.autoresizingMask = [.width]
    scroll.documentView = view
    return scroll
  }

  func updateNSView(_ scroll: NSScrollView, context: Context) {
    guard let view = scroll.documentView as? NSTextView, view.string != text else { return }
    view.string = text
  }

  func sizeThatFits(_ proposal: ProposedViewSize, nsView scroll: NSScrollView, context: Context)
    -> CGSize?
  {
    guard let width = proposal.width, width > 0,
      let view = scroll.documentView as? NSTextView,
      let container = view.textContainer, let layout = view.layoutManager
    else { return nil }
    view.setFrameSize(NSSize(width: width, height: view.frame.height))
    container.containerSize = NSSize(width: width, height: .greatestFiniteMagnitude)
    layout.ensureLayout(for: container)
    let height = max(20, ceil(layout.usedRect(for: container).height) + 2)
    view.setFrameSize(NSSize(width: width, height: height))
    return CGSize(width: width, height: min(180, height))
  }
}

/// An overlay never participates in pane layout or claims keyboard focus.
struct WarningToast: View {
  let message: String
  let retry: (() -> Void)?
  let dismiss: () -> Void
  @State private var expanded = false

  var body: some View {
    let content = WarningContent(message)
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .top, spacing: 10) {
        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
          .accessibilityHidden(true)
        Text(content.summary).font(.callout).fixedSize(horizontal: false, vertical: true)
        Spacer(minLength: 0)
        Button(action: dismiss) { Image(systemName: "xmark") }
          .buttonStyle(.plain).help("Dismiss warning").accessibilityLabel("Dismiss warning")
      }
      if expanded, let details = content.details {
        WarningDetails(text: details).id(details)
      }
      if retry != nil || content.details != nil {
        HStack(spacing: 8) {
          Spacer()
          if let retry { Button("Retry", action: retry) }
          if content.details != nil {
            Button(expanded ? "Less" : "Details") { expanded.toggle() }
            Button("Copy") {
              NSPasteboard.general.clearContents()
              NSPasteboard.general.setString(message, forType: .string)
            }
          }
        }.buttonStyle(.bordered).controlSize(.small)
      }
    }.padding(14).frame(maxWidth: 360, alignment: .leading)
      .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
      .overlay {
        RoundedRectangle(cornerRadius: 12).strokeBorder(.primary.opacity(0.12), lineWidth: 1)
          .allowsHitTesting(false)
      }
      .shadow(color: .black.opacity(0.18), radius: 12, y: 4)
      .accessibilityElement(children: .contain).accessibilityLabel("Warning")
      .onChange(of: message) { _, _ in
        expanded = false
      }
  }
}

/// Consistent inline errors for sheets: readable headline, complete selectable diagnostic.
struct ErrorNotice: View {
  let message: String
  @State private var expanded = false
  var body: some View {
    let content = WarningContent(message)
    VStack(alignment: .leading, spacing: 8) {
      Label(content.summary, systemImage: "exclamationmark.triangle")
        .font(.callout).foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
      if let details = content.details {
        DisclosureGroup("Details", isExpanded: $expanded) {
          WarningDetails(text: details).frame(maxWidth: .infinity)
          HStack {
            Spacer()
            Button("Copy") {
              NSPasteboard.general.clearContents()
              NSPasteboard.general.setString(message, forType: .string)
            }
          }
        }.font(.caption)
      }
    }.onChange(of: message) { _, _ in expanded = false }
  }
}
