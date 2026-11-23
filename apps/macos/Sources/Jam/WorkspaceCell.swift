import AppKit

/// Rows get their bounds from the outline. Size labels inside those bounds;
/// intrinsic text widths must never enlarge the sidebar's document view.
final class WorkspaceCell: NSView {
  let nameField: NSTextField
  let titleRow = NSView()
  private let icon: NSImageView
  private let branchField: NSTextField?
  private let addedField: NSTextField?
  private let removedField: NSTextField?
  private let reviewButton: NSButton?
  private let reviewURL: URL?
  private let spinner: NSProgressIndicator?
  private let detailField: NSTextField?
  private let leadingInset: CGFloat

  init(
    name: String, branch: String?, tint: NSColor?, symbol: String, diff: GitSummary?,
    loading: Bool = false, leadingInset: CGFloat = 0, detail: String? = nil,
    image: NSImage? = nil
  ) {
    self.leadingInset = leadingInset
    reviewURL = diff?.review?.link
    reviewButton = diff?.review.flatMap { review in
      review.link == nil ? nil : NSButton(title: review.label, target: nil, action: nil)
    }
    nameField = NSTextField(labelWithString: name)
    icon = NSImageView(
      image: image ?? NSImage(systemSymbolName: symbol, accessibilityDescription: nil)!)
    spinner = loading ? NSProgressIndicator() : nil
    detailField = detail.map { NSTextField(labelWithString: $0) }
    branchField = branch.map { NSTextField(labelWithString: $0) }
    if let diff, diff.added > 0 || diff.removed > 0 {
      addedField = NSTextField(labelWithString: "+" + GitSummary.compact(diff.added))
      removedField = NSTextField(labelWithString: "−" + GitSummary.compact(diff.removed))
    } else {
      addedField = nil
      removedField = nil
    }
    super.init(frame: .zero)
    autoresizingMask = [.width, .height]
    wantsLayer = true
    layer?.masksToBounds = true
    icon.contentTintColor = tint ?? .secondaryLabelColor
    nameField.font = .systemFont(ofSize: 12)
    nameField.lineBreakMode = .byTruncatingTail
    addSubview(icon)
    if let spinner {
      icon.isHidden = true
      spinner.style = .spinning
      spinner.controlSize = .small
      spinner.isIndeterminate = true
      spinner.setAccessibilityLabel("Creating workspace")
      addSubview(spinner)
      spinner.startAnimation(nil)
    }
    addSubview(titleRow)
    titleRow.addSubview(nameField)
    if let detailField {
      detailField.font = .systemFont(ofSize: 10)
      detailField.textColor = .tertiaryLabelColor
      detailField.maximumNumberOfLines = 1
      detailField.usesSingleLineMode = true
      detailField.lineBreakMode = .byTruncatingTail
      titleRow.addSubview(detailField)
    }
    if let branchField {
      branchField.font = .systemFont(ofSize: 11)
      branchField.textColor = .secondaryLabelColor
      branchField.maximumNumberOfLines = 1
      branchField.usesSingleLineMode = true
      branchField.lineBreakMode = .byTruncatingTail
      addSubview(branchField)
    }
    for field in [addedField, removedField].compactMap({ $0 }) {
      field.font = .monospacedDigitSystemFont(ofSize: 10, weight: .medium)
      titleRow.addSubview(field)
    }
    if let reviewButton {
      reviewButton.isBordered = false
      reviewButton.bezelStyle = .inline
      reviewButton.font = .monospacedDigitSystemFont(ofSize: 10, weight: .medium)
      reviewButton.contentTintColor = .secondaryLabelColor
      reviewButton.toolTip = diff?.review?.tooltip
      reviewButton.setAccessibilityLabel(diff?.review?.tooltip)
      reviewButton.target = self
      reviewButton.action = #selector(openReview)
      titleRow.addSubview(reviewButton)
    }
    addedField?.textColor = .systemGreen
    removedField?.textColor = .systemRed
    setAccessibilityLabel(name)
  }
  required init?(coder: NSCoder) { fatalError("unsupported") }
  @objc private func openReview() {
    if let reviewURL { NSWorkspace.shared.open(reviewURL) }
  }

  override func setFrameSize(_ newSize: NSSize) {
    super.setFrameSize(newSize)
    // A reused AppKit row can resize without creating another cell.
    needsLayout = true
  }

  override func layout() {
    super.layout()
    let textLeading = leadingInset + 22
    let width = max(0, bounds.width - textLeading)
    let textHeight: CGFloat = branchField == nil ? 16 : 30
    let bottom = floor((bounds.height - textHeight) / 2)
    let nameY = bottom + (branchField == nil ? 0 : 14)
    icon.frame = NSRect(
      x: leadingInset, y: floor((bounds.height - SidebarGeometry.iconSize) / 2),
      width: SidebarGeometry.iconSize, height: SidebarGeometry.iconSize)
    spinner?.frame = icon.frame
    titleRow.frame = NSRect(x: textLeading, y: nameY, width: width, height: 16)
    branchField?.frame = NSRect(x: textLeading, y: bottom, width: width, height: 14)
    var nameWidth = width
    var right = width
    if let addedField, let removedField {
      let added = ceil(addedField.intrinsicContentSize.width)
      let removed = ceil(removedField.intrinsicContentSize.width)
      let countsWidth = added + removed + 10
      let fits = width >= countsWidth + 36
      addedField.isHidden = !fits
      removedField.isHidden = !fits
      if fits {
        right -= removed
        removedField.frame = NSRect(x: right, y: 0, width: removed, height: 16)
        right -= added + 5
        addedField.frame = NSRect(x: right, y: 0, width: added, height: 16)
        right -= 5
      }
    }
    if let reviewButton {
      let reviewWidth = ceil(reviewButton.intrinsicContentSize.width)
      let fits = right >= reviewWidth + 36
      reviewButton.isHidden = !fits
      if fits {
        right -= reviewWidth
        reviewButton.frame = NSRect(x: right, y: 0, width: reviewWidth, height: 16)
        right -= 6
      }
    }
    nameWidth = max(0, right)
    nameField.frame = NSRect(x: 0, y: 0, width: nameWidth, height: 16)
    if let detailField {
      // Keep text clear of the outline's rounded trailing edge. Include a
      // little glyph overhang instead of clipping the final letter.
      let detailRight = max(0, width - 8)
      let detailWidth = min(detailRight, ceil(detailField.intrinsicContentSize.width) + 4)
      nameWidth = max(0, detailRight - detailWidth - 8)
      nameField.frame.size.width = nameWidth
      detailField.frame = NSRect(
        x: max(0, detailRight - detailWidth), y: 0, width: detailWidth, height: 16)
    }
  }
}
