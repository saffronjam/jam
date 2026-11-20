import AppKit
import CJam
import SwiftUI

struct WorktreePathPreview {
  let path: String?
  let error: String?
  init(
    template: String, repo: String = "jam", worktree: String = "feature/login",
    home: String = NSHomeDirectory()
  ) {
    let request: [String: String] = [
      "worktree_template": template, "repo": repo, "worktree": worktree, "home": home,
    ]
    let data = try? JSONSerialization.data(withJSONObject: request)
    let result = data.flatMap {
      try? JSONSerialization.jsonObject(
        with: Data(nativeString(jam_config(String(decoding: $0, as: UTF8.self))).utf8))
        as? [String: String]
    }
    path = result?["path"]
    error = result?["error"] ?? (path == nil ? "Could not preview this path" : nil)
  }
}

final class TemplateEditorController: ObservableObject {
  weak var editor: NSTextView?
  func insert(_ variable: String) {
    guard let editor, let window = editor.window else { return }
    window.makeFirstResponder(editor)
    editor.insertText("{{\(variable)}}", replacementRange: editor.selectedRange())
  }
}

/// A real native text editor; coloring never replaces the editable text or caret.
struct TemplateTextInput: NSViewRepresentable {
  @Environment(\.isEnabled) private var isEnabled
  @Binding var text: String
  let controller: TemplateEditorController
  func makeCoordinator() -> Coordinator { Coordinator(text: $text) }
  func makeNSView(context: Context) -> NSScrollView {
    let scroll = NSScrollView()
    scroll.hasHorizontalScroller = false
    scroll.hasVerticalScroller = false
    scroll.horizontalScrollElasticity = .none
    scroll.verticalScrollElasticity = .none
    scroll.drawsBackground = true
    scroll.backgroundColor = .textBackgroundColor
    scroll.wantsLayer = true
    scroll.layer?.cornerRadius = 6
    let editor = NSTextView(frame: NSRect(x: 0, y: 0, width: 400, height: 32))
    editor.delegate = context.coordinator
    editor.isEditable = isEnabled
    editor.isRichText = false
    editor.importsGraphics = false
    editor.allowsUndo = true
    editor.isHorizontallyResizable = true
    editor.isVerticallyResizable = false
    editor.minSize = NSSize(width: 0, height: 32)
    editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: 32)
    editor.textContainer?.widthTracksTextView = false
    editor.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: 32)
    editor.textContainerInset = NSSize(width: 8, height: 7)
    editor.isAutomaticQuoteSubstitutionEnabled = false
    editor.isAutomaticDashSubstitutionEnabled = false
    editor.isAutomaticTextReplacementEnabled = false
    editor.isAutomaticSpellingCorrectionEnabled = false
    editor.isContinuousSpellCheckingEnabled = false
    editor.setAccessibilityLabel("Worktree path template")
    editor.string = text
    editor.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
    editor.autoresizingMask = [.width]
    scroll.documentView = editor
    controller.editor = editor
    context.coordinator.highlight(editor)
    return scroll
  }
  func updateNSView(_ scroll: NSScrollView, context: Context) {
    guard let editor = scroll.documentView as? NSTextView else { return }
    context.coordinator.text = $text
    editor.isEditable = isEnabled
    controller.editor = editor
    if editor.string != text {
      editor.string = text
      if editor.window?.firstResponder !== editor {
        editor.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
      }
      context.coordinator.highlight(editor)
    }
  }
  final class Coordinator: NSObject, NSTextViewDelegate {
    var text: Binding<String>
    init(text: Binding<String>) { self.text = text }
    func textDidChange(_ notification: Notification) {
      guard let editor = notification.object as? NSTextView else { return }
      text.wrappedValue = editor.string
      if !editor.hasMarkedText() { highlight(editor) }
    }
    func textView(
      _ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange,
      replacementString: String?
    ) -> Bool {
      !(replacementString?.contains(where: { $0 == "\n" || $0 == "\r" }) ?? false)
    }
    func highlight(_ editor: NSTextView) {
      guard let storage = editor.textStorage else { return }
      let selection = editor.selectedRanges
      let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
      storage.beginEditing()
      storage.setAttributes(
        [.font: font, .foregroundColor: NSColor.labelColor],
        range: NSRange(location: 0, length: storage.length))
      if let regex = try? NSRegularExpression(
        pattern: "\\{\\{\\s*([A-Za-z_][A-Za-z_0-9]*)\\s*\\}\\}")
      {
        let string = editor.string as NSString
        for match in regex.matches(
          in: editor.string, range: NSRange(location: 0, length: string.length))
        {
          storage.addAttribute(
            .foregroundColor, value: NSColor.secondaryLabelColor, range: match.range)
          let name = string.substring(with: match.range(at: 1))
          storage.addAttribute(
            .foregroundColor,
            value: ["repo", "worktree", "wt"].contains(name)
              ? NSColor.systemBlue : NSColor.systemRed,
            range: match.range(at: 1))
        }
      }
      storage.endEditing()
      editor.typingAttributes = [.font: font, .foregroundColor: NSColor.labelColor]
      editor.selectedRanges = selection
      let width = (editor.string as NSString).size(withAttributes: [.font: font]).width + 24
      editor.setFrameSize(
        NSSize(width: max(editor.enclosingScrollView?.contentSize.width ?? 0, width), height: 32))
      editor.scrollRangeToVisible(editor.selectedRange())
    }
  }
}

struct WorktreeTemplateEditor: View {
  @Binding var template: String
  @StateObject private var controller = TemplateEditorController()
  private var preview: WorktreePathPreview { WorktreePathPreview(template: template) }
  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("Path template")
      TemplateTextInput(text: $template, controller: controller).frame(height: 32)
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Color.primary.opacity(0.15)))
      HStack(spacing: 6) {
        Text("Insert").font(.caption).foregroundStyle(.secondary)
        ForEach(["repo", "worktree"], id: \.self) { variable in
          Button {
            controller.insert(variable)
          } label: {
            Text("{{\(variable)}}").font(.system(.caption, design: .monospaced))
              .foregroundStyle(.blue)
          }.help(
            variable == "repo" ? "Repository folder name" : "Worktree name, derived from its branch"
          )
        }
      }
      if let error = preview.error {
        Text(error).font(.caption).foregroundStyle(.red)
      } else if let path = preview.path {
        VStack(alignment: .leading, spacing: 4) {
          Text("Example · repo: jam · branch: feature/login").font(.caption).foregroundStyle(
            .secondary)
          Text(path).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
            .lineLimit(2).truncationMode(.middle).help(path)
        }
      }
      Text(
        "{{worktree}} replaces / with -. ~ uses each machine’s home directory. Existing worktrees stay where they are."
      )
      .font(.caption).foregroundStyle(.secondary)
    }
  }
}
