// bluey-overlay-macos / cue-overlay-macos
//
// Native macOS overlay process. Speaks NDJSON IPC over a local Unix socket
// when BLUEY_OVERLAY_SOCKET is set, with stdin/stdout retained for test stubs
// and manual protocol checks. Provides:
//
//   - A compact centered pill (collapsed default state), draggable, click to
//     disappear into the full feed.
//   - A full feed/composer panel (expanded state) that renders the daemon's
//     CueCards, accepts Ask / Attach / Instructions / Recap input, and shows
//     streaming response chunks via UpdateCard.
//   - A boot card on startup driven by the daemon's Boot command.
//   - All windows are excluded from screen capture (capture_excluded=true)
//     and float above all spaces.
//
// Protocol is the canonical one defined in crates/cue-core/src/overlay.rs
// (OverlayCommand) and crates/cue-core/src/overlay_ipc.rs (OverlayEvent
// envelope wrapping the inner command). Every emitted line carries the
// per-session token from BLUEY_OVERLAY_SESSION_TOKEN so the daemon's
// production validator (validate_and_decode_overlay_line in app.rs) accepts
// our events.

import AppKit
import Darwin
import Foundation

// MARK: - Visual system

private enum BlueyTheme {
    static let cyan = NSColor(red: 0.35, green: 0.82, blue: 1.0, alpha: 1.0)
    static let cyanSoft = NSColor(red: 0.35, green: 0.82, blue: 1.0, alpha: 0.14)
    static let panel = NSColor(red: 0.025, green: 0.029, blue: 0.036, alpha: 0.95)
    static let panelDeep = NSColor(red: 0.015, green: 0.018, blue: 0.024, alpha: 0.97)
    static let surface = NSColor(red: 0.055, green: 0.065, blue: 0.080, alpha: 0.94)
    static let surfaceRaised = NSColor(red: 0.075, green: 0.090, blue: 0.110, alpha: 0.95)
    static let text = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
    static let textDim = NSColor(red: 0.58, green: 0.66, blue: 0.72, alpha: 1.0)
    static let hairline = NSColor.white.withAlphaComponent(0.08)
    static let green = NSColor(red: 0.42, green: 1.0, blue: 0.52, alpha: 1.0)
    static let warning = NSColor(red: 1.0, green: 0.72, blue: 0.28, alpha: 1.0)

    static func accent(for kind: String) -> NSColor {
        switch kind {
        case "answer": return cyan
        case "question": return NSColor(red: 0.58, green: 0.70, blue: 1.0, alpha: 1.0)
        case "transcript": return NSColor(red: 0.48, green: 1.0, blue: 0.72, alpha: 1.0)
        case "context": return NSColor(red: 0.78, green: 0.66, blue: 1.0, alpha: 1.0)
        case "action_item": return green
        case "decision": return NSColor(red: 0.72, green: 0.86, blue: 1.0, alpha: 1.0)
        case "warning": return warning
        default: return cyan
        }
    }
}

private enum ExpandedPanelMetrics {
    static let maxCompactWidth: CGFloat = 820
    static let minCompactWidth: CGFloat = 680
    static let maxCanvasWidth: CGFloat = 960
    static let height: CGFloat = 520
    static let minHeight: CGFloat = 440
    static let screenInset: CGFloat = 12

    static func fittingWidth(for screen: NSRect, preferred: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingHeight(for screen: NSRect, preferred: CGFloat = height) -> CGFloat {
        let available = max(minHeight, screen.height - screenInset * 2)
        return min(preferred, available)
    }

    static func fittingMinimumWidth(for screen: NSRect, targetWidth: CGFloat) -> CGFloat {
        let available = max(360, screen.width - screenInset * 2)
        return min(minCompactWidth, targetWidth, available)
    }

    static func fitExpandedFrameToVisibleScreen(_ frame: NSRect, visibleFrame: NSRect) -> NSRect {
        var fitted = frame
        let availableWidth = max(360, visibleFrame.width - screenInset * 2)
        let availableHeight = max(minHeight, visibleFrame.height - screenInset * 2)
        fitted.size.width = min(max(360, fitted.size.width), availableWidth)
        fitted.size.height = min(max(minHeight, fitted.size.height), availableHeight)
        fitted.origin.x = min(
            max(visibleFrame.minX + screenInset, fitted.origin.x),
            visibleFrame.maxX - fitted.size.width - screenInset)
        fitted.origin.y = min(
            max(visibleFrame.minY + screenInset, fitted.origin.y),
            visibleFrame.maxY - fitted.size.height - screenInset)
        return fitted
    }
}

private enum PillMetrics {
    static let size = NSSize(width: 128, height: 38)

    static func centeredFrame(in visibleFrame: NSRect) -> NSRect {
        NSRect(
            x: visibleFrame.midX - size.width / 2,
            y: visibleFrame.midY - size.height / 2,
            width: size.width,
            height: size.height)
    }
}

private func symbolImage(_ name: String) -> NSImage? {
    guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil) else {
        return nil
    }
    return image.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)) ?? image
}

private final class ComposerTextView: NSTextView {
    var placeholder = "Ask anything..." {
        didSet { needsDisplay = true }
    }
    var onSubmit: (() -> Void)?
    var onMeasuredHeight: ((CGFloat) -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override init(frame frameRect: NSRect, textContainer container: NSTextContainer?) {
        super.init(frame: frameRect, textContainer: container)
        drawsBackground = false
        isRichText = false
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        isContinuousSpellCheckingEnabled = false
        textColor = BlueyTheme.text
        insertionPointColor = BlueyTheme.cyan
        font = NSFont.systemFont(ofSize: 14.5, weight: .medium)
        textContainerInset = NSSize(width: 2, height: 7)
        textContainer?.lineFragmentPadding = 0
        textContainer?.widthTracksTextView = true
        textContainer?.heightTracksTextView = false
        minSize = NSSize(width: 0, height: 0)
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        isHorizontallyResizable = false
        isVerticallyResizable = true
        autoresizingMask = [.width]
    }

    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font ?? NSFont.systemFont(ofSize: 14.5, weight: .medium),
            .foregroundColor: BlueyTheme.textDim.withAlphaComponent(0.78),
        ]
        let rect = NSRect(x: 0, y: textContainerInset.height + 1, width: bounds.width, height: 22)
        placeholder.draw(in: rect, withAttributes: attributes)
    }

    override func didChangeText() {
        super.didChangeText()
        needsDisplay = true
        notifyMeasuredHeight()
    }

    override func layout() {
        super.layout()
        notifyMeasuredHeight()
    }

    override func keyDown(with event: NSEvent) {
        let chars = event.charactersIgnoringModifiers ?? ""
        let isReturn = event.keyCode == 36 || event.keyCode == 76 || chars == "\r" || chars == "\n"
        if isReturn {
            if event.modifierFlags.contains(.shift) {
                insertNewline(nil)
            } else {
                onSubmit?()
            }
            return
        }
        super.keyDown(with: event)
    }

    // Clicking the text view must make it first responder so keystrokes land.
    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        super.mouseDown(with: event)
    }

    func clearText() {
        string = ""
        selectedRange = NSRange(location: 0, length: 0)
        needsDisplay = true
        notifyMeasuredHeight()
    }

    private func notifyMeasuredHeight() {
        guard bounds.width > 24, let container = textContainer, let manager = layoutManager else {
            onMeasuredHeight?(44)
            return
        }
        container.containerSize = NSSize(width: max(80, bounds.width), height: CGFloat.greatestFiniteMagnitude)
        manager.ensureLayout(for: container)
        let used = manager.usedRect(for: container)
        let measured = ceil(used.height + textContainerInset.height * 2 + 6)
        onMeasuredHeight?(measured)
    }
}

/// The rounded background behind the composer. Clicking anywhere on it (not just
/// the text glyphs) focuses the composer, so the whole bar reads as one input.
private final class ComposerSurfaceView: NSView {
    weak var composer: ComposerTextView?

    override var acceptsFirstResponder: Bool { true }

    override func mouseDown(with event: NSEvent) {
        if let composer {
            window?.makeFirstResponder(composer)
        }
        super.mouseDown(with: event)
    }
}

// MARK: - Protocol

private struct CueCard: Decodable {
    let id: String
    let kind: String
    let title: String
    let body: String
    let createdAt: String?
    let source: String?
    let costLabel: String?
    let artifact: OverlayArtifact?

    enum CodingKeys: String, CodingKey {
        case id, kind, title, body
        case createdAt = "created_at"
        case source
        case costLabel = "cost_label"
        case artifact
    }
}

private struct OverlayArtifact: Decodable {
    let artifactType: String
    let title: String
    let body: String
    let confidence: Double?

    enum CodingKeys: String, CodingKey {
        case artifactType = "artifact_type"
        case title, body, confidence
    }
}

private struct OverlayContextItem {
    let id: String
    let title: String
    let kind: String
    let path: String?
}

private struct OverlaySessionItem {
    let id: String
    let title: String
    let subtitle: String
    let isActive: Bool
}

// MARK: - Agent-bridge wire DTOs (Slice 5b)
//
// Shape-only mirrors of crates/cue-core/src/agent_ui.rs. They carry a
// connector's name / auth tier / readiness and a session id / title /
// timestamp — never an env value, token, or message body. Field names match
// the Rust serde `snake_case` form exactly so JSON decodes one-to-one.

private struct AgentSummary {
    let kind: String
    let displayName: String
    /// snake_case capability: drive / read_only / needs_trust / needs_reauth /
    /// cloud_blocked.
    let capability: String
    let connectorCount: Int
    let readyConnectorCount: Int
    let sessionCount: Int?
    let attached: Bool
}

private struct AgentSessionSummary {
    let id: String
    let title: String?
    let updatedAt: String
}

private struct AgentConnectorInfo {
    let name: String
    /// snake_case auth tier: env_auth / hosted_oauth / none.
    let authTier: String
    let ready: Bool
}

/// Capability presentation: the snake_case `capability` string mapped to a
/// drawer chip label, accent color, and a "dimmed / non-tappable" flag for
/// `cloud_blocked`. Colors come straight from BlueyTheme (PLAN §9 surface 1).
private struct AgentCapability {
    let label: String
    let color: NSColor
    let dimmed: Bool

    init(_ raw: String) {
        switch raw {
        case "drive":
            label = "live"
            color = BlueyTheme.green
            dimmed = false
        case "read_only":
            label = "history only"
            color = BlueyTheme.textDim
            dimmed = false
        case "needs_trust":
            label = "needs trust"
            color = BlueyTheme.warning
            dimmed = false
        case "needs_reauth":
            label = "re-auth"
            color = BlueyTheme.warning
            dimmed = false
        case "cloud_blocked":
            label = "unavailable"
            color = NSColor(red: 1.0, green: 0.44, blue: 0.40, alpha: 1.0)
            dimmed = true
        default:
            label = raw.replacingOccurrences(of: "_", with: " ")
            color = BlueyTheme.textDim
            dimmed = false
        }
    }
}

/// Compact uppercase label for an agent kind, used on the pill-adjacent
/// header badge and the agent-answer card role badge (CLAUDE / CURSOR / …).
private func agentShortLabel(_ kind: String) -> String {
    switch kind {
    case "claude_code": return "CLAUDE"
    case "cursor": return "CURSOR"
    case "codex": return "CODEX"
    case "gemini": return "GEMINI"
    case "windsurf": return "WINDSURF"
    case "aider": return "AIDER"
    default:
        // Strip a trailing "_code" / "_cli" and uppercase the first token.
        let base = kind
            .replacingOccurrences(of: "_code", with: "")
            .replacingOccurrences(of: "_cli", with: "")
        let head = base.split(separator: "_").first.map(String.init) ?? base
        return head.uppercased()
    }
}

/// Inbound commands from the daemon.
private enum OverlayCommand {
    case ping
    case show
    case hide
    case toggle
    case clear
    case boot(title: String, lines: [String])
    case setOpacity(Double)
    case setPosition(String)
    case setBalance(String)
    case setContextItems([OverlayContextItem])
    case setSessions([OverlaySessionItem])
    case pushCard(CueCard)
    case updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?)
    case setAgents([AgentSummary])
    case setAgentSessions(kind: String, sessions: [AgentSessionSummary])
    case setAgentConnectors(kind: String, connectors: [AgentConnectorInfo])
    case pushFixProposal(FixProposal)
    case shutdown
    case unknown(String)
}

/// A review-gated Fix proposal pushed by the daemon (Fix-button slice F4).
/// Mirrors `OverlayCommand::PushFixProposal` in crates/cue-core/src/overlay.rs.
/// `diff` is absent when the agent only proposed commands (no unified diff);
/// `applySupported` is false for agents that cannot be driven to apply.
private struct FixProposal {
    let proposalId: String
    let diagnosis: String
    let reasoning: String
    let fix: String
    let diff: String?
    let applySupported: Bool
}

private func parseCommand(_ line: String) -> OverlayCommand {
    guard let data = line.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let type = obj["type"] as? String
    else {
        return .unknown(line)
    }
    switch type {
    case "ping":     return .ping
    case "show":     return .show
    case "hide":     return .hide
    case "toggle":   return .toggle
    case "clear":    return .clear
    case "shutdown": return .shutdown
    case "boot":
        let title = obj["title"] as? String ?? ""
        let lines = obj["lines"] as? [String] ?? []
        return .boot(title: title, lines: lines)
    case "set_opacity":
        let opacity = (obj["opacity"] as? Double) ?? 1.0
        return .setOpacity(opacity)
    case "set_position":
        let position = obj["position"] as? String ?? "top_right"
        return .setPosition(position)
    case "set_balance":
        let label = obj["label"] as? String ?? "Balance --"
        return .setBalance(label)
    case "set_context_items":
        let rawItems = obj["items"] as? [[String: Any]] ?? []
        let items = rawItems.map { item in
            OverlayContextItem(
                id: item["id"] as? String ?? UUID().uuidString,
                title: item["title"] as? String ?? "Attached file",
                kind: item["kind"] as? String ?? "document",
                path: item["path"] as? String
            )
        }
        return .setContextItems(items)
    case "set_sessions":
        let rawSessions = obj["sessions"] as? [[String: Any]] ?? []
        let sessions = rawSessions.map { item in
            OverlaySessionItem(
                id: item["id"] as? String ?? "",
                title: item["title"] as? String ?? "Bluey session",
                subtitle: item["subtitle"] as? String ?? "",
                isActive: item["is_active"] as? Bool ?? false
            )
        }.filter { !$0.id.isEmpty }
        return .setSessions(sessions)
    case "push_card":
        guard let cardObj = obj["card"] as? [String: Any],
              let cardData = try? JSONSerialization.data(withJSONObject: cardObj),
              let card = try? JSONDecoder().decode(CueCard.self, from: cardData)
        else { return .unknown(line) }
        return .pushCard(card)
    case "update_card":
        let id = obj["id"] as? String ?? ""
        let body = obj["body"] as? String ?? ""
        let done = obj["done"] as? Bool ?? false
        let costLabel = obj["cost_label"] as? String
        var artifact: OverlayArtifact?
        if let artifactObj = obj["artifact"] as? [String: Any],
           let artifactData = try? JSONSerialization.data(withJSONObject: artifactObj) {
            artifact = try? JSONDecoder().decode(OverlayArtifact.self, from: artifactData)
        }
        return .updateCard(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact)
    case "set_agents":
        let rawAgents = obj["agents"] as? [[String: Any]] ?? []
        let agents = rawAgents.map { item in
            AgentSummary(
                kind: item["kind"] as? String ?? "",
                displayName: item["display_name"] as? String ?? "Coding agent",
                capability: item["capability"] as? String ?? "read_only",
                connectorCount: item["connector_count"] as? Int ?? 0,
                readyConnectorCount: item["ready_connector_count"] as? Int ?? 0,
                sessionCount: item["session_count"] as? Int,
                attached: item["attached"] as? Bool ?? false
            )
        }.filter { !$0.kind.isEmpty }
        return .setAgents(agents)
    case "set_agent_sessions":
        let kind = obj["kind"] as? String ?? ""
        let rawSessions = obj["sessions"] as? [[String: Any]] ?? []
        let sessions = rawSessions.map { item in
            AgentSessionSummary(
                id: item["id"] as? String ?? "",
                title: item["title"] as? String,
                updatedAt: item["updated_at"] as? String ?? ""
            )
        }.filter { !$0.id.isEmpty }
        return .setAgentSessions(kind: kind, sessions: sessions)
    case "set_agent_connectors":
        let kind = obj["kind"] as? String ?? ""
        let rawConnectors = obj["connectors"] as? [[String: Any]] ?? []
        let connectors = rawConnectors.map { item in
            AgentConnectorInfo(
                name: item["name"] as? String ?? "connector",
                authTier: item["auth_tier"] as? String ?? "none",
                ready: item["ready"] as? Bool ?? false
            )
        }
        return .setAgentConnectors(kind: kind, connectors: connectors)
    case "push_fix_proposal":
        // Defensive decode: an unknown/missing proposal_id is unusable (the
        // approve/reject echo is id-matched upstream), so drop the command
        // rather than render an un-actionable card. The diff is optional and
        // is omitted from the wire form when absent -> nil.
        guard let proposalId = obj["proposal_id"] as? String, !proposalId.isEmpty else {
            return .unknown(line)
        }
        let proposal = FixProposal(
            proposalId: proposalId,
            diagnosis: obj["diagnosis"] as? String ?? "",
            reasoning: obj["reasoning"] as? String ?? "",
            fix: obj["fix"] as? String ?? "",
            diff: obj["diff"] as? String,
            applySupported: obj["apply_supported"] as? Bool ?? false
        )
        return .pushFixProposal(proposal)
    default:
        return .unknown(line)
    }
}

/// Outbound events to the daemon. Every event carries the session token
/// embedded as the top-level "token" field; the daemon's
/// validate_and_decode_overlay_line function rejects events without it.
private func argumentValue(_ name: String) -> String? {
    let args = CommandLine.arguments
    guard let idx = args.firstIndex(of: name),
          args.indices.contains(idx + 1)
    else {
        return nil
    }
    return args[idx + 1]
}

private func argumentFlag(_ name: String) -> Bool {
    CommandLine.arguments.contains(name)
}

private func envFlag(_ name: String) -> Bool {
    let raw = ProcessInfo.processInfo.environment[name]?
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
    return raw == "1" || raw == "true" || raw == "yes" || raw == "on"
}

private let sessionToken: String = ProcessInfo.processInfo
    .environment["BLUEY_OVERLAY_SESSION_TOKEN"]
    ?? argumentValue("--bluey-overlay-session-token")
    ?? ""
private let overlaySocketPath: String? = argumentValue("--bluey-overlay-socket")
    ?? ProcessInfo.processInfo
        .environment["BLUEY_OVERLAY_SOCKET"]

private let ipcLock = NSLock()
private var ipcInputHandle: FileHandle?
private var ipcOutputHandle: FileHandle = FileHandle.standardOutput

private let captureVisibleForDebug: Bool = {
    let devEnabled = argumentFlag("--bluey-dev-overlay") || envFlag("BLUEY_DEV_OVERLAY")
    guard devEnabled else { return false }
    return argumentFlag("--bluey-overlay-capture-visible")
        || envFlag("BLUEY_OVERLAY_CAPTURE_VISIBLE")
        || envFlag("BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE")
}()

private func connectUnixSocket(path: String) -> Int32? {
    let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { return nil }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8CString)
    let maxPathBytes = MemoryLayout.size(ofValue: addr.sun_path)
    guard bytes.count <= maxPathBytes else {
        Darwin.close(fd)
        return nil
    }

    withUnsafeMutableBytes(of: &addr.sun_path) { rawBuffer in
        let dest = rawBuffer.bindMemory(to: CChar.self)
        for idx in bytes.indices {
            dest[idx] = bytes[idx]
        }
    }

    let result = withUnsafePointer(to: &addr) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
            Darwin.connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard result == 0 else {
        Darwin.close(fd)
        return nil
    }

    return fd
}

private func connectIpcIfNeeded() {
    guard let path = overlaySocketPath,
          !path.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
        return
    }

    guard let inputFd = connectUnixSocket(path: path) else {
        fputs("bluey-overlay: failed to connect IPC socket \(path)\n", stderr)
        return
    }
    let outputFd = Darwin.dup(inputFd)
    guard outputFd >= 0 else {
        Darwin.close(inputFd)
        fputs("bluey-overlay: failed to duplicate IPC socket fd\n", stderr)
        return
    }

    ipcInputHandle = FileHandle(fileDescriptor: inputFd, closeOnDealloc: true)
    ipcOutputHandle = FileHandle(fileDescriptor: outputFd, closeOnDealloc: true)
}

private func emitEvent(_ payload: [String: Any]) {
    var withToken = payload
    if !sessionToken.isEmpty {
        withToken["token"] = sessionToken
    }
    guard let data = try? JSONSerialization.data(withJSONObject: withToken),
          let json = String(data: data, encoding: .utf8)
    else { return }
    guard let line = (json + "\n").data(using: .utf8) else { return }
    ipcLock.lock()
    ipcOutputHandle.write(line)
    ipcLock.unlock()
}

private func emitReady() {
    emitEvent([
        "type": "ready",
        "platform": "macos",
        "capture_excluded": !captureVisibleForDebug,
    ])
}

private func emitSimple(_ type: String) {
    emitEvent(["type": type])
}

private func emitLifecycle(_ stage: String, status: String = "ok", detail: String? = nil) {
    var payload: [String: Any] = [
        "type": "lifecycle",
        "stage": stage,
        "status": status,
    ]
    if let detail, !detail.isEmpty {
        payload["detail"] = detail
    }
    emitEvent(payload)
}

private func emitAsk(question: String, provider: String?, model: String?, mode: String?) {
    var p: [String: Any] = ["type": "ask_requested", "question": question]
    if let provider = provider { p["provider"] = provider }
    if let model = model       { p["model"]    = model }
    if let mode = mode         { p["mode"]     = mode }
    emitEvent(p)
}

private func emitAttachFiles(paths: [String]) {
    emitEvent(["type": "attach_files_requested", "paths": paths])
}

private func emitInstructions(text: String) {
    emitEvent(["type": "instructions_updated", "text": text])
}

private func emitSessionOpen(id: String) {
    emitEvent(["type": "session_open_requested", "id": id])
}

private func emitSessionRename(id: String, title: String) {
    emitEvent(["type": "session_rename_requested", "id": id, "title": title])
}

// MARK: - Agent-bridge emit helpers (Slice 5b)
//
// Each event is a dict tagged with "type", matching OverlayEvent's serde
// `snake_case` form in crates/cue-core/src/overlay.rs. Optional fields are
// omitted (not sent as null) so the daemon's `#[serde(default)]` applies.

private func emitAgentListRequested() {
    emitEvent(["type": "agent_list_requested"])
}

private func emitAgentAttachRequested(kind: String, sessionId: String?) {
    var p: [String: Any] = ["type": "agent_attach_requested", "kind": kind]
    if let sessionId, !sessionId.isEmpty { p["session_id"] = sessionId }
    emitEvent(p)
}

private func emitAgentDetachRequested() {
    emitEvent(["type": "agent_detach_requested"])
}

private func emitAgentSessionsRequested(kind: String) {
    emitEvent(["type": "agent_sessions_requested", "kind": kind])
}

private func emitAgentConnectorsRequested(kind: String) {
    emitEvent(["type": "agent_connectors_requested", "kind": kind])
}

private func emitConnectorReauthRequested(kind: String, name: String) {
    emitEvent(["type": "connector_reauth_requested", "kind": kind, "name": name])
}

// MARK: - Fix-button emit helpers (Slice F4)
//
// Mirror OverlayEvent::FixRequested / FixApprovalResponded. `card_id` is
// omitted (not null) when nil so the daemon's `#[serde(default)]` applies.

private func emitFixRequested(cardId: String?, question: String) {
    var p: [String: Any] = ["type": "fix_requested", "question": question]
    if let cardId, !cardId.isEmpty { p["card_id"] = cardId }
    emitEvent(p)
}

private func emitFixApprovalResponded(proposalId: String, approved: Bool) {
    emitEvent([
        "type": "fix_approval_responded",
        "proposal_id": proposalId,
        "approved": approved,
    ])
}

private func emitCardRendered(id: String) {
    emitEvent(["type": "card_rendered", "id": id])
}

// MARK: - Overlay NSWindow

/// Borderless, transparent, always-on-top overlay window.
/// Configured for either the small pill or the expanded feed depending on
/// the size passed at construction time.
private final class OverlayWindow: NSWindow {
    var lockedFrameHeight: CGFloat?
    var minimumFrameWidth: CGFloat?
    var maximumFrameWidth: CGFloat?
    var minimumFrameHeight: CGFloat?
    var maximumFrameHeight: CGFloat?

    init(contentRect: NSRect, draggable: Bool, resizable: Bool = false) {
        var style: NSWindow.StyleMask = [.borderless]
        if resizable {
            style.insert(.resizable)
        }
        super.init(
            contentRect: contentRect,
            styleMask: style,
            backing: .buffered,
            defer: false
        )
        self.isOpaque = false
        self.backgroundColor = .clear
        self.hasShadow = true
        self.isReleasedWhenClosed = false
        self.level = .floating
        self.collectionBehavior = [
            .canJoinAllSpaces,
            .stationary,
            .ignoresCycle,
            .fullScreenAuxiliary,
        ]
        self.isMovableByWindowBackground = draggable
        self.hidesOnDeactivate = false
        // Production keeps the overlay out of screen capture. Capture-visible
        // QA is dev-gated and must never be enabled in customer launch paths.
        self.sharingType = captureVisibleForDebug ? .readOnly : .none
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag)
    }

    override func setFrame(_ frameRect: NSRect, display displayFlag: Bool, animate animateFlag: Bool) {
        super.setFrame(clampedFrame(frameRect), display: displayFlag, animate: animateFlag)
    }

    override func setContentSize(_ size: NSSize) {
        if clampingEnabled {
            var requestedSize = size
            if lockedFrameHeight == nil {
                // AppKit may try to satisfy dense feed/composer constraints by
                // growing the borderless window. Preserve the current height
                // for content-size fitting while still allowing real user
                // frame resizing through setFrame(_:display:).
                requestedSize.height = frame.height
            }
            super.setFrame(
                clampedFrame(NSRect(origin: frame.origin, size: requestedSize)),
                display: true)
        } else {
            super.setContentSize(size)
        }
    }

    private func clampedFrame(_ frame: NSRect) -> NSRect {
        var clamped = frame
        if let minimumFrameWidth {
            clamped.size.width = max(minimumFrameWidth, clamped.size.width)
        }
        if let maximumFrameWidth {
            clamped.size.width = min(maximumFrameWidth, clamped.size.width)
        }
        if let lockedFrameHeight {
            clamped.size.height = lockedFrameHeight
        } else {
            if let minimumFrameHeight {
                clamped.size.height = max(minimumFrameHeight, clamped.size.height)
            }
            if let maximumFrameHeight {
                clamped.size.height = min(maximumFrameHeight, clamped.size.height)
            }
        }

        guard clampingEnabled else {
            return clamped
        }

        let inset: CGFloat = 12
        let visibleFrame = screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let screenMaxWidth = max(360, visibleFrame.width - inset * 2)
        clamped.size.width = min(clamped.size.width, screenMaxWidth)
        if let minimumFrameWidth, minimumFrameWidth <= screenMaxWidth {
            clamped.size.width = max(minimumFrameWidth, clamped.size.width)
        }
        return ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(clamped, visibleFrame: visibleFrame)
    }

    private var clampingEnabled: Bool {
        lockedFrameHeight != nil
            || minimumFrameWidth != nil
            || maximumFrameWidth != nil
            || minimumFrameHeight != nil
            || maximumFrameHeight != nil
    }
}

// MARK: - Pill view

private final class PillView: NSView {
    var statusText: String = "Bluey" {
        didSet {
            updateTitleDisplay()
            needsDisplay = true
        }
    }
    /// Codex Stage 19 commit 1 follow-up: live balance text rendered
    /// next to status. Daemon sends "$4.98" (or "$4.98 low" when below
    /// auto-topup threshold). Empty string clears.
    var balanceText: String = "" {
        didSet {
            updateTitleDisplay()
            needsDisplay = true
        }
    }
    private func updateTitleDisplay() {
        // The collapsed pill is intentionally identity-only. Balance lives in
        // the expanded header so the launcher stays compact and scannable.
        titleField.stringValue = statusText
        titleField.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
    }
    func setBalanceLabel(_ label: String) {
        balanceText = label
    }
    var dotColor: NSColor = NSColor.systemGreen {
        didSet {
            dotView.layer?.backgroundColor = dotColor.cgColor
            needsDisplay = true
        }
    }
    var onClick: (() -> Void)?

    /// Small cyan agent glyph shown at the pill's trailing edge while a coding
    /// agent is attached. Glyph-only — no text, so the pill never truncates
    /// (agent-bridge Slice 5b, PLAN §9 surface 4).
    var agentAttached: Bool = false {
        didSet {
            guard agentAttached != oldValue else { return }
            agentGlyph.isHidden = !agentAttached
            needsLayout = true
        }
    }

    private let logoTile = NSView()
    private let logoGlyph = NSTextField(labelWithString: ">_")
    private let titleField = NSTextField(labelWithString: "Bluey")
    private let dotView = NSView()
    private let agentGlyph = NSImageView()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.cornerRadius = frameRect.height / 2
        layer?.borderWidth = 0
        layer?.shadowColor = NSColor(red: 0.10, green: 0.70, blue: 0.96, alpha: 1.0).cgColor
        layer?.shadowOpacity = 0.18
        layer?.shadowRadius = 8
        layer?.shadowOffset = .zero

        logoTile.wantsLayer = true
        logoTile.layer?.backgroundColor = NSColor(red: 0.014, green: 0.104, blue: 0.136, alpha: 1.0).cgColor
        logoTile.layer?.cornerRadius = 9
        logoTile.layer?.borderWidth = 1
        logoTile.layer?.borderColor = NSColor(red: 0.38, green: 0.88, blue: 1.0, alpha: 0.70).cgColor
        logoTile.layer?.shadowColor = NSColor(red: 0.15, green: 0.66, blue: 1.0, alpha: 1.0).cgColor
        logoTile.layer?.shadowOpacity = 0.20
        logoTile.layer?.shadowRadius = 8
        logoTile.layer?.shadowOffset = .zero
        addSubview(logoTile)

        logoGlyph.font = NSFont.monospacedSystemFont(ofSize: 12.5, weight: .bold)
        logoGlyph.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        logoGlyph.alignment = .center
        logoTile.addSubview(logoGlyph)

        titleField.font = NSFont.systemFont(ofSize: 15, weight: .bold)
        titleField.textColor = NSColor(red: 0.92, green: 0.98, blue: 1.0, alpha: 1.0)
        titleField.alignment = .left
        addSubview(titleField)

        dotView.wantsLayer = true
        dotView.layer?.backgroundColor = dotColor.cgColor
        dotView.layer?.cornerRadius = 4
        dotView.layer?.shadowColor = dotColor.cgColor
        dotView.layer?.shadowOpacity = 0.62
        dotView.layer?.shadowRadius = 6
        dotView.layer?.shadowOffset = .zero
        addSubview(dotView)

        agentGlyph.wantsLayer = true
        agentGlyph.isHidden = true
        agentGlyph.imageScaling = .scaleProportionallyDown
        agentGlyph.contentTintColor = BlueyTheme.cyan
        agentGlyph.layer?.backgroundColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
        agentGlyph.layer?.cornerRadius = 9
        agentGlyph.layer?.borderWidth = 1
        agentGlyph.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.55).cgColor
        agentGlyph.toolTip = "A coding agent is attached"
        if let image = symbolImage("cpu") {
            image.isTemplate = true
            agentGlyph.image = image
        }
        addSubview(agentGlyph)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        layer?.cornerRadius = bounds.height / 2

        let logoSide: CGFloat = 29
        logoTile.frame = NSRect(x: 6, y: (bounds.height - logoSide) / 2, width: logoSide, height: logoSide)
        logoTile.layer?.cornerRadius = 9
        logoGlyph.frame = logoTile.bounds.insetBy(dx: 4, dy: 6)

        titleField.frame = NSRect(x: 47, y: (bounds.height - 21) / 2 + 1, width: bounds.width - 66, height: 21)

        // Glyph-only agent badge pinned to the trailing edge when attached.
        let glyphSide: CGFloat = 18
        var trailingLimit = bounds.width - 8
        if agentAttached {
            let glyphX = bounds.width - glyphSide - 8
            agentGlyph.frame = NSRect(
                x: glyphX, y: (bounds.height - glyphSide) / 2,
                width: glyphSide, height: glyphSide)
            agentGlyph.layer?.cornerRadius = glyphSide / 2
            trailingLimit = glyphX - 6
        }

        let labelWidth = ceil((titleField.stringValue as NSString).size(withAttributes: [
            .font: titleField.font ?? NSFont.systemFont(ofSize: 15, weight: .bold),
        ]).width)
        let dotSize: CGFloat = 8
        let dotX = min(titleField.frame.minX + labelWidth + 4, trailingLimit - dotSize)
        dotView.frame = NSRect(x: dotX, y: bounds.midY + 4, width: dotSize, height: dotSize)
        dotView.layer?.cornerRadius = dotSize / 2
    }

    override func draw(_ dirtyRect: NSRect) {
        NSGraphicsContext.saveGraphicsState()

        let outer = bounds.insetBy(dx: 0.85, dy: 0.85)
        let radius = outer.height / 2
        let path = NSBezierPath(roundedRect: outer, xRadius: radius, yRadius: radius)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.34)
        shadow.shadowBlurRadius = 9
        shadow.shadowOffset = .zero
        shadow.set()

        let bg = NSGradient(colors: [
            NSColor(red: 0.007, green: 0.011, blue: 0.017, alpha: 0.98),
            NSColor(red: 0.014, green: 0.026, blue: 0.032, alpha: 0.95),
            NSColor(red: 0.007, green: 0.010, blue: 0.015, alpha: 0.99),
        ])
        bg?.draw(in: path, angle: -12)

        NSGraphicsContext.restoreGraphicsState()

        NSColor(red: 0.26, green: 0.74, blue: 0.96, alpha: 0.38).setStroke()
        path.lineWidth = 1.0
        path.stroke()

        let inner = outer.insetBy(dx: 1.5, dy: 1.5)
        let innerPath = NSBezierPath(roundedRect: inner, xRadius: inner.height / 2, yRadius: inner.height / 2)
        NSColor.white.withAlphaComponent(0.050).setStroke()
        innerPath.lineWidth = 0.7
        innerPath.stroke()

        let gloss = NSBezierPath(roundedRect: outer.insetBy(dx: 2, dy: 2), xRadius: radius - 2, yRadius: radius - 2)
        NSGradient(colors: [
            NSColor.white.withAlphaComponent(0.08),
            NSColor.white.withAlphaComponent(0.00),
        ])?.draw(in: gloss, angle: 90)
    }

    override func mouseDown(with event: NSEvent) {
        let startLocation = event.locationInWindow
        var didDrag = false
        var keepGoing = true
        while keepGoing {
            guard let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp])
            else { break }
            switch next.type {
            case .leftMouseDragged:
                let dx = next.locationInWindow.x - startLocation.x
                let dy = next.locationInWindow.y - startLocation.y
                if abs(dx) > 4 || abs(dy) > 4 {
                    didDrag = true
                    window?.performDrag(with: event)
                    keepGoing = false
                }
            case .leftMouseUp:
                if !didDrag { onClick?() }
                keepGoing = false
            default:
                keepGoing = false
            }
        }
    }
}

// MARK: - Card feed view

/// The user's one-shot decision on a Fix proposal card (Slice F4). `pending`
/// shows Approve/Reject; the terminal states disable both buttons so a proposal
/// can never be double-submitted.
private enum FixProposalState: Equatable {
    case pending
    case applying
    case discarded
}

private struct RenderedCard {
    let id: String
    let kind: String
    let title: String
    var body: String
    var done: Bool
    var costLabel: String?
    var artifact: OverlayArtifact?
    /// Free-form provenance from CueCard.source (e.g. "claude_code agent").
    /// When an answer card's source names a coding agent, the role badge and
    /// status reflect that agent instead of BLUEY (agent-bridge Slice 5b).
    var source: String?
    /// Set only for kind == "fix_proposal" cards (Slice F4): the proposal
    /// payload plus the user's pending/applying/discarded decision.
    var fixProposal: FixProposal? = nil
    var fixState: FixProposalState = .pending
}

private enum CanvasKind {
    case code
    case systemDesign
    case screen
    case document
    case structured

    static func fromArtifactType(_ value: String) -> CanvasKind {
        switch value {
        case "code": return .code
        case "system_design": return .systemDesign
        case "screen": return .screen
        case "document": return .document
        default: return .structured
        }
    }

    var title: String {
        switch self {
        case .code: return "Code canvas"
        case .systemDesign: return "System design canvas"
        case .screen: return "Screen analysis"
        case .document: return "Document notes"
        case .structured: return "Workspace"
        }
    }

    var subtitle: String {
        switch self {
        case .code: return "Code, tests, complexity"
        case .systemDesign: return "Architecture, tradeoffs, scale"
        case .screen: return "Screen context and answer"
        case .document: return "Attached context notes"
        case .structured: return "Structured workspace"
        }
    }

    var icon: String {
        switch self {
        case .code: return "curlybraces"
        case .systemDesign: return "square.stack.3d.up"
        case .screen: return "rectangle.and.text.magnifyingglass"
        case .document: return "doc.text"
        case .structured: return "sidebar.right"
        }
    }
}

private struct CanvasArtifact {
    let kind: CanvasKind
    let title: String
    let subtitle: String
    let content: String
    let sourceCardId: String
}

private final class FeedView: NSView {
    private var cards: [RenderedCard] = []
    private let stack = NSStackView()
    private let scroll = NSScrollView()
    private let emptyState = NSView()
    var onTranscript: ((RenderedCard) -> Void)?
    /// Fired when the user taps **Fix** on an agent answer card (Slice F4).
    /// Carries the source card id + its body text (the problem to fix).
    var onFixRequested: ((_ cardId: String, _ question: String) -> Void)?
    /// Interactive controls inside cards (Fix / Approve / Reject buttons). The
    /// feed/workspace region is normally click-through; the panel consults
    /// `hasInteractiveControl(at:)` so only these button frames capture the
    /// mouse, leaving the rest of the feed transparent to the app underneath.
    private let interactiveControls = NSHashTable<NSView>.weakObjects()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = BlueyTheme.panel.cgColor
        layer?.cornerRadius = 16
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.hairline.cgColor

        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 0, bottom: 16, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.documentView = stack
        scroll.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scroll)
        configureEmptyState()
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.widthAnchor.constraint(equalTo: scroll.widthAnchor),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }

    /// Register a button so the panel's pass-through tracking treats its frame
    /// as interactive. Enabled state is re-checked live at hit-test time, so a
    /// disabled (already-submitted) button stops capturing the mouse.
    private func registerInteractive(_ control: NSView) {
        interactiveControls.add(control)
    }

    /// True when `point` (in FeedView coordinates) lands on an enabled card
    /// button. Used by ExpandedPanelView.isInteractiveAtScreenPoint so card
    /// affordances are clickable without making the whole feed opaque to mouse
    /// events. Disabled buttons (terminal proposal states) return false.
    func hasInteractiveControl(at point: NSPoint) -> Bool {
        guard let hit = hitTest(point) else { return false }
        var node: NSView? = hit
        while let current = node {
            if interactiveControls.contains(current) {
                if let control = current as? NSControl { return control.isEnabled }
                return true
            }
            node = current.superview
        }
        return false
    }

    func push(_ card: RenderedCard) {
        if card.kind == "transcript" {
            onTranscript?(card)
            emitCardRendered(id: card.id)
            return
        }
        cards.append(card)
        emptyState.isHidden = true
        let view = makeCardView(card)
        stack.addArrangedSubview(view)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        emitCardRendered(id: card.id)
    }

    @discardableResult
    func update(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) -> RenderedCard? {
        guard let idx = cards.firstIndex(where: { $0.id == id }) else { return nil }
        cards[idx].body = body
        cards[idx].done = done
        if let costLabel {
            cards[idx].costLabel = costLabel
        }
        if let artifact {
            cards[idx].artifact = artifact
        }
        // Replace the corresponding subview.
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        scrollToBottom()
        return cards[idx]
    }

    func clear() {
        cards.removeAll()
        interactiveControls.removeAllObjects()
        for v in stack.arrangedSubviews { v.removeFromSuperview() }
        emptyState.isHidden = false
    }

    /// Transition a Fix proposal card to a terminal/transient state (Slice F4)
    /// and rebuild just its subview so the buttons reflect the new state. The
    /// proposal id doubles as the card id, so we match on it directly.
    private func setFixState(proposalId: String, to state: FixProposalState) {
        guard let idx = cards.firstIndex(where: {
            $0.kind == "fix_proposal" && $0.id == proposalId
        }) else { return }
        guard case .pending = cards[idx].fixState else { return } // one-shot
        cards[idx].fixState = state
        let existing = stack.arrangedSubviews[idx]
        stack.removeArrangedSubview(existing)
        existing.removeFromSuperview()
        let view = makeCardView(cards[idx])
        stack.insertArrangedSubview(view, at: idx)
        view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    private func configureEmptyState() {
        emptyState.translatesAutoresizingMaskIntoConstraints = false
        emptyState.wantsLayer = true
        emptyState.layer?.backgroundColor = NSColor.clear.cgColor
        addSubview(emptyState)

        let badge = NSTextField(labelWithString: "READY")
        badge.translatesAutoresizingMaskIntoConstraints = false
        badge.font = NSFont.monospacedSystemFont(ofSize: 10, weight: .bold)
        badge.textColor = BlueyTheme.cyan

        let title = NSTextField(labelWithString: "New recording")
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 22, weight: .bold)
        title.textColor = BlueyTheme.text
        title.alignment = .center

        let subtitle = NSTextField(labelWithString: "Audio, files, screen context, and answers stay in this session.")
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.alignment = .center
        subtitle.maximumNumberOfLines = 2
        subtitle.lineBreakMode = .byWordWrapping

        let chips = NSStackView()
        chips.translatesAutoresizingMaskIntoConstraints = false
        chips.orientation = .horizontal
        chips.alignment = .centerY
        chips.spacing = 8
        for label in ["Audio", "Files", "Screen", "Canvas"] {
            chips.addArrangedSubview(emptyChip(label))
        }

        emptyState.addSubview(badge)
        emptyState.addSubview(title)
        emptyState.addSubview(subtitle)
        emptyState.addSubview(chips)

        NSLayoutConstraint.activate([
            emptyState.centerXAnchor.constraint(equalTo: centerXAnchor),
            emptyState.centerYAnchor.constraint(equalTo: centerYAnchor, constant: -12),
            emptyState.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, multiplier: 0.76),

            badge.topAnchor.constraint(equalTo: emptyState.topAnchor),
            badge.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),

            title.topAnchor.constraint(equalTo: badge.bottomAnchor, constant: 8),
            title.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            title.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 8),
            subtitle.leadingAnchor.constraint(equalTo: emptyState.leadingAnchor),
            subtitle.trailingAnchor.constraint(equalTo: emptyState.trailingAnchor),

            chips.topAnchor.constraint(equalTo: subtitle.bottomAnchor, constant: 16),
            chips.centerXAnchor.constraint(equalTo: emptyState.centerXAnchor),
            chips.bottomAnchor.constraint(equalTo: emptyState.bottomAnchor),
        ])
    }

    private func emptyChip(_ text: String) -> NSView {
        let chip = NSTextField(labelWithString: text)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        chip.textColor = BlueyTheme.textDim
        chip.alignment = .center
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        chip.layer?.cornerRadius = 10
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.hairline.cgColor
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 24),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 58),
        ])
        return chip
    }

    private func makeCardView(_ card: RenderedCard) -> NSView {
        if card.kind == "fix_proposal", let proposal = card.fixProposal {
            return makeFixProposalView(proposal, state: card.fixState)
        }
        let accent = BlueyTheme.accent(for: card.kind)
        let rightAligned = isUserSide(card)
        let answerLike = card.kind == "answer"
        // Agent answers (final, not streaming) get a compact Fix affordance.
        let showFix = answerLike && card.done && agentLabel(from: card.source) != nil
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false

        let bubble = NSView()
        bubble.wantsLayer = true
        bubble.layer?.backgroundColor = rightAligned
            ? NSColor(red: 0.90, green: 0.93, blue: 0.95, alpha: 0.96).cgColor
            : (answerLike ? NSColor.clear : BlueyTheme.surface).cgColor
        bubble.layer?.cornerRadius = rightAligned ? 16 : 12
        bubble.layer?.borderWidth = answerLike ? 0 : 1
        bubble.layer?.borderColor = rightAligned
            ? NSColor.white.withAlphaComponent(0.20).cgColor
            : accent.withAlphaComponent(card.kind == "answer" ? 0.24 : 0.14).cgColor
        bubble.layer?.shadowColor = NSColor.black.cgColor
        bubble.layer?.shadowOpacity = answerLike ? 0 : 0.14
        bubble.layer?.shadowRadius = 10
        bubble.layer?.shadowOffset = NSSize(width: 0, height: -4)
        bubble.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: kindLabel(card))
        metaLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        metaLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.58) : accent
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let titleText = displayTitle(for: card)
        let titleLabel = NSTextField(labelWithString: titleText)
        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .semibold)
        titleLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.74) : BlueyTheme.text
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.lineBreakMode = .byTruncatingTail

        let rawBody = card.body.isEmpty && !card.done ? "Thinking..." : card.body
        let bodyText = chatBody(for: card, rawBody: rawBody)
        let bodyLabel = NSTextField(wrappingLabelWithString: bodyText)
        bodyLabel.font = bodyFont(for: card)
        bodyLabel.textColor = rightAligned ? NSColor.black : BlueyTheme.text
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.preferredMaxLayoutWidth = rightAligned ? 360 : 480

        let statusLabel = NSTextField(labelWithString: statusText(for: card))
        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = rightAligned ? NSColor.black.withAlphaComponent(0.46) : BlueyTheme.textDim
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        row.addSubview(bubble)
        bubble.addSubview(metaLabel)
        bubble.addSubview(titleLabel)
        bubble.addSubview(bodyLabel)
        bubble.addSubview(statusLabel)

        let leading = bubble.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 8)
        let trailing = bubble.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8)
        if rightAligned {
            leading.priority = .defaultLow
            trailing.priority = .required
        } else {
            leading.priority = .required
            trailing.priority = .defaultLow
        }

        var constraints: [NSLayoutConstraint] = [
            row.heightAnchor.constraint(greaterThanOrEqualTo: bubble.heightAnchor),
            bubble.topAnchor.constraint(equalTo: row.topAnchor),
            bubble.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            leading,
            trailing,
            bubble.widthAnchor.constraint(lessThanOrEqualTo: row.widthAnchor, multiplier: answerLike ? 0.90 : (rightAligned ? 0.70 : 0.78)),
            bubble.widthAnchor.constraint(greaterThanOrEqualToConstant: answerLike ? 240 : 170),

            metaLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 10),
            metaLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),

            titleLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            titleLabel.leadingAnchor.constraint(equalTo: metaLabel.trailingAnchor, constant: 8),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -10),

            statusLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            statusLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),

            bodyLabel.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 8),
            bodyLabel.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
            bodyLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),
        ]

        if showFix {
            // Compact cyan "Fix" affordance under an agent answer. Asks the
            // attached agent to PROPOSE a fix for this answer (Slice F4).
            let fixButton = NSButton(title: "Fix", target: self, action: #selector(fixButtonClicked(_:)))
            fixButton.translatesAutoresizingMaskIntoConstraints = false
            fixButton.identifier = NSUserInterfaceItemIdentifier(card.id)
            fixButton.toolTip = "Ask your agent to propose a fix for this answer"
            styleFixButton(fixButton)
            bubble.addSubview(fixButton)
            registerInteractive(fixButton)
            constraints.append(contentsOf: [
                fixButton.topAnchor.constraint(equalTo: bodyLabel.bottomAnchor, constant: 10),
                fixButton.leadingAnchor.constraint(equalTo: metaLabel.leadingAnchor),
                fixButton.heightAnchor.constraint(equalToConstant: 26),
                fixButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 64),
                fixButton.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12),
            ])
        } else {
            constraints.append(
                bodyLabel.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -12))
        }

        NSLayoutConstraint.activate(constraints)
        return row
    }

    /// Small cyan-accented pill used for the per-answer **Fix** button. Matches
    /// the agent UI's compact-control look (Slice 5b) at a smaller scale.
    private func styleFixButton(_ button: NSButton) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 13
        button.layer?.backgroundColor = BlueyTheme.cyanSoft.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.45).cgColor
        button.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        button.contentTintColor = BlueyTheme.cyan
        if let image = symbolImage("wrench.and.screwdriver") {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageLeading
            button.imageHugsTitle = true
            button.imageScaling = .scaleProportionallyDown
        }
        button.attributedTitle = NSAttributedString(
            string: "Fix",
            attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .bold),
                .foregroundColor: BlueyTheme.cyan,
            ])
        button.alignment = .center
    }

    @objc private func fixButtonClicked(_ sender: NSButton) {
        guard let cardId = sender.identifier?.rawValue,
              let card = cards.first(where: { $0.id == cardId })
        else { return }
        onFixRequested?(cardId, card.body)
    }

    // MARK: Fix proposal card (Slice F4)

    /// Render a review-gated Fix proposal: DIAGNOSIS / REASONING / FIX sections
    /// (FIX shown as a monospace diff block when a unified diff is present) plus
    /// Approve / Reject. Approve is disabled when the agent can't apply. The
    /// `state` drives the terminal "Applying…" / "Discarded" presentation.
    private func makeFixProposalView(_ proposal: FixProposal, state: FixProposalState) -> NSView {
        let warn = BlueyTheme.warning
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false

        let bubble = NSView()
        bubble.wantsLayer = true
        bubble.layer?.backgroundColor = BlueyTheme.surface.cgColor
        bubble.layer?.cornerRadius = 14
        bubble.layer?.borderWidth = 1
        // A distinct warning/amber accent sets the review-gated proposal apart
        // from ordinary cyan answer cards.
        bubble.layer?.borderColor = warn.withAlphaComponent(0.45).cgColor
        bubble.layer?.shadowColor = NSColor.black.cgColor
        bubble.layer?.shadowOpacity = 0.16
        bubble.layer?.shadowRadius = 10
        bubble.layer?.shadowOffset = NSSize(width: 0, height: -4)
        bubble.translatesAutoresizingMaskIntoConstraints = false

        let metaLabel = NSTextField(labelWithString: "PROPOSED FIX")
        metaLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        metaLabel.textColor = warn
        metaLabel.translatesAutoresizingMaskIntoConstraints = false

        let stateLabel = NSTextField(labelWithString: fixStateBadge(state))
        stateLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .semibold)
        stateLabel.textColor = BlueyTheme.textDim
        stateLabel.translatesAutoresizingMaskIntoConstraints = false

        // Vertical content stack: the three labeled sections, then the diff (if
        // any), then the action row.
        let content = NSStackView()
        content.orientation = .vertical
        content.alignment = .leading
        content.spacing = 10
        content.translatesAutoresizingMaskIntoConstraints = false

        // Each section/diff/action fills the content width so wrapping labels
        // wrap at the bubble edge instead of taking intrinsic width.
        func addFullWidth(_ view: NSView) {
            content.addArrangedSubview(view)
            view.widthAnchor.constraint(equalTo: content.widthAnchor).isActive = true
        }

        addFullWidth(makeFixSection(title: "DIAGNOSIS", body: proposal.diagnosis))
        addFullWidth(makeFixSection(title: "REASONING", body: proposal.reasoning))

        if let diff = proposal.diff, !diff.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            addFullWidth(makeFixSectionHeader("FIX"))
            addFullWidth(makeDiffBlock(diff))
        } else {
            addFullWidth(makeFixSection(title: "FIX", body: proposal.fix))
        }

        let actionRow = makeFixActionRow(proposal: proposal, state: state)
        addFullWidth(actionRow)

        row.addSubview(bubble)
        bubble.addSubview(metaLabel)
        bubble.addSubview(stateLabel)
        bubble.addSubview(content)

        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(greaterThanOrEqualTo: bubble.heightAnchor),
            bubble.topAnchor.constraint(equalTo: row.topAnchor),
            bubble.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            bubble.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 8),
            bubble.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8),

            metaLabel.topAnchor.constraint(equalTo: bubble.topAnchor, constant: 12),
            metaLabel.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),

            stateLabel.centerYAnchor.constraint(equalTo: metaLabel.centerYAnchor),
            stateLabel.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),
            stateLabel.leadingAnchor.constraint(greaterThanOrEqualTo: metaLabel.trailingAnchor, constant: 8),

            content.topAnchor.constraint(equalTo: metaLabel.bottomAnchor, constant: 10),
            content.leadingAnchor.constraint(equalTo: bubble.leadingAnchor, constant: 14),
            content.trailingAnchor.constraint(equalTo: bubble.trailingAnchor, constant: -14),
            content.bottomAnchor.constraint(equalTo: bubble.bottomAnchor, constant: -14),
        ])
        return row
    }

    private func fixStateBadge(_ state: FixProposalState) -> String {
        switch state {
        case .pending:   return "awaiting review"
        case .applying:  return "applying…"
        case .discarded: return "discarded"
        }
    }

    private func makeFixSectionHeader(_ title: String) -> NSView {
        let label = NSTextField(labelWithString: title)
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = NSFont.systemFont(ofSize: 10, weight: .heavy)
        label.textColor = BlueyTheme.cyan
        return label
    }

    private func makeFixSection(title: String, body: String) -> NSView {
        let container = NSStackView()
        container.orientation = .vertical
        container.alignment = .leading
        container.spacing = 3
        container.translatesAutoresizingMaskIntoConstraints = false

        container.addArrangedSubview(makeFixSectionHeader(title))

        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        let bodyLabel = NSTextField(wrappingLabelWithString: text.isEmpty ? "—" : text)
        bodyLabel.translatesAutoresizingMaskIntoConstraints = false
        bodyLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .regular)
        bodyLabel.textColor = BlueyTheme.text
        bodyLabel.preferredMaxLayoutWidth = 460
        container.addArrangedSubview(bodyLabel)
        bodyLabel.widthAnchor.constraint(equalTo: container.widthAnchor).isActive = true
        return container
    }

    /// Monospace diff block. `+`/`-` lines are tinted green/red; hunk headers
    /// (`@@`) cyan; everything else dim. Plain monospace if coloring fails.
    private func makeDiffBlock(_ diff: String) -> NSView {
        let panel = NSView()
        panel.translatesAutoresizingMaskIntoConstraints = false
        panel.wantsLayer = true
        panel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        panel.layer?.cornerRadius = 8
        panel.layer?.borderWidth = 1
        panel.layer?.borderColor = BlueyTheme.hairline.cgColor

        let label = NSTextField(labelWithString: "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.attributedStringValue = attributedDiff(diff)
        label.isEditable = false
        label.isSelectable = true
        label.drawsBackground = false
        label.isBezeled = false
        label.lineBreakMode = .byClipping
        label.maximumNumberOfLines = 0
        label.preferredMaxLayoutWidth = 440

        panel.addSubview(label)
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: panel.topAnchor, constant: 8),
            label.leadingAnchor.constraint(equalTo: panel.leadingAnchor, constant: 10),
            label.trailingAnchor.constraint(equalTo: panel.trailingAnchor, constant: -10),
            label.bottomAnchor.constraint(equalTo: panel.bottomAnchor, constant: -8),
        ])
        return panel
    }

    private func attributedDiff(_ diff: String) -> NSAttributedString {
        let mono = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let result = NSMutableAttributedString()
        let lines = diff.components(separatedBy: "\n")
        for (idx, line) in lines.enumerated() {
            let color: NSColor
            if line.hasPrefix("+++") || line.hasPrefix("---") {
                color = BlueyTheme.textDim
            } else if line.hasPrefix("@@") {
                color = BlueyTheme.cyan
            } else if line.hasPrefix("+") {
                color = BlueyTheme.green
            } else if line.hasPrefix("-") {
                color = NSColor(red: 1.0, green: 0.45, blue: 0.45, alpha: 1.0)
            } else {
                color = BlueyTheme.textDim
            }
            let suffix = idx == lines.count - 1 ? "" : "\n"
            result.append(NSAttributedString(
                string: line + suffix,
                attributes: [.font: mono, .foregroundColor: color]))
        }
        return result
    }

    private func makeFixActionRow(proposal: FixProposal, state: FixProposalState) -> NSView {
        let container = NSStackView()
        container.orientation = .vertical
        container.alignment = .leading
        container.spacing = 5
        container.translatesAutoresizingMaskIntoConstraints = false

        let buttonRow = NSStackView()
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = 8
        buttonRow.translatesAutoresizingMaskIntoConstraints = false

        let pending = { if case .pending = state { return true }; return false }()
        let approveEnabled = pending && proposal.applySupported

        let approve = NSButton(title: "Approve", target: self, action: #selector(approveFixClicked(_:)))
        approve.translatesAutoresizingMaskIntoConstraints = false
        approve.identifier = NSUserInterfaceItemIdentifier(proposal.proposalId)
        approve.isEnabled = approveEnabled
        styleFixActionButton(approve, symbol: "checkmark", primary: true, enabled: approveEnabled)
        buttonRow.addArrangedSubview(approve)
        registerInteractive(approve)

        let reject = NSButton(title: "Reject", target: self, action: #selector(rejectFixClicked(_:)))
        reject.translatesAutoresizingMaskIntoConstraints = false
        reject.identifier = NSUserInterfaceItemIdentifier(proposal.proposalId)
        reject.isEnabled = pending
        styleFixActionButton(reject, symbol: "xmark", primary: false, enabled: pending)
        buttonRow.addArrangedSubview(reject)
        registerInteractive(reject)

        NSLayoutConstraint.activate([
            approve.heightAnchor.constraint(equalToConstant: 30),
            approve.widthAnchor.constraint(greaterThanOrEqualToConstant: 104),
            reject.heightAnchor.constraint(equalToConstant: 30),
            reject.widthAnchor.constraint(greaterThanOrEqualToConstant: 96),
        ])

        container.addArrangedSubview(buttonRow)

        // Caption: explain a disabled Approve, or echo the terminal decision.
        let captionText: String?
        switch state {
        case .pending:
            captionText = proposal.applySupported
                ? nil
                : "This agent can't apply automatically"
        case .applying:
            captionText = "Applying… sent to your agent"
        case .discarded:
            captionText = "Discarded — nothing was applied"
        }
        if let captionText {
            let caption = NSTextField(labelWithString: captionText)
            caption.translatesAutoresizingMaskIntoConstraints = false
            caption.font = NSFont.systemFont(ofSize: 10, weight: .medium)
            caption.textColor = state == .applying ? BlueyTheme.cyan : BlueyTheme.textDim
            caption.lineBreakMode = .byTruncatingTail
            container.addArrangedSubview(caption)
        }
        return container
    }

    private func styleFixActionButton(_ button: NSButton, symbol: String, primary: Bool, enabled: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 15
        let baseFill: NSColor = primary
            ? NSColor(red: 0.07, green: 0.19, blue: 0.24, alpha: 0.98)
            : NSColor.white.withAlphaComponent(0.070)
        let baseBorder: NSColor = primary
            ? BlueyTheme.cyan.withAlphaComponent(0.55)
            : NSColor.white.withAlphaComponent(0.12)
        button.layer?.backgroundColor = baseFill.cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = baseBorder.cgColor
        button.alphaValue = enabled ? 1.0 : 0.4
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        let titleColor: NSColor = primary ? BlueyTheme.text : BlueyTheme.textDim
        button.attributedTitle = NSAttributedString(
            string: button.title,
            attributes: [
                .font: NSFont.systemFont(ofSize: 12, weight: .bold),
                .foregroundColor: titleColor,
            ])
        button.contentTintColor = primary ? BlueyTheme.cyan : BlueyTheme.textDim
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.image = image
            button.imagePosition = .imageLeading
            button.imageHugsTitle = true
            button.imageScaling = .scaleProportionallyDown
        }
        button.alignment = .center
    }

    @objc private func approveFixClicked(_ sender: NSButton) {
        guard sender.isEnabled, let proposalId = sender.identifier?.rawValue else { return }
        // One-shot: flip to Applying (disables both buttons) before emitting so
        // a fast double-click can't re-submit.
        setFixState(proposalId: proposalId, to: .applying)
        emitFixApprovalResponded(proposalId: proposalId, approved: true)
    }

    @objc private func rejectFixClicked(_ sender: NSButton) {
        guard sender.isEnabled, let proposalId = sender.identifier?.rawValue else { return }
        setFixState(proposalId: proposalId, to: .discarded)
        emitFixApprovalResponded(proposalId: proposalId, approved: false)
    }

    private func kindLabel(_ card: RenderedCard) -> String {
        switch card.kind {
        case "answer":
            // Agent-mediated answers badge the agent (CLAUDE / CURSOR) instead
            // of BLUEY; the cyan rail stays (Bluey-mediated). Slice 5b.
            return agentLabel(from: card.source) ?? "BLUEY"
        case "question":    return "YOU"
        case "action_item": return "ACTION"
        case "decision":    return "DECISION"
        case "context":     return "CONTEXT"
        case "transcript":  return "TRANSCRIPT"
        case "warning":     return "WARNING"
        case "system":      return "SYSTEM"
        default:            return "BLUEY"
        }
    }

    /// Detect a coding-agent provenance inside a free-form CueCard.source and
    /// return its uppercase badge label, or nil for plain Bluey answers.
    private func agentLabel(from source: String?) -> String? {
        guard let source, !source.isEmpty else { return nil }
        let lower = source.lowercased()
        // Only treat as an agent answer when the source actually signals one.
        guard lower.contains("agent") || lower.contains("claude_code")
            || lower.contains("cursor") || lower.contains("codex")
            || lower.contains("gemini") || lower.contains("windsurf")
            || lower.contains("aider")
        else { return nil }
        let known: [(needle: String, label: String)] = [
            ("claude_code", "CLAUDE"),
            ("claude", "CLAUDE"),
            ("cursor", "CURSOR"),
            ("codex", "CODEX"),
            ("gemini", "GEMINI"),
            ("windsurf", "WINDSURF"),
            ("aider", "AIDER"),
        ]
        for entry in known where lower.contains(entry.needle) {
            return entry.label
        }
        return "AGENT"
    }

    private func displayTitle(for card: RenderedCard) -> String {
        switch card.kind {
        case "answer", "question", "transcript":
            return ""
        default:
            return card.title.isEmpty ? kindTitle(card.kind) : card.title
        }
    }

    private func isUserSide(_ card: RenderedCard) -> Bool {
        card.kind == "question" || card.kind == "transcript"
    }

    private func bodyFont(for card: RenderedCard) -> NSFont {
        return NSFont.systemFont(ofSize: 13.5, weight: .regular)
    }

    private func chatBody(for card: RenderedCard, rawBody: String) -> String {
        guard card.kind == "answer" else { return rawBody }

        if let artifact = card.artifact {
            if artifact.artifactType == "code" {
                let notes = stripFencedCode(from: rawBody)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return notes.isEmpty
                    ? "I opened the code in the canvas."
                    : notes + "\n\nCode opened in the canvas."
            }
            if rawBody.count > 1_100 {
                let prefix = String(rawBody.prefix(720))
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return prefix + "\n\nFull \(artifact.title.lowercased()) opened in the canvas."
            }
        }

        if rawBody.contains("```") {
            let notes = stripFencedCode(from: rawBody)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if notes.isEmpty {
                return "I opened the code in the canvas."
            }
            return notes + "\n\nCode opened in the canvas."
        }

        if rawBody.count > 1_100 && hasStructuredShape(rawBody) {
            let prefix = String(rawBody.prefix(720))
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return prefix + "\n\nFull structured version opened in the canvas."
        }

        return rawBody
    }

    private func stripFencedCode(from text: String) -> String {
        var lines: [String] = []
        var inFence = false
        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                inFence.toggle()
                continue
            }
            if !inFence {
                lines.append(line)
            }
        }
        return lines.joined(separator: "\n")
    }

    private func hasStructuredShape(_ text: String) -> Bool {
        let lines = text.components(separatedBy: .newlines)
        let structured = lines.filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("- ")
                || trimmed.hasPrefix("* ")
                || trimmed.hasPrefix("#")
                || trimmed.range(of: #"^\d+[\.\)]\s"#, options: .regularExpression) != nil
        }
        return structured.count >= 3
    }

    private func statusText(for card: RenderedCard) -> String {
        if !card.done { return "streaming..." }
        if let costLabel = card.costLabel, !costLabel.isEmpty { return costLabel }
        switch card.kind {
        case "answer":
            // No cost label yet: name the agent that produced the answer.
            if let label = agentLabel(from: card.source) {
                return "answered by your \(label.lowercased())"
            }
            return ""
        case "question": return "sent"
        default:         return ""
        }
    }

    private func kindTitle(_ kind: String) -> String {
        switch kind {
        case "answer":      return "Response"
        case "question":    return "Question"
        case "action_item": return "Action item"
        case "decision":    return "Decision"
        case "context":     return "Context attached"
        case "transcript":  return "Transcript"
        case "warning":     return "Needs attention"
        case "system":      return "Bluey"
        default:            return "Update"
        }
    }

    private func scrollToBottom() {
        DispatchQueue.main.async { [weak self] in
            guard let s = self else { return }
            let bottom = NSPoint(x: 0, y: max(0, s.stack.bounds.height - s.scroll.contentView.bounds.height))
            s.scroll.contentView.scroll(to: bottom)
            s.scroll.reflectScrolledClipView(s.scroll.contentView)
        }
    }
}

// MARK: - Canvas pane

private final class CanvasPaneView: NSView {
    private let header = NSView()
    private let iconView = NSImageView()
    private let titleLabel = NSTextField(labelWithString: "Workspace")
    private let subtitleLabel = NSTextField(labelWithString: "Structured output appears here")
    private let closeButton = NSButton(title: "", target: nil, action: nil)
    private let scroll = NSScrollView()
    private let textView = NSTextView()

    var onCollapse: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor(red: 0.020, green: 0.024, blue: 0.031, alpha: 0.98).cgColor
        layer?.cornerRadius = 16
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.22).cgColor

        header.translatesAutoresizingMaskIntoConstraints = false
        iconView.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header)
        header.addSubview(iconView)
        header.addSubview(titleLabel)
        header.addSubview(subtitleLabel)
        header.addSubview(closeButton)
        addSubview(scroll)

        iconView.imageScaling = .scaleProportionallyDown
        iconView.contentTintColor = BlueyTheme.cyan

        titleLabel.font = NSFont.systemFont(ofSize: 12.5, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        subtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        subtitleLabel.textColor = BlueyTheme.textDim
        subtitleLabel.lineBreakMode = .byTruncatingTail

        closeButton.isBordered = false
        closeButton.wantsLayer = true
        closeButton.layer?.cornerRadius = 12
        closeButton.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        closeButton.layer?.borderWidth = 1
        closeButton.layer?.borderColor = BlueyTheme.hairline.cgColor
        closeButton.contentTintColor = BlueyTheme.textDim
        if let image = symbolImage("chevron.right") {
            image.isTemplate = true
            closeButton.image = image
            closeButton.imagePosition = .imageOnly
        } else {
            closeButton.title = "<"
        }
        closeButton.toolTip = "Collapse canvas"
        closeButton.target = self
        closeButton.action = #selector(collapseClicked)

        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textColor = BlueyTheme.text
        textView.font = NSFont.monospacedSystemFont(ofSize: 12.2, weight: .regular)
        textView.textContainerInset = NSSize(width: 12, height: 12)
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude)

        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.documentView = textView
        scroll.scrollerStyle = .overlay

        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: topAnchor, constant: 12),
            header.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            header.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            header.heightAnchor.constraint(equalToConstant: 48),

            iconView.leadingAnchor.constraint(equalTo: header.leadingAnchor),
            iconView.topAnchor.constraint(equalTo: header.topAnchor, constant: 3),
            iconView.widthAnchor.constraint(equalToConstant: 22),
            iconView.heightAnchor.constraint(equalToConstant: 22),

            closeButton.trailingAnchor.constraint(equalTo: header.trailingAnchor),
            closeButton.topAnchor.constraint(equalTo: header.topAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            titleLabel.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 8),
            titleLabel.topAnchor.constraint(equalTo: header.topAnchor),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: closeButton.leadingAnchor, constant: -8),

            subtitleLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 3),
            subtitleLabel.trailingAnchor.constraint(equalTo: header.trailingAnchor),

            scroll.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 8),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            scroll.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            scroll.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    func render(_ artifact: CanvasArtifact) {
        titleLabel.stringValue = artifact.title
        subtitleLabel.stringValue = artifact.subtitle
        if let image = symbolImage(artifact.kind.icon) {
            image.isTemplate = true
            iconView.image = image
        }
        textView.string = artifact.content
        textView.scrollRangeToVisible(NSRange(location: 0, length: 0))
    }

    @objc private func collapseClicked() {
        onCollapse?()
    }
}

// MARK: - Expanded panel (feed + composer)

private final class ExpandedPanelView: NSView {
    let feed: FeedView
    let workspace: NSView
    let canvasPane: CanvasPaneView
    let headerBar: NSView
    let headerStack: NSStackView
    let brandStack: NSStackView
    let headerSpacer: NSView
    let titleLabel: NSTextField
    let statusLabel: NSTextField
    let modelMenu: NSPopUpButton
    let routeBadge: NSTextField
    let knowledgeBadge: NSTextField
    let balanceLabel: NSTextField
    let canvasToggleButton: NSButton
    let navButton: NSButton
    let newSessionButton: NSButton
    let sessionDrawer: NSView
    let drawerTitleLabel: NSTextField
    let drawerSubtitleLabel: NSTextField
    let latestSessionButton: NSButton
    let sessionScroll: NSScrollView
    let sessionStack: NSStackView
    // Agent-bridge surfaces (Slice 5b). A parallel drawer mirroring
    // sessionDrawer's geometry, plus a bottom connector sheet reusing the
    // close-confirm overlay pattern.
    let agentDrawer: NSView
    let agentDrawerTitleLabel: NSTextField
    let agentDrawerBackButton: NSButton
    let agentDrawerCloseButton: NSButton
    let agentDrawerCaption: NSTextField
    let agentScroll: NSScrollView
    let agentStack: NSStackView
    let agentButton: NSButton
    let agentBadge: NSTextField
    let connectorSheetOverlay: NSView
    let connectorSheetPanel: NSView
    let connectorSheetTitle: NSTextField
    let connectorSheetSummary: NSTextField
    let connectorSheetScroll: NSScrollView
    let connectorSheetStack: NSStackView
    let connectorSheetCancelButton: NSButton
    let connectorSheetAttachButton: NSButton
    let answerStyleOverlay: NSView
    let answerStylePanel: NSView
    let answerStyleLabel: NSTextField
    let answerStyleBox: NSTextField
    let answerStyleSaveButton: NSButton
    let transcriptStrip: NSView
    let transcriptActivityDot: NSView
    let transcriptStateLabel: NSTextField
    let transcriptScroll: NSScrollView
    let transcriptLabel: NSTextField
    let attachmentStrip: NSScrollView
    let attachmentStack: NSStackView
    let composerBar: NSView
    let composerSurface: NSView
    let composer: ComposerTextView
    let recordingButton: NSButton
    let askButton: NSButton
    let analyzeButton: NSButton
    let attachButton: NSButton
    let instructionsButton: NSButton
    let opacityControl: NSView
    let opacityLabel: NSTextField
    let opacitySlider: NSSlider
    let opacityValueLabel: NSTextField
    let hideButton: NSButton
    let closeButton: NSButton
    let closeConfirmOverlay: NSView
    let closeConfirmPanel: NSView
    let closeConfirmTitle: NSTextField
    let closeConfirmBody: NSTextField
    let closeConfirmCancelButton: NSButton
    let closeConfirmTurnOffButton: NSButton

    var onClose: (() -> Void)?
    var onOpacityChanged: ((Double) -> Void)?
    /// Notifies the coordinator when an agent attaches/detaches so the pill can
    /// show or hide its glyph-only agent badge (Slice 5b).
    var onAgentAttachmentChanged: ((Bool) -> Void)?
    private var recordingActive = false
    private var transcriptSnippets: [String] = []
    private var sessionItems: [OverlaySessionItem] = []
    private var editingSessionId: String?
    private var renameField: NSTextField?
    // Agent-bridge state (Slice 5b).
    private enum AgentDrawerStage {
        case picker
        case sessions(kind: String, displayName: String)
    }
    private var agentDrawerStage: AgentDrawerStage = .picker
    private var agentSummaries: [AgentSummary] = []
    private var agentListLoaded = false
    private var agentSessions: [AgentSessionSummary] = []
    private var agentSessionsLoaded = false
    private var attachedAgentKind: String?
    private var pendingConnectorKind: String?
    private var pendingConnectorSessionId: String?
    private var pendingConnectorInfos: [AgentConnectorInfo] = []
    private var pendingConnectorsLoaded = false
    private var canvasWidthConstraint: NSLayoutConstraint?
    private var composerBarHeightConstraint: NSLayoutConstraint?
    private var composerTextHeightConstraint: NSLayoutConstraint?
    private var attachmentStripHeightConstraint: NSLayoutConstraint?
    private var latestCanvas: CanvasArtifact?
    private var canvasOpen = false
    override init(frame frameRect: NSRect) {
        feed = FeedView(frame: .zero)
        workspace = NSView()
        canvasPane = CanvasPaneView(frame: .zero)
        headerBar = NSView()
        headerStack = NSStackView()
        brandStack = NSStackView()
        headerSpacer = NSView()
        titleLabel = NSTextField(labelWithString: "Bluey")
        statusLabel = NSTextField(labelWithString: "New recording")
        modelMenu = NSPopUpButton(frame: .zero, pullsDown: false)
        routeBadge = NSTextField(labelWithString: "Auto · ready")
        knowledgeBadge = NSTextField(labelWithString: "KB empty")
        balanceLabel = NSTextField(labelWithString: "Balance --")
        canvasToggleButton = NSButton(title: "", target: nil, action: nil)
        navButton = NSButton(title: "", target: nil, action: nil)
        newSessionButton = NSButton(title: "", target: nil, action: nil)
        sessionDrawer = NSView()
        drawerTitleLabel = NSTextField(labelWithString: "Recordings")
        drawerSubtitleLabel = NSTextField(labelWithString: "Click to continue. Pencil to rename.")
        latestSessionButton = NSButton(title: "Continue latest", target: nil, action: nil)
        sessionScroll = NSScrollView()
        sessionStack = NSStackView()
        agentDrawer = NSView()
        agentDrawerTitleLabel = NSTextField(labelWithString: "Coding agents")
        agentDrawerBackButton = NSButton(title: "", target: nil, action: nil)
        agentDrawerCloseButton = NSButton(title: "", target: nil, action: nil)
        agentDrawerCaption = NSTextField(
            wrappingLabelWithString: "Answers run on your machine — your agent replies.")
        agentScroll = NSScrollView()
        agentStack = NSStackView()
        agentButton = NSButton(title: "Agent", target: nil, action: nil)
        agentBadge = NSTextField(labelWithString: "No agent")
        connectorSheetOverlay = NSView()
        connectorSheetPanel = NSView()
        connectorSheetTitle = NSTextField(labelWithString: "Inherited connectors")
        connectorSheetSummary = NSTextField(labelWithString: "")
        connectorSheetScroll = NSScrollView()
        connectorSheetStack = NSStackView()
        connectorSheetCancelButton = NSButton(title: "Cancel", target: nil, action: nil)
        connectorSheetAttachButton = NSButton(title: "Attach", target: nil, action: nil)
        answerStyleOverlay = NSView()
        answerStylePanel = NSView()
        answerStyleLabel = NSTextField(labelWithString: "How Bluey should answer")
        answerStyleBox = NSTextField()
        answerStyleSaveButton = NSButton(title: "Save style", target: nil, action: nil)
        transcriptStrip = NSView()
        transcriptActivityDot = NSView()
        transcriptStateLabel = NSTextField(labelWithString: "IDLE")
        transcriptScroll = NSScrollView()
        transcriptLabel = NSTextField(labelWithString: "Live captions preview")
        attachmentStrip = NSScrollView()
        attachmentStack = NSStackView()
        composerBar = NSView()
        composerSurface = ComposerSurfaceView()
        composer = ComposerTextView(frame: .zero, textContainer: nil)
        recordingButton = NSButton(title: "Listen", target: nil, action: nil)
        askButton = NSButton(title: "", target: nil, action: nil)
        analyzeButton = NSButton(title: "Screen", target: nil, action: nil)
        attachButton = NSButton(title: "", target: nil, action: nil)
        instructionsButton = NSButton(title: "Style", target: nil, action: nil)
        opacityControl = NSView()
        opacityLabel = NSTextField(labelWithString: "Opacity")
        opacitySlider = NSSlider(value: 0.94, minValue: 0.50, maxValue: 1.0, target: nil, action: nil)
        opacityValueLabel = NSTextField(labelWithString: "94%")
        hideButton = NSButton(title: "", target: nil, action: nil)
        closeButton = NSButton(title: "x", target: nil, action: nil)
        closeConfirmOverlay = NSView()
        closeConfirmPanel = NSView()
        closeConfirmTitle = NSTextField(labelWithString: "Turn Bluey off?")
        closeConfirmBody = NSTextField(wrappingLabelWithString: "This closes Bluey completely. To start again, run: bluey on")
        closeConfirmCancelButton = NSButton(title: "Cancel", target: nil, action: nil)
        closeConfirmTurnOffButton = NSButton(title: "Turn Off", target: nil, action: nil)

        super.init(frame: frameRect)

        wantsLayer = true
        layer?.backgroundColor = NSColor(red: 0.010, green: 0.012, blue: 0.016, alpha: 0.94).cgColor
        layer?.cornerRadius = 24
        layer?.borderWidth = 1
        layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.14).cgColor
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.28
        layer?.shadowRadius = 24
        layer?.shadowOffset = .zero

        configureHeader()
        configureContextRows()
        configureComposer()
        configureFixedChromeLayoutPriorities()
        configureCloseConfirm()
        styleDrawer()
        feed.onTranscript = { [weak self] card in
            self?.appendTranscriptSnippet(card)
        }
        // Tapping Fix on an agent answer asks the daemon to drive a propose-only
        // fix; nothing is applied until the proposal card is approved (Slice F4).
        feed.onFixRequested = { cardId, question in
            emitFixRequested(cardId: cardId, question: question)
        }

        for view in [
            headerBar,
            headerStack,
            brandStack,
            headerSpacer,
            titleLabel,
            statusLabel,
            modelMenu,
            routeBadge,
            knowledgeBadge,
            balanceLabel,
            canvasToggleButton,
            navButton,
            newSessionButton,
            sessionDrawer,
            drawerTitleLabel,
            drawerSubtitleLabel,
            latestSessionButton,
            sessionScroll,
            sessionStack,
            agentDrawer,
            agentDrawerTitleLabel,
            agentDrawerBackButton,
            agentDrawerCloseButton,
            agentDrawerCaption,
            agentScroll,
            agentStack,
            agentButton,
            agentBadge,
            connectorSheetOverlay,
            connectorSheetPanel,
            connectorSheetTitle,
            connectorSheetSummary,
            connectorSheetScroll,
            connectorSheetStack,
            connectorSheetCancelButton,
            connectorSheetAttachButton,
            answerStyleOverlay,
            answerStylePanel,
            answerStyleLabel,
            answerStyleBox,
            answerStyleSaveButton,
            workspace,
            feed,
            canvasPane,
            transcriptStrip,
            transcriptActivityDot,
            transcriptStateLabel,
            transcriptScroll,
            transcriptLabel,
            attachmentStrip,
            attachmentStack,
            composerBar,
            composerSurface,
            composer,
            recordingButton,
            askButton,
            analyzeButton,
            attachButton,
            instructionsButton,
            opacityControl,
            opacityLabel,
            opacitySlider,
            opacityValueLabel,
            hideButton,
            closeButton,
            closeConfirmOverlay,
            closeConfirmPanel,
            closeConfirmTitle,
            closeConfirmBody,
            closeConfirmCancelButton,
            closeConfirmTurnOffButton,
        ] {
            view.translatesAutoresizingMaskIntoConstraints = false
        }

        headerBar.addSubview(headerStack)
        brandStack.addArrangedSubview(titleLabel)
        brandStack.addArrangedSubview(statusLabel)
        for view in [
            navButton,
            newSessionButton,
            brandStack,
            routeBadge,
            knowledgeBadge,
            agentBadge,
            headerSpacer,
            canvasToggleButton,
            balanceLabel,
            hideButton,
            closeButton,
        ] {
            headerStack.addArrangedSubview(view)
        }
        addSubview(workspace)
        workspace.addSubview(feed)
        workspace.addSubview(canvasPane)
        addSubview(sessionDrawer)
        sessionDrawer.addSubview(drawerTitleLabel)
        sessionDrawer.addSubview(drawerSubtitleLabel)
        sessionDrawer.addSubview(latestSessionButton)
        sessionDrawer.addSubview(sessionScroll)
        addSubview(agentDrawer)
        agentDrawer.addSubview(agentDrawerBackButton)
        agentDrawer.addSubview(agentDrawerTitleLabel)
        agentDrawer.addSubview(agentDrawerCloseButton)
        agentDrawer.addSubview(agentScroll)
        agentDrawer.addSubview(agentDrawerCaption)
        addSubview(answerStyleOverlay)
        answerStyleOverlay.addSubview(answerStylePanel)
        answerStylePanel.addSubview(answerStyleLabel)
        answerStylePanel.addSubview(answerStyleBox)
        answerStylePanel.addSubview(answerStyleSaveButton)
        addSubview(transcriptStrip)
        transcriptStrip.addSubview(transcriptActivityDot)
        transcriptStrip.addSubview(transcriptStateLabel)
        transcriptStrip.addSubview(transcriptScroll)
        transcriptScroll.documentView = transcriptLabel
        transcriptLabel.translatesAutoresizingMaskIntoConstraints = true
        addSubview(attachmentStrip)
        addSubview(composerBar)
        composerBar.addSubview(composerSurface)
        composerSurface.addSubview(composer)
        composerBar.addSubview(attachButton)
        composerBar.addSubview(agentButton)
        composerBar.addSubview(instructionsButton)
        composerBar.addSubview(recordingButton)
        composerBar.addSubview(opacityControl)
        opacityControl.addSubview(opacityLabel)
        opacityControl.addSubview(opacitySlider)
        opacityControl.addSubview(opacityValueLabel)
        composerBar.addSubview(modelMenu)
        composerBar.addSubview(analyzeButton)
        composerBar.addSubview(askButton)
        addSubview(headerBar, positioned: .above, relativeTo: nil)
        addSubview(connectorSheetOverlay)
        connectorSheetOverlay.addSubview(connectorSheetPanel)
        connectorSheetPanel.addSubview(connectorSheetTitle)
        connectorSheetPanel.addSubview(connectorSheetSummary)
        connectorSheetPanel.addSubview(connectorSheetScroll)
        connectorSheetPanel.addSubview(connectorSheetCancelButton)
        connectorSheetPanel.addSubview(connectorSheetAttachButton)
        addSubview(closeConfirmOverlay)
        closeConfirmOverlay.addSubview(closeConfirmPanel)
        closeConfirmPanel.addSubview(closeConfirmTitle)
        closeConfirmPanel.addSubview(closeConfirmBody)
        closeConfirmPanel.addSubview(closeConfirmCancelButton)
        closeConfirmPanel.addSubview(closeConfirmTurnOffButton)
        // Keep the fixed chrome rows above transparent scroll/canvas surfaces
        // even when AppKit re-lays out the dense center workspace.
        headerBar.layer?.zPosition = 50
        transcriptStrip.layer?.zPosition = 40
        attachmentStrip.layer?.zPosition = 40
        composerBar.layer?.zPosition = 50

        // Bind the agent scroll views' document stacks before activating the
        // stack-in-clip-view constraints below (they need a shared ancestor).
        // Full visual styling happens later in styleAgentSurfaces().
        agentScroll.documentView = agentStack
        connectorSheetScroll.documentView = connectorSheetStack

        let canvasWidth = canvasPane.widthAnchor.constraint(equalToConstant: 0)
        canvasWidthConstraint = canvasWidth
        let composerTextHeight = composerSurface.heightAnchor.constraint(equalToConstant: 46)
        let composerBarHeight = composerBar.heightAnchor.constraint(equalToConstant: 108)
        let attachmentStripHeight = attachmentStrip.heightAnchor.constraint(equalToConstant: 0)
        composerTextHeightConstraint = composerTextHeight
        composerBarHeightConstraint = composerBarHeight
        attachmentStripHeightConstraint = attachmentStripHeight

        NSLayoutConstraint.activate([
            headerBar.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            headerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            headerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            headerBar.heightAnchor.constraint(equalToConstant: 42),

            headerStack.leadingAnchor.constraint(equalTo: headerBar.leadingAnchor, constant: 9),
            headerStack.trailingAnchor.constraint(equalTo: headerBar.trailingAnchor, constant: -9),
            headerStack.topAnchor.constraint(equalTo: headerBar.topAnchor, constant: 4),
            headerStack.bottomAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: -4),

            navButton.widthAnchor.constraint(equalToConstant: 30),
            navButton.heightAnchor.constraint(equalToConstant: 30),

            newSessionButton.widthAnchor.constraint(equalToConstant: 30),
            newSessionButton.heightAnchor.constraint(equalToConstant: 30),

            brandStack.widthAnchor.constraint(greaterThanOrEqualToConstant: 110),
            brandStack.widthAnchor.constraint(lessThanOrEqualToConstant: 162),

            routeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 96),
            routeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 142),
            routeBadge.heightAnchor.constraint(equalToConstant: 26),

            knowledgeBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 94),
            knowledgeBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 136),
            knowledgeBadge.heightAnchor.constraint(equalToConstant: 26),

            closeButton.widthAnchor.constraint(equalToConstant: 26),
            closeButton.heightAnchor.constraint(equalToConstant: 26),

            hideButton.widthAnchor.constraint(equalToConstant: 26),
            hideButton.heightAnchor.constraint(equalToConstant: 26),

            balanceLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 84),
            balanceLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 116),
            balanceLabel.heightAnchor.constraint(equalToConstant: 26),

            canvasToggleButton.widthAnchor.constraint(equalToConstant: 30),
            canvasToggleButton.heightAnchor.constraint(equalToConstant: 30),

            workspace.topAnchor.constraint(equalTo: headerBar.bottomAnchor, constant: 8),
            workspace.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            workspace.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            workspace.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -8),

            feed.topAnchor.constraint(equalTo: workspace.topAnchor),
            feed.leadingAnchor.constraint(equalTo: workspace.leadingAnchor),
            feed.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            feed.widthAnchor.constraint(greaterThanOrEqualToConstant: 260),

            canvasPane.topAnchor.constraint(equalTo: workspace.topAnchor),
            canvasPane.leadingAnchor.constraint(equalTo: feed.trailingAnchor, constant: 8),
            canvasPane.trailingAnchor.constraint(equalTo: workspace.trailingAnchor),
            canvasPane.bottomAnchor.constraint(equalTo: workspace.bottomAnchor),
            canvasWidth,

            sessionDrawer.topAnchor.constraint(equalTo: feed.topAnchor, constant: 10),
            sessionDrawer.leadingAnchor.constraint(equalTo: feed.leadingAnchor, constant: 10),
            sessionDrawer.widthAnchor.constraint(equalToConstant: 190),
            sessionDrawer.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -10),

            drawerTitleLabel.topAnchor.constraint(equalTo: sessionDrawer.topAnchor, constant: 14),
            drawerTitleLabel.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 14),
            drawerTitleLabel.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -14),

            drawerSubtitleLabel.topAnchor.constraint(equalTo: drawerTitleLabel.bottomAnchor, constant: 4),
            drawerSubtitleLabel.leadingAnchor.constraint(equalTo: drawerTitleLabel.leadingAnchor),
            drawerSubtitleLabel.trailingAnchor.constraint(equalTo: drawerTitleLabel.trailingAnchor),

            latestSessionButton.topAnchor.constraint(equalTo: drawerSubtitleLabel.bottomAnchor, constant: 16),
            latestSessionButton.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 12),
            latestSessionButton.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -12),
            latestSessionButton.heightAnchor.constraint(equalToConstant: 32),

            sessionScroll.topAnchor.constraint(equalTo: latestSessionButton.bottomAnchor, constant: 10),
            sessionScroll.leadingAnchor.constraint(equalTo: sessionDrawer.leadingAnchor, constant: 8),
            sessionScroll.trailingAnchor.constraint(equalTo: sessionDrawer.trailingAnchor, constant: -8),
            sessionScroll.bottomAnchor.constraint(equalTo: sessionDrawer.bottomAnchor, constant: -12),

            sessionStack.leadingAnchor.constraint(equalTo: sessionScroll.contentView.leadingAnchor),
            sessionStack.topAnchor.constraint(equalTo: sessionScroll.contentView.topAnchor),
            sessionStack.trailingAnchor.constraint(equalTo: sessionScroll.contentView.trailingAnchor),
            sessionStack.bottomAnchor.constraint(lessThanOrEqualTo: sessionScroll.contentView.bottomAnchor),
            sessionStack.widthAnchor.constraint(equalTo: sessionScroll.widthAnchor),

            // Agent drawer mirrors sessionDrawer geometry, widened to 230pt
            // (PLAN §9) so capability chips + connector counts truncate-tail.
            agentDrawer.topAnchor.constraint(equalTo: feed.topAnchor, constant: 10),
            agentDrawer.leadingAnchor.constraint(equalTo: feed.leadingAnchor, constant: 10),
            agentDrawer.widthAnchor.constraint(equalToConstant: 230),
            agentDrawer.bottomAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: -10),

            agentDrawerBackButton.topAnchor.constraint(equalTo: agentDrawer.topAnchor, constant: 12),
            agentDrawerBackButton.leadingAnchor.constraint(equalTo: agentDrawer.leadingAnchor, constant: 10),
            agentDrawerBackButton.widthAnchor.constraint(equalToConstant: 26),
            agentDrawerBackButton.heightAnchor.constraint(equalToConstant: 26),

            agentDrawerTitleLabel.centerYAnchor.constraint(equalTo: agentDrawerBackButton.centerYAnchor),
            agentDrawerTitleLabel.leadingAnchor.constraint(equalTo: agentDrawerBackButton.trailingAnchor, constant: 8),
            agentDrawerTitleLabel.trailingAnchor.constraint(equalTo: agentDrawerCloseButton.leadingAnchor, constant: -8),

            agentDrawerCloseButton.centerYAnchor.constraint(equalTo: agentDrawerBackButton.centerYAnchor),
            agentDrawerCloseButton.trailingAnchor.constraint(equalTo: agentDrawer.trailingAnchor, constant: -10),
            agentDrawerCloseButton.widthAnchor.constraint(equalToConstant: 26),
            agentDrawerCloseButton.heightAnchor.constraint(equalToConstant: 26),

            agentScroll.topAnchor.constraint(equalTo: agentDrawerBackButton.bottomAnchor, constant: 10),
            agentScroll.leadingAnchor.constraint(equalTo: agentDrawer.leadingAnchor, constant: 8),
            agentScroll.trailingAnchor.constraint(equalTo: agentDrawer.trailingAnchor, constant: -8),
            agentScroll.bottomAnchor.constraint(equalTo: agentDrawerCaption.topAnchor, constant: -8),

            agentStack.leadingAnchor.constraint(equalTo: agentScroll.contentView.leadingAnchor),
            agentStack.topAnchor.constraint(equalTo: agentScroll.contentView.topAnchor),
            agentStack.trailingAnchor.constraint(equalTo: agentScroll.contentView.trailingAnchor),
            agentStack.bottomAnchor.constraint(lessThanOrEqualTo: agentScroll.contentView.bottomAnchor),
            agentStack.widthAnchor.constraint(equalTo: agentScroll.widthAnchor),

            agentDrawerCaption.leadingAnchor.constraint(equalTo: agentDrawer.leadingAnchor, constant: 12),
            agentDrawerCaption.trailingAnchor.constraint(equalTo: agentDrawer.trailingAnchor, constant: -12),
            agentDrawerCaption.bottomAnchor.constraint(equalTo: agentDrawer.bottomAnchor, constant: -12),

            agentBadge.heightAnchor.constraint(equalToConstant: 26),
            agentBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 92),
            agentBadge.widthAnchor.constraint(lessThanOrEqualToConstant: 158),

            answerStyleOverlay.topAnchor.constraint(equalTo: topAnchor),
            answerStyleOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            answerStyleOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            answerStyleOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            answerStylePanel.centerXAnchor.constraint(equalTo: answerStyleOverlay.centerXAnchor),
            answerStylePanel.centerYAnchor.constraint(equalTo: answerStyleOverlay.centerYAnchor),
            answerStylePanel.widthAnchor.constraint(equalToConstant: 360),

            answerStyleLabel.topAnchor.constraint(equalTo: answerStylePanel.topAnchor, constant: 18),
            answerStyleLabel.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleLabel.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),

            answerStyleBox.topAnchor.constraint(equalTo: answerStyleLabel.bottomAnchor, constant: 12),
            answerStyleBox.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleBox.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),
            answerStyleBox.heightAnchor.constraint(equalToConstant: 48),

            answerStyleSaveButton.topAnchor.constraint(equalTo: answerStyleBox.bottomAnchor, constant: 14),
            answerStyleSaveButton.leadingAnchor.constraint(equalTo: answerStylePanel.leadingAnchor, constant: 18),
            answerStyleSaveButton.trailingAnchor.constraint(equalTo: answerStylePanel.trailingAnchor, constant: -18),
            answerStyleSaveButton.bottomAnchor.constraint(equalTo: answerStylePanel.bottomAnchor, constant: -18),
            answerStyleSaveButton.heightAnchor.constraint(equalToConstant: 36),

            transcriptStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            transcriptStrip.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            transcriptStrip.bottomAnchor.constraint(equalTo: attachmentStrip.topAnchor, constant: -6),
            transcriptStrip.heightAnchor.constraint(equalToConstant: 26),

            transcriptActivityDot.leadingAnchor.constraint(equalTo: transcriptStrip.leadingAnchor, constant: 11),
            transcriptActivityDot.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptActivityDot.widthAnchor.constraint(equalToConstant: 7),
            transcriptActivityDot.heightAnchor.constraint(equalToConstant: 7),

            transcriptStateLabel.leadingAnchor.constraint(equalTo: transcriptActivityDot.trailingAnchor, constant: 7),
            transcriptStateLabel.centerYAnchor.constraint(equalTo: transcriptStrip.centerYAnchor),
            transcriptStateLabel.widthAnchor.constraint(equalToConstant: 88),

            transcriptScroll.topAnchor.constraint(equalTo: transcriptStrip.topAnchor, constant: 2),
            transcriptScroll.leadingAnchor.constraint(equalTo: transcriptStateLabel.trailingAnchor, constant: 8),
            transcriptScroll.trailingAnchor.constraint(equalTo: transcriptStrip.trailingAnchor, constant: -10),
            transcriptScroll.bottomAnchor.constraint(equalTo: transcriptStrip.bottomAnchor, constant: -2),

            attachmentStrip.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            attachmentStrip.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            attachmentStrip.bottomAnchor.constraint(equalTo: composerBar.topAnchor, constant: -6),
            attachmentStripHeight,

            attachmentStack.leadingAnchor.constraint(equalTo: attachmentStrip.contentView.leadingAnchor),
            attachmentStack.topAnchor.constraint(equalTo: attachmentStrip.contentView.topAnchor),
            attachmentStack.bottomAnchor.constraint(equalTo: attachmentStrip.contentView.bottomAnchor),
            attachmentStack.heightAnchor.constraint(equalTo: attachmentStrip.heightAnchor),

            composerBar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            composerBar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            composerBar.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            composerBarHeight,

            composerSurface.topAnchor.constraint(equalTo: composerBar.topAnchor, constant: 10),
            composerSurface.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 14),
            composerSurface.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -14),
            composerTextHeight,

            composer.topAnchor.constraint(equalTo: composerSurface.topAnchor, constant: 3),
            composer.leadingAnchor.constraint(equalTo: composerSurface.leadingAnchor, constant: 14),
            composer.trailingAnchor.constraint(equalTo: composerSurface.trailingAnchor, constant: -14),
            composer.bottomAnchor.constraint(equalTo: composerSurface.bottomAnchor, constant: -3),

            attachButton.leadingAnchor.constraint(equalTo: composerBar.leadingAnchor, constant: 14),
            attachButton.bottomAnchor.constraint(equalTo: composerBar.bottomAnchor, constant: -10),
            attachButton.widthAnchor.constraint(equalToConstant: 36),
            attachButton.heightAnchor.constraint(equalToConstant: 36),

            agentButton.leadingAnchor.constraint(equalTo: attachButton.trailingAnchor, constant: 8),
            agentButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            agentButton.widthAnchor.constraint(equalToConstant: 84),
            agentButton.heightAnchor.constraint(equalToConstant: 36),

            instructionsButton.leadingAnchor.constraint(equalTo: agentButton.trailingAnchor, constant: 8),
            instructionsButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            instructionsButton.widthAnchor.constraint(equalToConstant: 86),
            instructionsButton.heightAnchor.constraint(equalToConstant: 36),

            recordingButton.leadingAnchor.constraint(equalTo: instructionsButton.trailingAnchor, constant: 8),
            recordingButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            recordingButton.widthAnchor.constraint(equalToConstant: 94),
            recordingButton.heightAnchor.constraint(equalToConstant: 36),

            opacityControl.leadingAnchor.constraint(equalTo: recordingButton.trailingAnchor, constant: 8),
            opacityControl.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            opacityControl.widthAnchor.constraint(equalToConstant: 96),
            opacityControl.heightAnchor.constraint(equalToConstant: 36),

            opacityLabel.leadingAnchor.constraint(equalTo: opacityControl.leadingAnchor, constant: 10),
            opacityLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityLabel.widthAnchor.constraint(equalToConstant: 18),

            opacitySlider.leadingAnchor.constraint(equalTo: opacityLabel.trailingAnchor, constant: 5),
            opacitySlider.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacitySlider.trailingAnchor.constraint(equalTo: opacityValueLabel.leadingAnchor, constant: -5),
            opacitySlider.heightAnchor.constraint(equalToConstant: 20),

            opacityValueLabel.trailingAnchor.constraint(equalTo: opacityControl.trailingAnchor, constant: -8),
            opacityValueLabel.centerYAnchor.constraint(equalTo: opacityControl.centerYAnchor),
            opacityValueLabel.widthAnchor.constraint(equalToConstant: 28),

            askButton.trailingAnchor.constraint(equalTo: composerBar.trailingAnchor, constant: -8),
            askButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            askButton.widthAnchor.constraint(equalToConstant: 40),
            askButton.heightAnchor.constraint(equalToConstant: 40),

            analyzeButton.trailingAnchor.constraint(equalTo: askButton.leadingAnchor, constant: -7),
            analyzeButton.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            analyzeButton.widthAnchor.constraint(equalToConstant: 88),
            analyzeButton.heightAnchor.constraint(equalToConstant: 36),

            modelMenu.trailingAnchor.constraint(equalTo: analyzeButton.leadingAnchor, constant: -7),
            modelMenu.centerYAnchor.constraint(equalTo: attachButton.centerYAnchor),
            modelMenu.widthAnchor.constraint(greaterThanOrEqualToConstant: 118),
            modelMenu.widthAnchor.constraint(lessThanOrEqualToConstant: 144),
            modelMenu.heightAnchor.constraint(equalToConstant: 34),

            opacityControl.trailingAnchor.constraint(lessThanOrEqualTo: modelMenu.leadingAnchor, constant: -10),

            closeConfirmOverlay.topAnchor.constraint(equalTo: topAnchor),
            closeConfirmOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            closeConfirmOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            closeConfirmOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            closeConfirmPanel.centerXAnchor.constraint(equalTo: closeConfirmOverlay.centerXAnchor),
            closeConfirmPanel.centerYAnchor.constraint(equalTo: closeConfirmOverlay.centerYAnchor),
            closeConfirmPanel.widthAnchor.constraint(equalToConstant: 360),

            closeConfirmTitle.topAnchor.constraint(equalTo: closeConfirmPanel.topAnchor, constant: 18),
            closeConfirmTitle.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmTitle.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),

            closeConfirmBody.topAnchor.constraint(equalTo: closeConfirmTitle.bottomAnchor, constant: 8),
            closeConfirmBody.leadingAnchor.constraint(equalTo: closeConfirmTitle.leadingAnchor),
            closeConfirmBody.trailingAnchor.constraint(equalTo: closeConfirmTitle.trailingAnchor),

            closeConfirmCancelButton.topAnchor.constraint(equalTo: closeConfirmBody.bottomAnchor, constant: 18),
            closeConfirmCancelButton.leadingAnchor.constraint(equalTo: closeConfirmPanel.leadingAnchor, constant: 18),
            closeConfirmCancelButton.bottomAnchor.constraint(equalTo: closeConfirmPanel.bottomAnchor, constant: -18),
            closeConfirmCancelButton.widthAnchor.constraint(equalToConstant: 150),
            closeConfirmCancelButton.heightAnchor.constraint(equalToConstant: 34),

            closeConfirmTurnOffButton.topAnchor.constraint(equalTo: closeConfirmCancelButton.topAnchor),
            closeConfirmTurnOffButton.leadingAnchor.constraint(equalTo: closeConfirmCancelButton.trailingAnchor, constant: 12),
            closeConfirmTurnOffButton.trailingAnchor.constraint(equalTo: closeConfirmPanel.trailingAnchor, constant: -18),
            closeConfirmTurnOffButton.heightAnchor.constraint(equalTo: closeConfirmCancelButton.heightAnchor),

            // Connector inheritance sheet — bottom confirm, reusing the
            // close-confirm overlay pattern (PLAN §9 surface 3).
            connectorSheetOverlay.topAnchor.constraint(equalTo: topAnchor),
            connectorSheetOverlay.leadingAnchor.constraint(equalTo: leadingAnchor),
            connectorSheetOverlay.trailingAnchor.constraint(equalTo: trailingAnchor),
            connectorSheetOverlay.bottomAnchor.constraint(equalTo: bottomAnchor),

            connectorSheetPanel.centerXAnchor.constraint(equalTo: connectorSheetOverlay.centerXAnchor),
            connectorSheetPanel.centerYAnchor.constraint(equalTo: connectorSheetOverlay.centerYAnchor),
            connectorSheetPanel.widthAnchor.constraint(equalToConstant: 380),

            connectorSheetTitle.topAnchor.constraint(equalTo: connectorSheetPanel.topAnchor, constant: 18),
            connectorSheetTitle.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 18),
            connectorSheetTitle.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -18),

            connectorSheetSummary.topAnchor.constraint(equalTo: connectorSheetTitle.bottomAnchor, constant: 6),
            connectorSheetSummary.leadingAnchor.constraint(equalTo: connectorSheetTitle.leadingAnchor),
            connectorSheetSummary.trailingAnchor.constraint(equalTo: connectorSheetTitle.trailingAnchor),

            connectorSheetScroll.topAnchor.constraint(equalTo: connectorSheetSummary.bottomAnchor, constant: 12),
            connectorSheetScroll.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 14),
            connectorSheetScroll.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -14),
            connectorSheetScroll.heightAnchor.constraint(equalToConstant: 168),

            connectorSheetStack.leadingAnchor.constraint(equalTo: connectorSheetScroll.contentView.leadingAnchor),
            connectorSheetStack.topAnchor.constraint(equalTo: connectorSheetScroll.contentView.topAnchor),
            connectorSheetStack.trailingAnchor.constraint(equalTo: connectorSheetScroll.contentView.trailingAnchor),
            connectorSheetStack.bottomAnchor.constraint(lessThanOrEqualTo: connectorSheetScroll.contentView.bottomAnchor),
            connectorSheetStack.widthAnchor.constraint(equalTo: connectorSheetScroll.widthAnchor),

            connectorSheetCancelButton.topAnchor.constraint(equalTo: connectorSheetScroll.bottomAnchor, constant: 14),
            connectorSheetCancelButton.leadingAnchor.constraint(equalTo: connectorSheetPanel.leadingAnchor, constant: 18),
            connectorSheetCancelButton.bottomAnchor.constraint(equalTo: connectorSheetPanel.bottomAnchor, constant: -18),
            connectorSheetCancelButton.widthAnchor.constraint(equalToConstant: 158),
            connectorSheetCancelButton.heightAnchor.constraint(equalToConstant: 34),

            connectorSheetAttachButton.topAnchor.constraint(equalTo: connectorSheetCancelButton.topAnchor),
            connectorSheetAttachButton.leadingAnchor.constraint(equalTo: connectorSheetCancelButton.trailingAnchor, constant: 12),
            connectorSheetAttachButton.trailingAnchor.constraint(equalTo: connectorSheetPanel.trailingAnchor, constant: -18),
            connectorSheetAttachButton.heightAnchor.constraint(equalTo: connectorSheetCancelButton.heightAnchor),
        ])

        navButton.target = self
        navButton.action = #selector(toggleSessionsClicked)
        canvasToggleButton.target = self
        canvasToggleButton.action = #selector(toggleCanvasClicked)
        newSessionButton.target = self
        newSessionButton.action = #selector(newSessionClicked)
        latestSessionButton.target = self
        latestSessionButton.action = #selector(continueSessionClicked)
        answerStyleSaveButton.target = self
        answerStyleSaveButton.action = #selector(saveAnswerStyleClicked)
        hideButton.target = self
        hideButton.action = #selector(hideClicked)
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeConfirmCancelButton.target = self
        closeConfirmCancelButton.action = #selector(cancelCloseConfirmClicked)
        closeConfirmTurnOffButton.target = self
        closeConfirmTurnOffButton.action = #selector(confirmTurnOffClicked)
        opacitySlider.target = self
        opacitySlider.action = #selector(opacityChanged)
        composer.onSubmit = { [weak self] in self?.askClicked() }
        composer.onMeasuredHeight = { [weak self] height in self?.setComposerTextHeight(height) }
        // Clicking the rounded surface (not just the glyphs) focuses the composer.
        (composerSurface as? ComposerSurfaceView)?.composer = composer
        recordingButton.target = self
        recordingButton.action = #selector(recordingClicked)
        askButton.target = self
        askButton.action = #selector(askClicked)
        analyzeButton.target = self
        analyzeButton.action = #selector(analyzeClicked)
        attachButton.target = self
        attachButton.action = #selector(attachClicked)
        instructionsButton.target = self
        instructionsButton.action = #selector(instructionsClicked)
        agentButton.target = self
        agentButton.action = #selector(agentClicked)
        agentBadge.addGestureRecognizer(
            NSClickGestureRecognizer(target: self, action: #selector(agentBadgeClicked)))
        agentDrawerBackButton.target = self
        agentDrawerBackButton.action = #selector(agentDrawerBackClicked)
        agentDrawerCloseButton.target = self
        agentDrawerCloseButton.action = #selector(agentDrawerCloseClicked)
        connectorSheetCancelButton.target = self
        connectorSheetCancelButton.action = #selector(connectorSheetCancelClicked)
        connectorSheetAttachButton.target = self
        connectorSheetAttachButton.action = #selector(connectorSheetAttachClicked)

        sessionDrawer.isHidden = true
        agentDrawer.isHidden = true
        connectorSheetOverlay.isHidden = true
        answerStyleOverlay.isHidden = true
        canvasPane.isHidden = true
        canvasToggleButton.isHidden = true
        agentBadge.isHidden = true
        canvasPane.onCollapse = { [weak self] in self?.setCanvasOpen(false) }
        styleHeaderIconButton(navButton, symbol: "sidebar.left", fallback: "[]")
        styleHeaderIconButton(canvasToggleButton, symbol: "sidebar.right", fallback: "|")
        styleHeaderIconButton(newSessionButton, symbol: "square.and.pencil", fallback: "+")
        styleControlButton(latestSessionButton, symbol: "clock.arrow.circlepath", accent: false)
        styleControlButton(answerStyleSaveButton, symbol: "checkmark", accent: true)
        styleControlButton(recordingButton, symbol: "waveform", accent: false)
        styleControlButton(instructionsButton, symbol: "text.bubble", accent: false)
        styleIconButton(attachButton, symbol: "plus", fallback: "+")
        styleControlButton(agentButton, symbol: "cpu", accent: false)
        styleControlButton(analyzeButton, symbol: "sparkle.magnifyingglass", accent: false)
        styleIconButton(askButton, symbol: "arrow.up", fallback: "↑", accent: true)
        styleHeaderIconButton(hideButton, symbol: "eye.slash", fallback: "-")
        styleHeaderIconButton(closeButton, symbol: "xmark", fallback: "x")
        styleAgentSurfaces()
        configureTooltips()
        setContextItems([])
        setTranscriptState("IDLE", active: false)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func layout() {
        super.layout()
        keepFixedChromeInBounds()
        resizeTranscriptLabelToContent()
    }

    func isInteractiveAtScreenPoint(_ screenPoint: NSPoint) -> Bool {
        guard let window else { return false }
        let windowPoint = window.convertPoint(fromScreen: screenPoint)
        let localPoint = convert(windowPoint, from: nil)
        guard bounds.contains(localPoint) else { return false }

        if !closeConfirmOverlay.isHidden {
            return true
        }
        if !connectorSheetOverlay.isHidden {
            return true
        }
        if !answerStyleOverlay.isHidden {
            return true
        }
        if headerBar.frame.contains(localPoint) || composerBar.frame.contains(localPoint) {
            return true
        }
        if !sessionDrawer.isHidden && sessionDrawer.frame.contains(localPoint) {
            return true
        }
        if !agentDrawer.isHidden && agentDrawer.frame.contains(localPoint) {
            return true
        }
        // Card affordances (Fix / Approve / Reject) live inside the otherwise
        // click-through feed: capture the mouse only over an enabled button so
        // the rest of the feed stays transparent to the app underneath (F4).
        let feedPoint = feed.convert(windowPoint, from: nil)
        if feed.hasInteractiveControl(at: feedPoint) {
            return true
        }
        return false
    }

    /// The expanded overlay is a bounded, resizable tool surface. Header,
    /// transcript, attachments, and composer are chrome; only the workspace
    /// may compress/scroll as content grows.
    private func configureFixedChromeLayoutPriorities() {
        for chrome in [headerBar, transcriptStrip, attachmentStrip, composerBar] {
            chrome.setContentHuggingPriority(.required, for: .vertical)
            chrome.setContentCompressionResistancePriority(.required, for: .vertical)
        }
        workspace.setContentHuggingPriority(.defaultLow, for: .vertical)
        workspace.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        feed.setContentHuggingPriority(.defaultLow, for: .vertical)
        feed.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        canvasPane.setContentHuggingPriority(.defaultLow, for: .vertical)
        canvasPane.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
    }

    private func keepFixedChromeInBounds() {
        // Defensive guard for AppKit/autolayout edge cases: if a dense feed,
        // drawer, or growing composer ever tries to push the header out of the
        // content rect, restore the fixed chrome frame immediately instead of
        // letting the user lose navigation/model/balance controls.
        guard bounds.height >= ExpandedPanelMetrics.minHeight else { return }
        headerBar.isHidden = false
        headerBar.layer?.zPosition = 1_000
        headerStack.layer?.zPosition = 1_001
        addSubview(headerBar, positioned: .above, relativeTo: nil)
        headerBar.frame = NSRect(
            x: 10,
            y: bounds.height - 52,
            width: max(0, bounds.width - 20),
            height: 42)
        headerStack.frame = headerBar.bounds.insetBy(dx: 9, dy: 4)
        if composerBar.frame.minY < 0 || composerBar.frame.maxY > bounds.height {
            let height = composerBarHeightConstraint?.constant ?? 108
            composerBar.frame = NSRect(
                x: 10,
                y: 10,
                width: max(0, bounds.width - 20),
                height: height)
        }
    }

    private func configureHeader() {
        headerBar.wantsLayer = true
        headerBar.layer?.backgroundColor = NSColor(red: 0.018, green: 0.022, blue: 0.030, alpha: 0.92).cgColor
        headerBar.layer?.cornerRadius = 21
        headerBar.layer?.borderWidth = 1
        headerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor
        headerBar.layer?.shadowColor = NSColor.black.cgColor
        headerBar.layer?.shadowOpacity = 0.18
        headerBar.layer?.shadowRadius = 14
        headerBar.layer?.shadowOffset = NSSize(width: 0, height: -6)

        headerStack.orientation = .horizontal
        headerStack.alignment = .centerY
        headerStack.distribution = .fill
        headerStack.spacing = 7
        headerSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        headerSpacer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        brandStack.orientation = .vertical
        brandStack.alignment = .leading
        brandStack.distribution = .fill
        brandStack.spacing = -1
        brandStack.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        brandStack.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        titleLabel.font = NSFont.systemFont(ofSize: 13.5, weight: .bold)
        titleLabel.textColor = BlueyTheme.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1
        titleLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        statusLabel.font = NSFont.systemFont(ofSize: 9.5, weight: .semibold)
        statusLabel.textColor = BlueyTheme.textDim
        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.maximumNumberOfLines = 1
        statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        modelMenu.addItems(withTitles: ["Auto", "Instant", "Balanced", "Deep"])
        modelMenu.selectItem(at: 0)
        modelMenu.isBordered = false
        modelMenu.wantsLayer = true
        modelMenu.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.075).cgColor
        modelMenu.layer?.cornerRadius = 15
        modelMenu.layer?.borderWidth = 1
        modelMenu.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        modelMenu.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        modelMenu.contentTintColor = BlueyTheme.text
        modelMenu.setContentHuggingPriority(.defaultLow, for: .horizontal)
        modelMenu.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        styleHeaderBadge(routeBadge, textColor: BlueyTheme.cyan)
        routeBadge.toolTip = "Auto Router classification and selected lane"

        styleHeaderBadge(knowledgeBadge, textColor: BlueyTheme.text)
        knowledgeBadge.toolTip = "Knowledge base status for attached documents"

        balanceLabel.font = NSFont.monospacedSystemFont(ofSize: 10.5, weight: .bold)
        balanceLabel.textColor = BlueyTheme.text
        balanceLabel.alignment = .center
        balanceLabel.lineBreakMode = .byTruncatingMiddle
        balanceLabel.maximumNumberOfLines = 1
        balanceLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        balanceLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        balanceLabel.wantsLayer = true
        balanceLabel.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.055).cgColor
        balanceLabel.layer?.cornerRadius = 14
        balanceLabel.layer?.borderWidth = 1
        balanceLabel.layer?.borderColor = NSColor.white.withAlphaComponent(0.10).cgColor
    }

    private func configureContextRows() {
        transcriptStrip.wantsLayer = true
        transcriptStrip.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.16).cgColor
        transcriptStrip.layer?.cornerRadius = 13
        transcriptStrip.layer?.borderWidth = 1
        transcriptStrip.layer?.borderColor = BlueyTheme.hairline.cgColor

        transcriptActivityDot.wantsLayer = true
        transcriptActivityDot.layer?.cornerRadius = 3.5
        transcriptActivityDot.layer?.backgroundColor = BlueyTheme.textDim.withAlphaComponent(0.55).cgColor
        transcriptActivityDot.layer?.shadowColor = BlueyTheme.cyan.cgColor
        transcriptActivityDot.layer?.shadowOpacity = 0
        transcriptActivityDot.layer?.shadowRadius = 7
        transcriptActivityDot.layer?.shadowOffset = .zero

        transcriptStateLabel.font = NSFont.monospacedSystemFont(ofSize: 9.5, weight: .bold)
        transcriptStateLabel.textColor = BlueyTheme.textDim
        transcriptStateLabel.alignment = .left
        transcriptStateLabel.lineBreakMode = .byTruncatingTail

        transcriptScroll.drawsBackground = false
        transcriptScroll.hasVerticalScroller = false
        transcriptScroll.hasHorizontalScroller = true
        transcriptScroll.autohidesScrollers = true
        transcriptScroll.borderType = .noBorder
        transcriptScroll.scrollerStyle = .overlay

        transcriptLabel.isBezeled = false
        transcriptLabel.drawsBackground = false
        transcriptLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        transcriptLabel.textColor = BlueyTheme.textDim
        transcriptLabel.lineBreakMode = .byClipping
        transcriptLabel.maximumNumberOfLines = 1
        transcriptLabel.alignment = .left
        if let cell = transcriptLabel.cell as? NSTextFieldCell {
            cell.isScrollable = true
            cell.wraps = false
            cell.lineBreakMode = .byClipping
        }

        attachmentStack.orientation = .horizontal
        attachmentStack.alignment = .centerY
        attachmentStack.spacing = 6
        attachmentStack.edgeInsets = NSEdgeInsets(top: 3, left: 4, bottom: 3, right: 4)

        attachmentStrip.drawsBackground = false
        attachmentStrip.hasVerticalScroller = false
        attachmentStrip.hasHorizontalScroller = true
        attachmentStrip.autohidesScrollers = true
        attachmentStrip.borderType = .noBorder
        attachmentStrip.documentView = attachmentStack
        attachmentStrip.scrollerStyle = .overlay
        attachmentStrip.isHidden = true
    }

    private func styleDrawer() {
        sessionDrawer.wantsLayer = true
        sessionDrawer.layer?.backgroundColor = NSColor(red: 0.035, green: 0.040, blue: 0.050, alpha: 0.98).cgColor
        sessionDrawer.layer?.cornerRadius = 16
        sessionDrawer.layer?.borderWidth = 1
        sessionDrawer.layer?.borderColor = BlueyTheme.hairline.cgColor
        sessionDrawer.layer?.shadowColor = NSColor.black.cgColor
        sessionDrawer.layer?.shadowOpacity = 0.26
        sessionDrawer.layer?.shadowRadius = 18
        sessionDrawer.layer?.shadowOffset = NSSize(width: 0, height: -8)
        sessionDrawer.layer?.zPosition = 10

        answerStyleOverlay.isHidden = true
        answerStyleOverlay.wantsLayer = true
        answerStyleOverlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.44).cgColor
        answerStyleOverlay.layer?.zPosition = 90

        answerStylePanel.wantsLayer = true
        answerStylePanel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        answerStylePanel.layer?.cornerRadius = 18
        answerStylePanel.layer?.borderWidth = 1
        answerStylePanel.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
        answerStylePanel.layer?.shadowColor = NSColor.black.cgColor
        answerStylePanel.layer?.shadowOpacity = 0.32
        answerStylePanel.layer?.shadowRadius = 20
        answerStylePanel.layer?.shadowOffset = .zero

        drawerTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        drawerTitleLabel.textColor = BlueyTheme.text
        drawerSubtitleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .medium)
        drawerSubtitleLabel.textColor = BlueyTheme.textDim
        drawerSubtitleLabel.lineBreakMode = .byWordWrapping
        drawerSubtitleLabel.maximumNumberOfLines = 2

        sessionStack.orientation = .vertical
        sessionStack.alignment = .centerX
        sessionStack.spacing = 6
        sessionStack.edgeInsets = NSEdgeInsets(top: 2, left: 0, bottom: 2, right: 0)

        sessionScroll.drawsBackground = false
        sessionScroll.hasVerticalScroller = true
        sessionScroll.hasHorizontalScroller = false
        sessionScroll.autohidesScrollers = true
        sessionScroll.borderType = .noBorder
        sessionScroll.documentView = sessionStack
        sessionScroll.scrollerStyle = .overlay

        answerStyleLabel.font = NSFont.systemFont(ofSize: 10.5, weight: .bold)
        answerStyleLabel.textColor = BlueyTheme.textDim
        answerStyleLabel.alignment = .center
        answerStyleLabel.stringValue = "Answer style"
        answerStyleBox.placeholderString = "Concise, structured, implementation-first..."
        answerStyleBox.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        answerStyleBox.isBezeled = false
        answerStyleBox.drawsBackground = false
        answerStyleBox.focusRingType = .none
        answerStyleBox.textColor = BlueyTheme.text
        answerStyleBox.alignment = .center
        answerStyleBox.placeholderAttributedString = NSAttributedString(
            string: "Concise, structured, implementation-first...",
            attributes: [.foregroundColor: BlueyTheme.textDim.withAlphaComponent(0.78)])
        answerStyleBox.wantsLayer = true
        answerStyleBox.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.18).cgColor
        answerStyleBox.layer?.cornerRadius = 10
        answerStyleBox.layer?.borderWidth = 1
        answerStyleBox.layer?.borderColor = BlueyTheme.hairline.cgColor
    }

    private func configureComposer() {
        composerBar.wantsLayer = true
        composerBar.layer?.backgroundColor = NSColor(red: 0.014, green: 0.016, blue: 0.022, alpha: 0.94).cgColor
        composerBar.layer?.cornerRadius = 28
        composerBar.layer?.borderWidth = 1
        composerBar.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.22).cgColor
        composerBar.layer?.shadowColor = NSColor.black.cgColor
        composerBar.layer?.shadowOpacity = 0.22
        composerBar.layer?.shadowRadius = 18
        composerBar.layer?.shadowOffset = .zero

        composerSurface.wantsLayer = true
        composerSurface.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.050).cgColor
        composerSurface.layer?.cornerRadius = 20
        composerSurface.layer?.borderWidth = 1
        composerSurface.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor

        opacityControl.wantsLayer = true
        opacityControl.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.055).cgColor
        opacityControl.layer?.cornerRadius = 18
        opacityControl.layer?.borderWidth = 1
        opacityControl.layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
        opacityControl.toolTip = "Overlay opacity"
        opacityLabel.stringValue = "%"
        opacityLabel.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
        opacityLabel.textColor = BlueyTheme.textDim
        opacityLabel.alignment = .left
        opacityValueLabel.font = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .semibold)
        opacityValueLabel.textColor = BlueyTheme.textDim
        opacityValueLabel.alignment = .right
        opacitySlider.controlSize = .small
        opacitySlider.wantsLayer = true
        opacitySlider.toolTip = "Overlay opacity"

        composer.placeholder = "Ask anything..."
        composer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        composer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        for control in [
            recordingButton,
            opacityControl,
            instructionsButton,
            attachButton,
            analyzeButton,
            askButton,
        ] {
            control.setContentHuggingPriority(.required, for: .horizontal)
            control.setContentCompressionResistancePriority(.required, for: .horizontal)
        }
    }

    private func configureCloseConfirm() {
        closeConfirmOverlay.isHidden = true
        closeConfirmOverlay.wantsLayer = true
        closeConfirmOverlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.52).cgColor
        closeConfirmOverlay.layer?.zPosition = 100

        closeConfirmPanel.wantsLayer = true
        closeConfirmPanel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        closeConfirmPanel.layer?.cornerRadius = 18
        closeConfirmPanel.layer?.borderWidth = 1
        closeConfirmPanel.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
        closeConfirmPanel.layer?.shadowColor = NSColor.black.cgColor
        closeConfirmPanel.layer?.shadowOpacity = 0.34
        closeConfirmPanel.layer?.shadowRadius = 22
        closeConfirmPanel.layer?.shadowOffset = .zero

        closeConfirmTitle.font = NSFont.systemFont(ofSize: 16, weight: .bold)
        closeConfirmTitle.textColor = BlueyTheme.text
        closeConfirmTitle.alignment = .center
        closeConfirmBody.font = NSFont.systemFont(ofSize: 12.5, weight: .medium)
        closeConfirmBody.textColor = BlueyTheme.textDim
        closeConfirmBody.alignment = .center
        closeConfirmBody.maximumNumberOfLines = 3

        styleControlButton(closeConfirmCancelButton, symbol: "xmark", accent: false)
        styleControlButton(closeConfirmTurnOffButton, symbol: "power", accent: true)
        closeConfirmCancelButton.toolTip = "Keep Bluey running"
        closeConfirmTurnOffButton.toolTip = "Turn Bluey off. Run bluey on to start again."
    }

    private func configureTooltips() {
        navButton.toolTip = "Show recordings"
        newSessionButton.toolTip = "Start a new recording"
        modelMenu.toolTip = "Choose routing lane"
        canvasToggleButton.toolTip = "Open or collapse the canvas"
        balanceLabel.toolTip = "Remaining Bluey balance"
        hideButton.toolTip = "Hide to pill"
        closeButton.toolTip = "Turn Bluey off. Run bluey on to start again."
        recordingButton.toolTip = "Start or stop listening"
        instructionsButton.toolTip = "How Bluey should answer"
        agentButton.toolTip = "Attach a coding agent to answer from your context"
        attachButton.toolTip = "Attach files"
        analyzeButton.toolTip = "Analyse screen"
        askButton.toolTip = "Send"
        latestSessionButton.toolTip = "Continue the latest recording"
        answerStyleSaveButton.toolTip = "Save answer style for this session"
    }

    private func styleControlButton(_ button: NSButton, symbol: String, accent: Bool) {
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.07, green: 0.19, blue: 0.24, alpha: 0.98).cgColor
            : NSColor.white.withAlphaComponent(0.070).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? BlueyTheme.cyan.withAlphaComponent(0.55) : NSColor.white.withAlphaComponent(0.12)).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.attributedTitle = NSAttributedString(
            string: button.title,
            attributes: [
                .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                .foregroundColor: BlueyTheme.text,
            ])
        button.contentTintColor = BlueyTheme.cyan
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.image = image
        }
        button.imagePosition = .imageLeading
        button.imageHugsTitle = true
        button.imageScaling = .scaleProportionallyDown
        button.alignment = .center
    }

    private func styleHeaderBadge(_ label: NSTextField, textColor: NSColor) {
        label.font = NSFont.systemFont(ofSize: 11.3, weight: .bold)
        label.textColor = textColor
        label.alignment = .center
        label.lineBreakMode = .byTruncatingMiddle
        label.maximumNumberOfLines = 1
        label.setContentHuggingPriority(.defaultLow, for: .horizontal)
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.wantsLayer = true
        label.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.052).cgColor
        label.layer?.cornerRadius = 13
        label.layer?.borderWidth = 1
        label.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.16).cgColor
    }

    private func styleHeaderIconButton(_ button: NSButton, symbol: String, fallback: String) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 15
        button.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.070).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = BlueyTheme.text
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.title = ""
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                    .foregroundColor: BlueyTheme.textDim,
                ])
        }
        button.imageHugsTitle = true
        button.alignment = .center
    }

    private func styleIconButton(_ button: NSButton, symbol: String, fallback: String, accent: Bool = false) {
        button.title = fallback
        button.isBordered = false
        button.wantsLayer = true
        button.layer?.cornerRadius = 16
        button.layer?.backgroundColor = accent
            ? NSColor(red: 0.84, green: 0.92, blue: 0.96, alpha: 0.95).cgColor
            : NSColor.white.withAlphaComponent(0.070).cgColor
        button.layer?.borderWidth = 1
        button.layer?.borderColor = (accent ? NSColor.white.withAlphaComponent(0.18) : NSColor.white.withAlphaComponent(0.12)).cgColor
        button.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        button.contentTintColor = accent ? NSColor.black.withAlphaComponent(0.82) : BlueyTheme.cyan
        if let image = symbolImage(symbol) {
            image.isTemplate = true
            button.title = ""
            button.image = image
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
        } else {
            button.attributedTitle = NSAttributedString(
                string: fallback,
                attributes: [
                    .font: button.font ?? NSFont.systemFont(ofSize: 12, weight: .bold),
                    .foregroundColor: BlueyTheme.text,
                ])
        }
        button.imageHugsTitle = true
        button.alignment = .center
    }

    @objc private func hideClicked() { onClose?() }

    @objc private func closeClicked() {
        closeConfirmOverlay.isHidden = false
        closeConfirmOverlay.alphaValue = 0
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            closeConfirmOverlay.animator().alphaValue = 1
        }
    }

    @objc private func cancelCloseConfirmClicked() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            closeConfirmOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.closeConfirmOverlay.isHidden = true
            self.closeConfirmOverlay.alphaValue = 1
        })
    }

    @objc private func confirmTurnOffClicked() {
        emitSimple("close_requested")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
            NSApp.terminate(nil)
        }
    }

    @objc private func opacityChanged() {
        applyOpacity(opacitySlider.doubleValue)
    }

    func applyOpacity(_ opacity: Double) {
        let value = min(max(opacity, 0.50), 1.0)
        if abs(opacitySlider.doubleValue - value) > 0.001 {
            opacitySlider.doubleValue = value
        }
        opacityValueLabel.stringValue = "\(Int((value * 100.0).rounded()))%"
        onOpacityChanged?(value)
    }

    private func setComposerTextHeight(_ rawHeight: CGFloat) {
        let textHeight = min(max(rawHeight, 46), 96)
        guard abs((composerTextHeightConstraint?.constant ?? 0) - textHeight) > 0.5 else { return }
        composerTextHeightConstraint?.constant = textHeight
        composerBarHeightConstraint?.constant = textHeight + 62
        needsLayout = true
        layoutSubtreeIfNeeded()
    }

    @objc private func toggleSessionsClicked() {
        // Sessions and agent drawers share the left rail — only one at a time.
        agentDrawer.isHidden = true
        sessionDrawer.isHidden.toggle()
        statusLabel.stringValue = sessionDrawer.isHidden ? statusLabel.stringValue : "Sessions"
    }

    @objc private func toggleCanvasClicked() {
        guard latestCanvas != nil else { return }
        setCanvasOpen(!canvasOpen)
    }

    @objc private func newSessionClicked() {
        resetSessionSurface()
        composer.clearText()
        statusLabel.stringValue = "New recording"
        sessionDrawer.isHidden = true
        emitSimple("session_new_requested")
    }

    @objc private func continueSessionClicked() {
        sessionDrawer.isHidden = true
        statusLabel.stringValue = "Latest recording"
        emitSimple("session_continue_requested")
    }

    @objc private func saveAnswerStyleClicked() {
        let text = answerStyleBox.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        emitInstructions(text: text)
        statusLabel.stringValue = text.isEmpty ? "Default style" : "Answer style saved"
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            answerStyleOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.answerStyleOverlay.isHidden = true
            self.answerStyleOverlay.alphaValue = 1
        })
    }

    @objc private func recordingClicked() {
        if recordingActive {
            emitSimple("recording_stop_requested")
            recordingActive = false
            recordingButton.title = "Listen"
            statusLabel.stringValue = "Paused"
            composer.placeholder = "Ask anything..."
            setTranscriptState("PAUSED", active: false)
            styleControlButton(recordingButton, symbol: "waveform", accent: false)
        } else {
            emitSimple("recording_start_requested")
            recordingActive = true
            recordingButton.title = "Stop"
            statusLabel.stringValue = "Listening"
            composer.placeholder = "Listening... type a follow-up anytime"
            setTranscriptState("LISTENING", active: true)
            styleControlButton(recordingButton, symbol: "stop.fill", accent: true)
        }
    }

    @objc private func askClicked() {
        let raw = composer.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let q = raw.isEmpty
            ? "Answer the latest clear question or useful context from this Bluey session."
            : raw
        composer.clearText()
        let route = selectedRoute()
        updateRouteBadge(for: q, selectedRoute: route)
        emitAsk(question: q, provider: route.provider, model: route.model, mode: route.mode)
    }

    @objc private func analyzeClicked() {
        routeBadge.stringValue = "Vision · deep"
        statusLabel.stringValue = "Reading screen"
        emitSimple("analyze_screen_requested")
    }

    @objc private func attachClicked() {
        setKnowledgeBadge("KB loading", accent: BlueyTheme.warning)
        showKnowledgePlaceholder("Indexing selected files...")
        emitSimple("attach_requested")
    }

    @objc private func instructionsClicked() {
        answerStyleOverlay.isHidden = false
        answerStyleOverlay.alphaValue = 0
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            answerStyleOverlay.animator().alphaValue = 1
        }
        window?.makeFirstResponder(answerStyleBox)
    }

    func setBalanceLabel(_ label: String) {
        let clean = label.trimmingCharacters(in: .whitespacesAndNewlines)
        balanceLabel.stringValue = clean.isEmpty ? "Balance --" : clean
    }

    private func setKnowledgeBadge(_ text: String, accent: NSColor) {
        knowledgeBadge.stringValue = text
        knowledgeBadge.textColor = accent
        knowledgeBadge.layer?.borderColor = accent.withAlphaComponent(0.30).cgColor
        knowledgeBadge.layer?.backgroundColor = accent.withAlphaComponent(0.08).cgColor
    }

    private func showKnowledgePlaceholder(_ text: String) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        attachmentStrip.isHidden = false
        attachmentStripHeightConstraint?.constant = 34

        let chip = NSTextField(labelWithString: text)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        chip.textColor = BlueyTheme.textDim
        chip.alignment = .center
        chip.lineBreakMode = .byTruncatingTail
        chip.wantsLayer = true
        chip.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.hairline.cgColor
        attachmentStack.addArrangedSubview(chip)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 210),
        ])
        layoutSubtreeIfNeeded()
    }

    private func setTranscriptState(_ text: String, active: Bool) {
        transcriptStateLabel.stringValue = text
        transcriptStateLabel.textColor = active ? BlueyTheme.green : BlueyTheme.textDim
        transcriptActivityDot.layer?.backgroundColor = (active ? BlueyTheme.green : BlueyTheme.textDim.withAlphaComponent(0.55)).cgColor
        transcriptActivityDot.layer?.shadowOpacity = active ? 0.45 : 0
        transcriptStrip.layer?.borderColor = (active ? BlueyTheme.green.withAlphaComponent(0.26) : BlueyTheme.hairline).cgColor
    }

    private func updateRouteBadge(
        for question: String,
        selectedRoute: (provider: String?, model: String?, mode: String?)
    ) {
        let manual = (selectedRoute.provider ?? "auto").lowercased() != "auto"
        if manual {
            switch selectedRoute.model ?? selectedRoute.provider ?? "Manual" {
            case let value where value.contains("mini"):
                routeBadge.stringValue = "Instant · manual"
            case let value where value.contains("sonnet"):
                routeBadge.stringValue = "Deep · manual"
            default:
                routeBadge.stringValue = "Manual lane"
            }
            routeBadge.textColor = BlueyTheme.text
            return
        }

        let lower = question.lowercased()
        let words = lower.split { $0.isWhitespace || $0.isNewline }.count
        let vision = lower.contains("screen") || lower.contains("screenshot") || lower.contains("image")
        let code = looksLikeCode(lower) || lower.contains("leetcode") || lower.contains("debug")
        let design = looksLikeSystemDesign(lower) || lower.contains("architecture")
        let hard = words > 80 || design || lower.contains("tradeoff") || lower.contains("scale")
        let label: String
        if vision {
            label = "Vision · deep"
        } else if hard {
            label = "Auto · hard"
        } else if code {
            label = "Auto · medium"
        } else {
            label = "Auto · easy"
        }
        routeBadge.stringValue = label
        routeBadge.textColor = hard || vision ? BlueyTheme.warning : BlueyTheme.cyan
    }

    private func routeBadgeText(for artifact: OverlayArtifact) -> String {
        switch artifact.artifactType {
        case "code": return "Code · canvas"
        case "system_design": return "Design · canvas"
        case "screen": return "Vision · canvas"
        case "document": return "Docs · canvas"
        default: return "Auto · canvas"
        }
    }

    func setContextItems(_ items: [OverlayContextItem]) {
        for view in attachmentStack.arrangedSubviews {
            attachmentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        attachmentStrip.isHidden = false
        guard !items.isEmpty else {
            setKnowledgeBadge("KB empty", accent: BlueyTheme.textDim)
            attachmentStrip.isHidden = true
            attachmentStripHeightConstraint?.constant = 0
            layoutSubtreeIfNeeded()
            return
        }

        setKnowledgeBadge("KB \(items.count) loaded", accent: BlueyTheme.green)
        attachmentStripHeightConstraint?.constant = 34

        for item in items {
            attachmentStack.addArrangedSubview(makeAttachmentChip(item))
        }
        layoutSubtreeIfNeeded()
    }

    func setSessions(_ sessions: [OverlaySessionItem]) {
        sessionItems = sessions
        renameField = nil
        for view in sessionStack.arrangedSubviews {
            sessionStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        if sessions.isEmpty {
            let empty = NSTextField(wrappingLabelWithString: "No saved recordings yet.")
            empty.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
            empty.textColor = BlueyTheme.textDim
            empty.alignment = .center
            empty.translatesAutoresizingMaskIntoConstraints = false
            sessionStack.addArrangedSubview(empty)
            empty.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -20).isActive = true
            return
        }

        for session in sessions {
            let row = makeSessionRow(session)
            sessionStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: sessionStack.widthAnchor, constant: -2).isActive = true
        }
    }

    // MARK: - Agent-bridge surfaces (Slice 5b)

    private func styleAgentSurfaces() {
        agentDrawer.wantsLayer = true
        agentDrawer.layer?.backgroundColor = NSColor(red: 0.035, green: 0.040, blue: 0.050, alpha: 0.98).cgColor
        agentDrawer.layer?.cornerRadius = 16
        agentDrawer.layer?.borderWidth = 1
        agentDrawer.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.20).cgColor
        agentDrawer.layer?.shadowColor = NSColor.black.cgColor
        agentDrawer.layer?.shadowOpacity = 0.26
        agentDrawer.layer?.shadowRadius = 18
        agentDrawer.layer?.shadowOffset = NSSize(width: 0, height: -8)
        agentDrawer.layer?.zPosition = 11

        agentDrawerTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        agentDrawerTitleLabel.textColor = BlueyTheme.text
        agentDrawerTitleLabel.lineBreakMode = .byTruncatingTail
        agentDrawerTitleLabel.maximumNumberOfLines = 1

        agentDrawerCaption.font = NSFont.systemFont(ofSize: 9.5, weight: .medium)
        agentDrawerCaption.textColor = BlueyTheme.textDim
        agentDrawerCaption.lineBreakMode = .byWordWrapping
        agentDrawerCaption.maximumNumberOfLines = 2

        styleHeaderIconButton(agentDrawerBackButton, symbol: "chevron.left", fallback: "<")
        styleHeaderIconButton(agentDrawerCloseButton, symbol: "xmark", fallback: "x")
        agentDrawerBackButton.toolTip = "Back to agents"
        agentDrawerCloseButton.toolTip = "Close"

        agentStack.orientation = .vertical
        agentStack.alignment = .centerX
        agentStack.spacing = 6
        agentStack.edgeInsets = NSEdgeInsets(top: 2, left: 0, bottom: 2, right: 0)

        agentScroll.drawsBackground = false
        agentScroll.hasVerticalScroller = true
        agentScroll.hasHorizontalScroller = false
        agentScroll.autohidesScrollers = true
        agentScroll.borderType = .noBorder
        agentScroll.documentView = agentStack
        agentScroll.scrollerStyle = .overlay

        agentBadge.font = NSFont.systemFont(ofSize: 11.3, weight: .bold)
        agentBadge.textColor = BlueyTheme.cyan
        agentBadge.alignment = .center
        agentBadge.lineBreakMode = .byTruncatingTail
        agentBadge.maximumNumberOfLines = 1
        agentBadge.setContentHuggingPriority(.defaultLow, for: .horizontal)
        agentBadge.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        agentBadge.wantsLayer = true
        agentBadge.layer?.backgroundColor = BlueyTheme.cyan.withAlphaComponent(0.10).cgColor
        agentBadge.layer?.cornerRadius = 13
        agentBadge.layer?.borderWidth = 1
        agentBadge.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.40).cgColor
        agentBadge.toolTip = "Attached coding agent — tap to switch or detach"

        connectorSheetOverlay.wantsLayer = true
        connectorSheetOverlay.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.52).cgColor
        connectorSheetOverlay.layer?.zPosition = 95

        connectorSheetPanel.wantsLayer = true
        connectorSheetPanel.layer?.backgroundColor = BlueyTheme.panelDeep.cgColor
        connectorSheetPanel.layer?.cornerRadius = 18
        connectorSheetPanel.layer?.borderWidth = 1
        connectorSheetPanel.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.30).cgColor
        connectorSheetPanel.layer?.shadowColor = NSColor.black.cgColor
        connectorSheetPanel.layer?.shadowOpacity = 0.34
        connectorSheetPanel.layer?.shadowRadius = 22
        connectorSheetPanel.layer?.shadowOffset = .zero

        connectorSheetTitle.font = NSFont.systemFont(ofSize: 15, weight: .bold)
        connectorSheetTitle.textColor = BlueyTheme.text
        connectorSheetTitle.alignment = .center
        connectorSheetSummary.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        connectorSheetSummary.textColor = BlueyTheme.textDim
        connectorSheetSummary.alignment = .center
        connectorSheetSummary.lineBreakMode = .byTruncatingTail

        connectorSheetStack.orientation = .vertical
        connectorSheetStack.alignment = .centerX
        connectorSheetStack.spacing = 6
        connectorSheetScroll.drawsBackground = false
        connectorSheetScroll.hasVerticalScroller = true
        connectorSheetScroll.hasHorizontalScroller = false
        connectorSheetScroll.autohidesScrollers = true
        connectorSheetScroll.borderType = .noBorder
        connectorSheetScroll.documentView = connectorSheetStack
        connectorSheetScroll.scrollerStyle = .overlay

        styleControlButton(connectorSheetCancelButton, symbol: "xmark", accent: false)
        styleControlButton(connectorSheetAttachButton, symbol: "link", accent: true)
        connectorSheetCancelButton.toolTip = "Cancel"
        connectorSheetAttachButton.toolTip = "Attach this agent"
    }

    // Open the agent picker drawer and request a fresh agent list. Closes the
    // session drawer so only one left-rail surface shows at a time.
    private func openAgentDrawer() {
        sessionDrawer.isHidden = true
        agentDrawerStage = .picker
        agentDrawer.isHidden = false
        if !agentListLoaded {
            renderAgentLoading()
        } else {
            renderAgentPicker()
        }
        emitAgentListRequested()
    }

    @objc private func agentClicked() {
        if agentDrawer.isHidden {
            openAgentDrawer()
        } else {
            agentDrawer.isHidden = true
        }
    }

    @objc private func agentBadgeClicked() {
        openAgentDrawer()
    }

    @objc private func agentDrawerCloseClicked() {
        agentDrawer.isHidden = true
    }

    @objc private func agentDrawerBackClicked() {
        agentDrawerStage = .picker
        if agentListLoaded {
            renderAgentPicker()
        } else {
            renderAgentLoading()
            emitAgentListRequested()
        }
    }

    func setAgents(_ agents: [AgentSummary]) {
        agentSummaries = agents
        agentListLoaded = true
        attachedAgentKind = agents.first(where: { $0.attached })?.kind
        updateAgentBadge()
        // Only repaint the picker if the drawer is showing the picker stage.
        if !agentDrawer.isHidden, case .picker = agentDrawerStage {
            renderAgentPicker()
        }
    }

    private func clearAgentStack() {
        for view in agentStack.arrangedSubviews {
            agentStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
    }

    private func addAgentStackRow(_ row: NSView) {
        agentStack.addArrangedSubview(row)
        row.widthAnchor.constraint(equalTo: agentStack.widthAnchor, constant: -2).isActive = true
    }

    private func renderAgentLoading() {
        agentDrawerTitleLabel.stringValue = "Coding agents"
        agentDrawerBackButton.isHidden = true
        agentDrawerCaption.stringValue = "Answers run on your machine — your agent replies."
        clearAgentStack()
        addAgentStackRow(makeAgentMessageRow("Finding coding agents…", dim: true))
    }

    private func renderAgentPicker() {
        agentDrawerTitleLabel.stringValue = "Coding agents"
        agentDrawerBackButton.isHidden = true
        agentDrawerCaption.stringValue = "Answers run on your machine — your agent replies."
        clearAgentStack()
        guard !agentSummaries.isEmpty else {
            addAgentStackRow(makeAgentMessageRow("No coding agents found", dim: true))
            return
        }
        for agent in agentSummaries {
            addAgentStackRow(makeAgentRow(agent))
        }
    }

    // One picker row: name + capability chip + "{n} tools · {n} sessions".
    // Attached rows render cyan-active with a ✕ detach affordance.
    private func makeAgentRow(_ agent: AgentSummary) -> NSView {
        let cap = AgentCapability(agent.capability)
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = agent.attached
            ? BlueyTheme.cyanSoft.cgColor
            : NSColor.white.withAlphaComponent(cap.dimmed ? 0.02 : 0.035).cgColor
        row.layer?.cornerRadius = 12
        row.layer?.borderWidth = 1
        row.layer?.borderColor = agent.attached
            ? BlueyTheme.cyan.withAlphaComponent(0.40).cgColor
            : BlueyTheme.hairline.cgColor

        let title = NSTextField(labelWithString: agent.displayName)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        title.textColor = cap.dimmed ? BlueyTheme.textDim : BlueyTheme.text
        title.lineBreakMode = .byTruncatingTail

        let chip = makeCapabilityChip(cap)

        let sessions = agent.sessionCount ?? 0
        let subtitleText = "\(agent.connectorCount) tools · \(sessions) sessions"
        let subtitle = NSTextField(labelWithString: subtitleText)
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 9.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.lineBreakMode = .byTruncatingTail

        row.addSubview(title)
        row.addSubview(chip)
        row.addSubview(subtitle)

        // A transparent click button fills the row for tappable agents.
        let tappable = !cap.dimmed
        var trailingRef = row.trailingAnchor
        var trailingConst: CGFloat = -10
        if agent.attached {
            let detach = NSButton(title: "", target: self, action: #selector(agentDetachClicked))
            detach.translatesAutoresizingMaskIntoConstraints = false
            detach.isBordered = false
            detach.contentTintColor = BlueyTheme.cyan
            detach.toolTip = "Detach \(agent.displayName)"
            if let image = symbolImage("xmark.circle.fill") {
                image.isTemplate = true
                detach.image = image
                detach.imagePosition = .imageOnly
                detach.imageScaling = .scaleProportionallyDown
            } else {
                detach.title = "✕"
                detach.font = NSFont.systemFont(ofSize: 11, weight: .bold)
            }
            row.addSubview(detach)
            NSLayoutConstraint.activate([
                detach.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8),
                detach.centerYAnchor.constraint(equalTo: row.centerYAnchor),
                detach.widthAnchor.constraint(equalToConstant: 26),
                detach.heightAnchor.constraint(equalToConstant: 26),
            ])
            trailingRef = detach.leadingAnchor
            trailingConst = -6
        } else if tappable {
            let openButton = NSButton(title: "", target: self, action: #selector(agentRowClicked(_:)))
            openButton.translatesAutoresizingMaskIntoConstraints = false
            openButton.isBordered = false
            openButton.tag = agentIndex(agent.kind)
            row.addSubview(openButton)
            NSLayoutConstraint.activate([
                openButton.topAnchor.constraint(equalTo: row.topAnchor),
                openButton.leadingAnchor.constraint(equalTo: row.leadingAnchor),
                openButton.trailingAnchor.constraint(equalTo: row.trailingAnchor),
                openButton.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            ])
        }

        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 52),

            title.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            title.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            title.trailingAnchor.constraint(lessThanOrEqualTo: chip.leadingAnchor, constant: -6),

            chip.centerYAnchor.constraint(equalTo: title.centerYAnchor),
            chip.trailingAnchor.constraint(equalTo: trailingRef, constant: trailingConst),

            subtitle.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 2),
            subtitle.trailingAnchor.constraint(equalTo: trailingRef, constant: trailingConst),
        ])
        return row
    }

    private func makeCapabilityChip(_ cap: AgentCapability) -> NSView {
        let chip = NSTextField(labelWithString: cap.label)
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.font = NSFont.monospacedSystemFont(ofSize: 8.5, weight: .bold)
        chip.textColor = cap.color
        chip.alignment = .center
        chip.wantsLayer = true
        chip.layer?.backgroundColor = cap.color.withAlphaComponent(0.14).cgColor
        chip.layer?.cornerRadius = 8
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = cap.color.withAlphaComponent(0.36).cgColor
        chip.setContentCompressionResistancePriority(.required, for: .horizontal)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 17),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 52),
        ])
        return chip
    }

    private func makeAgentMessageRow(_ text: String, dim: Bool) -> NSView {
        let label = NSTextField(wrappingLabelWithString: text)
        label.font = NSFont.systemFont(ofSize: 11.5, weight: .medium)
        label.textColor = dim ? BlueyTheme.textDim : BlueyTheme.text
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(label)
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: row.topAnchor, constant: 14),
            label.bottomAnchor.constraint(equalTo: row.bottomAnchor, constant: -14),
            label.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 8),
            label.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8),
        ])
        return row
    }

    private func agentIndex(_ kind: String) -> Int {
        agentSummaries.firstIndex(where: { $0.kind == kind }) ?? -1
    }

    @objc private func agentRowClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < agentSummaries.count else { return }
        let agent = agentSummaries[sender.tag]
        // Drivable / read-only agents move to the session picker.
        agentDrawerStage = .sessions(kind: agent.kind, displayName: agent.displayName)
        agentSessions = []
        agentSessionsLoaded = false
        renderAgentSessions()
        emitAgentSessionsRequested(kind: agent.kind)
    }

    @objc private func agentDetachClicked() {
        attachedAgentKind = nil
        emitAgentDetachRequested()
        // Optimistically reflect detach; the daemon re-emits set_agents to confirm.
        agentSummaries = agentSummaries.map { agent in
            AgentSummary(
                kind: agent.kind, displayName: agent.displayName, capability: agent.capability,
                connectorCount: agent.connectorCount, readyConnectorCount: agent.readyConnectorCount,
                sessionCount: agent.sessionCount, attached: false)
        }
        updateAgentBadge()
        if case .picker = agentDrawerStage { renderAgentPicker() }
    }

    // MARK: Session picker (agent drawer push-nav)

    func setAgentSessions(kind: String, sessions: [AgentSessionSummary]) {
        // Ignore late deliveries for an agent we've navigated away from.
        guard case let .sessions(currentKind, _) = agentDrawerStage, currentKind == kind else {
            return
        }
        agentSessions = sessions
        agentSessionsLoaded = true
        renderAgentSessions()
    }

    private func renderAgentSessions() {
        guard case let .sessions(kind, displayName) = agentDrawerStage else { return }
        agentDrawerTitleLabel.stringValue = displayName
        agentDrawerBackButton.isHidden = false
        agentDrawerCaption.stringValue = "Connectors run from your agent. Bluey stores nothing."
        clearAgentStack()

        // Pinned quick-actions: continue most recent + fresh.
        let newest = agentSessions.first?.id
        addAgentStackRow(makeQuickAttachRow(
            title: "Continue most recent",
            subtitle: newest == nil ? "No past sessions yet" : "Resume your latest agent context",
            accent: true,
            kind: kind,
            sessionId: newest,
            enabled: newest != nil))
        addAgentStackRow(makeQuickAttachRow(
            title: "Fresh — no past context",
            subtitle: "Start the agent clean",
            accent: false,
            kind: kind,
            sessionId: nil,
            enabled: true))

        if !agentSessionsLoaded {
            addAgentStackRow(makeAgentMessageRow("Loading sessions…", dim: true))
            return
        }
        guard !agentSessions.isEmpty else {
            addAgentStackRow(makeAgentMessageRow(
                "Session history off — enable in settings", dim: true))
            return
        }
        for session in agentSessions {
            addAgentStackRow(makeAgentSessionRow(kind: kind, session: session))
        }
    }

    private func makeQuickAttachRow(
        title: String, subtitle: String, accent: Bool,
        kind: String, sessionId: String?, enabled: Bool
    ) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = accent
            ? BlueyTheme.cyanSoft.cgColor
            : NSColor.white.withAlphaComponent(0.035).cgColor
        row.layer?.cornerRadius = 12
        row.layer?.borderWidth = 1
        row.layer?.borderColor = accent
            ? BlueyTheme.cyan.withAlphaComponent(0.40).cgColor
            : BlueyTheme.hairline.cgColor
        row.alphaValue = enabled ? 1.0 : 0.5

        let titleLabel = NSTextField(labelWithString: title)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .bold)
        titleLabel.textColor = accent ? BlueyTheme.cyan : BlueyTheme.text
        titleLabel.lineBreakMode = .byTruncatingTail

        let subtitleLabel = NSTextField(labelWithString: subtitle)
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.font = NSFont.systemFont(ofSize: 9.5, weight: .medium)
        subtitleLabel.textColor = BlueyTheme.textDim
        subtitleLabel.lineBreakMode = .byTruncatingTail

        row.addSubview(titleLabel)
        row.addSubview(subtitleLabel)
        if enabled {
            let button = NSButton(title: "", target: self, action: #selector(quickAttachClicked(_:)))
            button.translatesAutoresizingMaskIntoConstraints = false
            button.isBordered = false
            button.identifier = NSUserInterfaceItemIdentifier(attachToken(kind: kind, sessionId: sessionId))
            row.addSubview(button)
            NSLayoutConstraint.activate([
                button.topAnchor.constraint(equalTo: row.topAnchor),
                button.leadingAnchor.constraint(equalTo: row.leadingAnchor),
                button.trailingAnchor.constraint(equalTo: row.trailingAnchor),
                button.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            ])
        }
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 48),
            titleLabel.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            titleLabel.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            titleLabel.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            subtitleLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 2),
            subtitleLabel.trailingAnchor.constraint(equalTo: titleLabel.trailingAnchor),
        ])
        return row
    }

    private func makeAgentSessionRow(kind: String, session: AgentSessionSummary) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        row.layer?.cornerRadius = 12
        row.layer?.borderWidth = 1
        row.layer?.borderColor = BlueyTheme.hairline.cgColor

        let titleLabel = NSTextField(labelWithString: session.title ?? "Untitled session")
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        titleLabel.textColor = BlueyTheme.text
        titleLabel.lineBreakMode = .byTruncatingTail

        let subtitleLabel = NSTextField(labelWithString: session.updatedAt)
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.font = NSFont.monospacedSystemFont(ofSize: 9, weight: .medium)
        subtitleLabel.textColor = BlueyTheme.textDim
        subtitleLabel.lineBreakMode = .byTruncatingTail

        let button = NSButton(title: "", target: self, action: #selector(quickAttachClicked(_:)))
        button.translatesAutoresizingMaskIntoConstraints = false
        button.isBordered = false
        button.identifier = NSUserInterfaceItemIdentifier(attachToken(kind: kind, sessionId: session.id))

        row.addSubview(button)
        row.addSubview(titleLabel)
        row.addSubview(subtitleLabel)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 48),
            button.topAnchor.constraint(equalTo: row.topAnchor),
            button.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            button.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            button.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            titleLabel.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            titleLabel.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            titleLabel.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            subtitleLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 2),
            subtitleLabel.trailingAnchor.constraint(equalTo: titleLabel.trailingAnchor),
        ])
        return row
    }

    // "kind|sessionId" identifier round-trips an attach target through the
    // button without a side table; empty session segment means attach fresh.
    private func attachToken(kind: String, sessionId: String?) -> String {
        "\(kind)|\(sessionId ?? "")"
    }

    @objc private func quickAttachClicked(_ sender: NSButton) {
        guard let raw = sender.identifier?.rawValue else { return }
        let parts = raw.split(separator: "|", maxSplits: 1, omittingEmptySubsequences: false)
        guard let kind = parts.first.map(String.init), !kind.isEmpty else { return }
        let sessionId = parts.count > 1 && !parts[1].isEmpty ? String(parts[1]) : nil
        beginAttachFlow(kind: kind, sessionId: sessionId)
    }

    // MARK: Connector inheritance sheet

    // Stage the attach target, then surface the connector sheet. The daemon's
    // connectors arrive async; attach is never blocked on re-auth gaps.
    private func beginAttachFlow(kind: String, sessionId: String?) {
        pendingConnectorKind = kind
        pendingConnectorSessionId = sessionId
        pendingConnectorInfos = []
        pendingConnectorsLoaded = false
        renderConnectorSheet()
        connectorSheetOverlay.isHidden = false
        connectorSheetOverlay.alphaValue = 0
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            self.connectorSheetOverlay.animator().alphaValue = 1
        }
        emitAgentConnectorsRequested(kind: kind)
    }

    func setAgentConnectors(kind: String, connectors: [AgentConnectorInfo]) {
        guard pendingConnectorKind == kind, !connectorSheetOverlay.isHidden else { return }
        pendingConnectorInfos = connectors
        pendingConnectorsLoaded = true
        renderConnectorSheet()
    }

    private func renderConnectorSheet() {
        let displayName = agentSummaries.first(where: { $0.kind == pendingConnectorKind })?.displayName
            ?? "agent"
        connectorSheetTitle.stringValue = "\(displayName) connectors"
        for view in connectorSheetStack.arrangedSubviews {
            connectorSheetStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        guard pendingConnectorsLoaded else {
            connectorSheetSummary.stringValue = "Reading inherited connectors…"
            let row = makeAgentMessageRow("Loading…", dim: true)
            connectorSheetStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: connectorSheetStack.widthAnchor, constant: -2).isActive = true
            return
        }

        let total = pendingConnectorInfos.count
        let ready = pendingConnectorInfos.filter { $0.ready }.count
        let needReauth = total - ready
        if total == 0 {
            connectorSheetSummary.stringValue = "No inherited connectors — attach runs clean."
        } else if needReauth > 0 {
            connectorSheetSummary.stringValue =
                "\(ready) of \(total) connectors ready · \(needReauth) need re-auth"
        } else {
            connectorSheetSummary.stringValue = "\(ready) of \(total) connectors ready"
        }

        for connector in pendingConnectorInfos {
            let row = makeConnectorRow(connector)
            connectorSheetStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: connectorSheetStack.widthAnchor, constant: -2).isActive = true
        }
    }

    private func makeConnectorRow(_ connector: AgentConnectorInfo) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        row.layer?.cornerRadius = 10
        row.layer?.borderWidth = 1
        row.layer?.borderColor = BlueyTheme.hairline.cgColor

        let name = NSTextField(labelWithString: connector.name)
        name.translatesAutoresizingMaskIntoConstraints = false
        name.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        name.textColor = BlueyTheme.text
        name.lineBreakMode = .byTruncatingTail

        let tierColor: NSColor = connector.ready ? BlueyTheme.green : BlueyTheme.warning
        let tierText = connectorTierText(connector)
        let tier = NSTextField(labelWithString: tierText)
        tier.translatesAutoresizingMaskIntoConstraints = false
        tier.font = NSFont.monospacedSystemFont(ofSize: 9, weight: .bold)
        tier.textColor = tierColor
        tier.alignment = .right
        tier.lineBreakMode = .byTruncatingTail
        tier.setContentCompressionResistancePriority(.required, for: .horizontal)

        row.addSubview(name)
        row.addSubview(tier)

        if !connector.ready {
            let reauth = NSButton(title: "", target: self, action: #selector(connectorReauthClicked(_:)))
            reauth.translatesAutoresizingMaskIntoConstraints = false
            reauth.isBordered = false
            reauth.contentTintColor = BlueyTheme.warning
            reauth.identifier = NSUserInterfaceItemIdentifier(connector.name)
            reauth.toolTip = "Re-authenticate \(connector.name)"
            if let image = symbolImage("arrow.clockwise") {
                image.isTemplate = true
                reauth.image = image
                reauth.imagePosition = .imageOnly
                reauth.imageScaling = .scaleProportionallyDown
            } else {
                reauth.title = "↻"
                reauth.font = NSFont.systemFont(ofSize: 11, weight: .bold)
            }
            row.addSubview(reauth)
            NSLayoutConstraint.activate([
                reauth.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -8),
                reauth.centerYAnchor.constraint(equalTo: row.centerYAnchor),
                reauth.widthAnchor.constraint(equalToConstant: 24),
                reauth.heightAnchor.constraint(equalToConstant: 24),
                tier.trailingAnchor.constraint(equalTo: reauth.leadingAnchor, constant: -6),
            ])
        } else {
            tier.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10).isActive = true
        }

        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 38),
            name.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            name.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            name.trailingAnchor.constraint(lessThanOrEqualTo: tier.leadingAnchor, constant: -8),
            tier.centerYAnchor.constraint(equalTo: row.centerYAnchor),
        ])
        return row
    }

    private func connectorTierText(_ connector: AgentConnectorInfo) -> String {
        switch connector.authTier {
        case "env_auth": return connector.ready ? "env · ready" : "env · re-auth"
        case "hosted_oauth": return connector.ready ? "oauth · ready" : "oauth · re-auth"
        default: return connector.ready ? "ready" : "re-auth"
        }
    }

    @objc private func connectorReauthClicked(_ sender: NSButton) {
        guard let name = sender.identifier?.rawValue, let kind = pendingConnectorKind else { return }
        emitConnectorReauthRequested(kind: kind, name: name)
        sender.toolTip = "Re-auth requested for \(name)"
    }

    @objc private func connectorSheetCancelClicked() {
        dismissConnectorSheet()
    }

    @objc private func connectorSheetAttachClicked() {
        guard let kind = pendingConnectorKind else { return }
        let sessionId = pendingConnectorSessionId
        emitAgentAttachRequested(kind: kind, sessionId: sessionId)
        attachedAgentKind = kind
        dismissConnectorSheet()
        agentDrawer.isHidden = true
        // Optimistic badge; the daemon confirms with set_agents.
        updateAgentBadge()
    }

    private func dismissConnectorSheet() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.10
            self.connectorSheetOverlay.animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            guard let self else { return }
            self.connectorSheetOverlay.isHidden = true
            self.connectorSheetOverlay.alphaValue = 1
        })
    }

    // MARK: Attached-state badge

    private func updateAgentBadge() {
        guard let kind = attachedAgentKind,
              let agent = agentSummaries.first(where: { $0.kind == kind })
        else {
            agentBadge.isHidden = true
            onAgentAttachmentChanged?(false)
            return
        }
        agentBadge.isHidden = false
        agentBadge.stringValue = "\(agentShortLabel(kind)) · \(agent.readyConnectorCount)/\(agent.connectorCount) tools"
        statusLabel.stringValue = "agent: \(agent.displayName)"
        onAgentAttachmentChanged?(true)
    }

    func resetSessionSurface() {
        feed.clear()
        setContextItems([])
        transcriptSnippets.removeAll()
        updateTranscriptStripText("Live captions preview", scrollToEnd: false)
        setTranscriptState("IDLE", active: false)
        routeBadge.stringValue = "Auto · ready"
        latestCanvas = nil
        setCanvasOpen(false)
        canvasToggleButton.isHidden = true
    }

    func pushCard(_ card: RenderedCard) {
        feed.push(card)
        routeCanvasIfNeeded(card)
    }

    /// Render a review-gated Fix proposal as a dedicated card in the feed
    /// (Slice F4). Carries the proposal payload + a pending decision state; the
    /// feed's Approve/Reject buttons echo back via emitFixApprovalResponded.
    func pushFixProposal(_ proposal: FixProposal) {
        let card = RenderedCard(
            id: proposal.proposalId,
            kind: "fix_proposal",
            title: "Proposed fix",
            body: "",
            done: true,
            costLabel: nil,
            artifact: nil,
            source: nil,
            fixProposal: proposal,
            fixState: .pending)
        feed.push(card)
    }

    func updateCard(id: String, body: String, done: Bool, costLabel: String?, artifact: OverlayArtifact?) {
        guard let card = feed.update(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact) else {
            return
        }
        if let artifact {
            routeBadge.stringValue = routeBadgeText(for: artifact)
        } else if !done {
            statusLabel.stringValue = "Answer streaming"
        }
        routeCanvasIfNeeded(card)
    }

    private func routeCanvasIfNeeded(_ card: RenderedCard) {
        guard let artifact = makeCanvasArtifact(from: card) else { return }
        latestCanvas = artifact
        canvasPane.render(artifact)
        canvasToggleButton.isHidden = false
        if shouldAutoOpenCanvas(for: card, artifact: artifact) {
            setCanvasOpen(true)
        }
    }

    private func shouldAutoOpenCanvas(for card: RenderedCard, artifact: CanvasArtifact) -> Bool {
        if card.artifact != nil { return true }
        guard card.kind == "answer" else { return false }
        switch artifact.kind {
        case .code, .systemDesign, .screen:
            return true
        case .document, .structured:
            return false
        }
    }

    private func setCanvasOpen(_ open: Bool) {
        canvasOpen = open
        canvasPane.isHidden = !open
        canvasWidthConstraint?.constant = open ? 310 : 0
        canvasToggleButton.contentTintColor = open ? BlueyTheme.cyan : BlueyTheme.textDim
        if open {
            ensureRoomForCanvas()
        } else {
            restoreCompactWidth()
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            self.layoutSubtreeIfNeeded()
        }
    }

    private func ensureRoomForCanvas() {
        guard let window else { return }
        let targetWidth = ExpandedPanelMetrics.maxCanvasWidth
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let clampedTargetWidth = ExpandedPanelMetrics.fittingWidth(for: screen, preferred: targetWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: clampedTargetWidth)
        let maximumWidth = max(clampedTargetWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maximumHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = maximumWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maximumHeight
        }
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maximumWidth, height: maximumHeight)
        window.contentMaxSize = NSSize(width: maximumWidth, height: maximumHeight)
        guard window.frame.width < clampedTargetWidth else { return }
        var frame = window.frame
        frame.size.width = clampedTargetWidth
        frame.origin.x = min(max(screen.minX + 12, frame.origin.x), screen.maxX - frame.width - 12)
        frame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(frame, visibleFrame: screen)
        window.setFrame(frame, display: true, animate: true)
    }

    private func restoreCompactWidth() {
        guard let window else { return }
        let screen = window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let compactWidth = ExpandedPanelMetrics.fittingWidth(
            for: screen,
            preferred: ExpandedPanelMetrics.maxCompactWidth)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: compactWidth)
        let maximumWidth = max(compactWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maximumHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        if let overlayWindow = window as? OverlayWindow {
            overlayWindow.minimumFrameWidth = minimumWidth
            overlayWindow.maximumFrameWidth = maximumWidth
            overlayWindow.minimumFrameHeight = ExpandedPanelMetrics.minHeight
            overlayWindow.maximumFrameHeight = maximumHeight
        }
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maximumWidth, height: maximumHeight)
        window.contentMaxSize = NSSize(width: maximumWidth, height: maximumHeight)
    }

    private func makeCanvasArtifact(from card: RenderedCard) -> CanvasArtifact? {
        guard card.kind == "answer" || card.kind == "context" || card.kind == "system" else {
            return nil
        }
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return nil }

        if let artifact = card.artifact {
            let kind = CanvasKind.fromArtifactType(artifact.artifactType)
            let confidence = artifact.confidence.map { "Confidence \(Int(($0 * 100).rounded()))%" }
            return CanvasArtifact(
                kind: kind,
                title: artifact.title.isEmpty ? kind.title : artifact.title,
                subtitle: confidence ?? kind.subtitle,
                content: artifact.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    ? body
                    : artifact.body,
                sourceCardId: card.id)
        }

        let lower = body.lowercased()
        let codeBlocks = extractCodeBlocks(from: body)
        if !codeBlocks.isEmpty || looksLikeCode(lower) {
            return CanvasArtifact(
                kind: .code,
                title: "Code canvas",
                subtitle: "Code, tests, complexity, and implementation notes",
                content: formatCodeCanvas(body: body, codeBlocks: codeBlocks),
                sourceCardId: card.id)
        }

        if looksLikeSystemDesign(lower) {
            return CanvasArtifact(
                kind: .systemDesign,
                title: "System design canvas",
                subtitle: "Architecture, tradeoffs, APIs, data, and scale",
                content: formatStructuredCanvas(body, fallbackHeading: "System Design"),
                sourceCardId: card.id)
        }

        if looksLikeScreenAnalysis(lower) {
            return CanvasArtifact(
                kind: .screen,
                title: "Screen analysis",
                subtitle: "Detected context and answerable details",
                content: formatStructuredCanvas(body, fallbackHeading: "Screen Context"),
                sourceCardId: card.id)
        }

        if card.kind == "context" || looksLikeDocumentWork(lower) {
            return CanvasArtifact(
                kind: .document,
                title: "Document notes",
                subtitle: "Attached context distilled for this session",
                content: formatStructuredCanvas(body, fallbackHeading: "Document Context"),
                sourceCardId: card.id)
        }

        if body.count > 950 && hasStructuredShape(body) {
            return CanvasArtifact(
                kind: .structured,
                title: "Workspace",
                subtitle: "Long-form answer kept beside the chat",
                content: formatStructuredCanvas(body, fallbackHeading: "Details"),
                sourceCardId: card.id)
        }

        return nil
    }

    private func appendTranscriptSnippet(_ card: RenderedCard) {
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = card.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty else { return }

        let label = title.isEmpty ? "Transcript" : title
        transcriptSnippets.append("\(label): \(body)")
        if transcriptSnippets.count > 6 {
            transcriptSnippets.removeFirst(transcriptSnippets.count - 6)
        }
        setTranscriptState(recordingActive ? "TRANSCRIBING" : "CAPTURED", active: recordingActive)
        updateTranscriptStripText(transcriptSnippets.joined(separator: "   "), scrollToEnd: true)
    }

    private func updateTranscriptStripText(_ text: String, scrollToEnd: Bool) {
        transcriptLabel.stringValue = text
        resizeTranscriptLabelToContent()
        guard scrollToEnd else {
            transcriptScroll.contentView.scroll(to: .zero)
            transcriptScroll.reflectScrolledClipView(transcriptScroll.contentView)
            return
        }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.resizeTranscriptLabelToContent()
            let maxX = max(0, self.transcriptLabel.frame.width - self.transcriptScroll.contentView.bounds.width)
            self.transcriptScroll.contentView.scroll(to: NSPoint(x: maxX, y: 0))
            self.transcriptScroll.reflectScrolledClipView(self.transcriptScroll.contentView)
        }
    }

    private func resizeTranscriptLabelToContent() {
        let viewport = max(0, transcriptScroll.contentView.bounds.width)
        let height = max(22, transcriptScroll.contentView.bounds.height)
        let font = transcriptLabel.font ?? NSFont.systemFont(ofSize: 11.5, weight: .medium)
        let textWidth = ceil((transcriptLabel.stringValue as NSString).size(
            withAttributes: [.font: font]).width) + 24
        transcriptLabel.frame = NSRect(
            x: 0,
            y: max(0, (height - 18) / 2),
            width: max(viewport, textWidth),
            height: 18)
    }

    private func makeAttachmentChip(_ item: OverlayContextItem) -> NSView {
        let chip = NSView()
        chip.translatesAutoresizingMaskIntoConstraints = false
        chip.wantsLayer = true
        chip.layer?.backgroundColor = BlueyTheme.surfaceRaised.cgColor
        chip.layer?.cornerRadius = 12
        chip.layer?.borderWidth = 1
        chip.layer?.borderColor = BlueyTheme.cyan.withAlphaComponent(0.18).cgColor

        let icon = NSImageView()
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.imageScaling = .scaleProportionallyDown
        icon.contentTintColor = fileAccent(for: item.kind)
        if let image = symbolImage(fileSymbol(for: item.kind)) {
            image.isTemplate = true
            icon.image = image
        }

        let title = NSTextField(labelWithString: item.title.isEmpty ? "Attached file" : item.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        title.textColor = BlueyTheme.text
        title.lineBreakMode = .byTruncatingMiddle
        title.maximumNumberOfLines = 1

        let kind = NSTextField(labelWithString: item.kind.uppercased())
        kind.translatesAutoresizingMaskIntoConstraints = false
        kind.font = NSFont.monospacedSystemFont(ofSize: 8.5, weight: .bold)
        kind.textColor = BlueyTheme.textDim
        kind.stringValue = "LOADED · \(item.kind.uppercased())"

        chip.addSubview(icon)
        chip.addSubview(title)
        chip.addSubview(kind)
        NSLayoutConstraint.activate([
            chip.heightAnchor.constraint(equalToConstant: 28),
            chip.widthAnchor.constraint(lessThanOrEqualToConstant: 190),
            chip.widthAnchor.constraint(greaterThanOrEqualToConstant: 112),

            icon.leadingAnchor.constraint(equalTo: chip.leadingAnchor, constant: 8),
            icon.centerYAnchor.constraint(equalTo: chip.centerYAnchor),
            icon.widthAnchor.constraint(equalToConstant: 16),
            icon.heightAnchor.constraint(equalToConstant: 16),

            title.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 7),
            title.topAnchor.constraint(equalTo: chip.topAnchor, constant: 4),
            title.trailingAnchor.constraint(equalTo: chip.trailingAnchor, constant: -8),

            kind.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            kind.topAnchor.constraint(equalTo: title.bottomAnchor, constant: -1),
            kind.trailingAnchor.constraint(lessThanOrEqualTo: title.trailingAnchor),
        ])
        return chip
    }

    private func makeSessionRow(_ session: OverlaySessionItem) -> NSView {
        let row = NSView()
        row.translatesAutoresizingMaskIntoConstraints = false
        row.wantsLayer = true
        row.layer?.backgroundColor = session.isActive
            ? BlueyTheme.cyanSoft.cgColor
            : NSColor.white.withAlphaComponent(0.035).cgColor
        row.layer?.cornerRadius = 12
        row.layer?.borderWidth = 1
        row.layer?.borderColor = session.isActive
            ? BlueyTheme.cyan.withAlphaComponent(0.34).cgColor
            : BlueyTheme.hairline.cgColor

        if editingSessionId == session.id {
            return configureRenameRow(row, session: session)
        }

        let openButton = NSButton(title: "", target: self, action: #selector(sessionRowClicked(_:)))
        openButton.translatesAutoresizingMaskIntoConstraints = false
        openButton.isBordered = false
        openButton.tag = sessionIndex(session.id)

        let title = NSTextField(labelWithString: session.title)
        title.translatesAutoresizingMaskIntoConstraints = false
        title.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        title.textColor = BlueyTheme.text
        title.lineBreakMode = .byTruncatingTail

        let subtitle = NSTextField(labelWithString: session.subtitle)
        subtitle.translatesAutoresizingMaskIntoConstraints = false
        subtitle.font = NSFont.systemFont(ofSize: 9.5, weight: .medium)
        subtitle.textColor = BlueyTheme.textDim
        subtitle.lineBreakMode = .byTruncatingTail

        let rename = NSButton(title: "", target: self, action: #selector(renameSessionClicked(_:)))
        rename.translatesAutoresizingMaskIntoConstraints = false
        rename.isBordered = false
        rename.tag = sessionIndex(session.id)
        rename.contentTintColor = BlueyTheme.cyan
        if let image = symbolImage("pencil") {
            image.isTemplate = true
            rename.image = image
            rename.imagePosition = .imageOnly
            rename.imageScaling = .scaleProportionallyDown
        } else {
            rename.title = "Edit"
            rename.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        row.addSubview(openButton)
        row.addSubview(title)
        row.addSubview(subtitle)
        row.addSubview(rename)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 52),

            openButton.topAnchor.constraint(equalTo: row.topAnchor),
            openButton.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            openButton.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            openButton.trailingAnchor.constraint(equalTo: rename.leadingAnchor),

            title.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            title.topAnchor.constraint(equalTo: row.topAnchor, constant: 8),
            title.trailingAnchor.constraint(equalTo: rename.leadingAnchor, constant: -6),

            subtitle.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 2),
            subtitle.trailingAnchor.constraint(equalTo: title.trailingAnchor),

            rename.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -6),
            rename.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            rename.widthAnchor.constraint(equalToConstant: 28),
            rename.heightAnchor.constraint(equalToConstant: 28),
        ])
        return row
    }

    private func configureRenameRow(_ row: NSView, session: OverlaySessionItem) -> NSView {
        let field = NSTextField()
        field.translatesAutoresizingMaskIntoConstraints = false
        field.stringValue = session.title
        field.font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        field.textColor = BlueyTheme.text
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.target = self
        field.action = #selector(saveInlineRenameClicked(_:))
        field.tag = sessionIndex(session.id)
        renameField = field

        let save = NSButton(title: "", target: self, action: #selector(saveInlineRenameClicked(_:)))
        save.translatesAutoresizingMaskIntoConstraints = false
        save.isBordered = false
        save.tag = sessionIndex(session.id)
        save.contentTintColor = BlueyTheme.cyan
        save.toolTip = "Save recording name"
        if let image = symbolImage("checkmark") {
            image.isTemplate = true
            save.image = image
            save.imagePosition = .imageOnly
            save.imageScaling = .scaleProportionallyDown
        } else {
            save.title = "Save"
            save.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        }

        row.addSubview(field)
        row.addSubview(save)
        NSLayoutConstraint.activate([
            row.heightAnchor.constraint(equalToConstant: 44),
            field.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            field.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            field.trailingAnchor.constraint(equalTo: save.leadingAnchor, constant: -6),
            field.heightAnchor.constraint(equalToConstant: 28),

            save.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -6),
            save.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            save.widthAnchor.constraint(equalToConstant: 28),
            save.heightAnchor.constraint(equalToConstant: 28),
        ])
        DispatchQueue.main.async { [weak self, weak field] in
            guard self?.editingSessionId == session.id else { return }
            self?.window?.makeFirstResponder(field)
            field?.selectText(nil)
        }
        return row
    }

    private func sessionIndex(_ id: String) -> Int {
        sessionItems.firstIndex(where: { $0.id == id }) ?? -1
    }

    @objc private func sessionRowClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        sessionDrawer.isHidden = true
        statusLabel.stringValue = session.title
        emitSessionOpen(id: session.id)
    }

    @objc private func renameSessionClicked(_ sender: NSButton) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        editingSessionId = session.id
        setSessions(sessionItems)
    }

    @objc private func saveInlineRenameClicked(_ sender: NSControl) {
        guard sender.tag >= 0, sender.tag < sessionItems.count else { return }
        let session = sessionItems[sender.tag]
        let title = (renameField?.stringValue ?? session.title)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        sessionItems[sender.tag] = OverlaySessionItem(
            id: session.id,
            title: title,
            subtitle: session.subtitle,
            isActive: session.isActive)
        editingSessionId = nil
        emitSessionRename(id: session.id, title: title)
        setSessions(sessionItems)
    }

    private func fileSymbol(for kind: String) -> String {
        switch kind {
        case "image", "diagram": return "photo"
        case "code": return "curlybraces"
        case "text": return "doc.plaintext"
        case "document": return "doc.text"
        default: return "doc"
        }
    }

    private func fileAccent(for kind: String) -> NSColor {
        switch kind {
        case "image", "diagram": return NSColor(red: 0.58, green: 0.74, blue: 1.0, alpha: 1.0)
        case "code": return NSColor(red: 0.58, green: 1.0, blue: 0.74, alpha: 1.0)
        case "text": return NSColor(red: 1.0, green: 0.82, blue: 0.42, alpha: 1.0)
        case "document": return NSColor(red: 1.0, green: 0.43, blue: 0.34, alpha: 1.0)
        default: return BlueyTheme.cyan
        }
    }

    private func extractCodeBlocks(from text: String) -> [String] {
        var blocks: [String] = []
        var current: [String] = []
        var inFence = false

        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                if inFence {
                    blocks.append(current.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines))
                    current.removeAll()
                }
                inFence.toggle()
                continue
            }
            if inFence {
                current.append(line)
            }
        }

        return blocks.filter { !$0.isEmpty }
    }

    private func looksLikeCode(_ lower: String) -> Bool {
        let signals = [
            "class solution",
            "def ",
            "function ",
            "const ",
            "let ",
            "public ",
            "private ",
            "time complexity",
            "space complexity",
            "test case",
            "edge case",
            "sql",
        ]
        return signals.filter { lower.contains($0) }.count >= 2
    }

    private func looksLikeSystemDesign(_ lower: String) -> Bool {
        let signals = [
            "system design",
            "architecture",
            "api",
            "database",
            "cache",
            "queue",
            "scale",
            "latency",
            "throughput",
            "tradeoff",
            "shard",
            "load balancer",
            "microservice",
            "event-driven",
        ]
        return signals.filter { lower.contains($0) }.count >= 3
    }

    private func looksLikeScreenAnalysis(_ lower: String) -> Bool {
        lower.contains("screenshot")
            || lower.contains("screen context")
            || lower.contains("analyse screen")
            || lower.contains("analyze screen")
            || lower.contains("image shows")
    }

    private func looksLikeDocumentWork(_ lower: String) -> Bool {
        lower.contains("attached document")
            || lower.contains("pdf")
            || lower.contains("resume")
            || lower.contains("document context")
            || lower.contains("source:")
    }

    private func hasStructuredShape(_ text: String) -> Bool {
        let lines = text.components(separatedBy: .newlines)
        let structured = lines.filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("- ")
                || trimmed.hasPrefix("* ")
                || trimmed.hasPrefix("#")
                || trimmed.range(of: #"^\d+[\.\)]\s"#, options: .regularExpression) != nil
        }
        return structured.count >= 3
    }

    private func formatCodeCanvas(body: String, codeBlocks: [String]) -> String {
        let notes = stripCodeFences(from: body)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        var sections: [String] = []

        if !codeBlocks.isEmpty {
            sections.append("CODE\n----\n" + codeBlocks.joined(separator: "\n\n// ---\n\n"))
        }

        if !notes.isEmpty {
            sections.append("NOTES\n-----\n" + notes)
        }

        return sections.isEmpty ? body : sections.joined(separator: "\n\n")
    }

    private func stripCodeFences(from text: String) -> String {
        var lines: [String] = []
        var inFence = false
        for line in text.components(separatedBy: .newlines) {
            if line.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                inFence.toggle()
                continue
            }
            if !inFence {
                lines.append(line)
            }
        }
        return lines.joined(separator: "\n")
    }

    private func formatStructuredCanvas(_ body: String, fallbackHeading: String) -> String {
        let clean = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return fallbackHeading }
        if clean.hasPrefix("#") || clean.uppercased().hasPrefix(fallbackHeading.uppercased()) {
            return clean
        }
        return "\(fallbackHeading)\n" + String(repeating: "-", count: fallbackHeading.count) + "\n" + clean
    }

    private func selectedRoute() -> (provider: String?, model: String?, mode: String?) {
        switch modelMenu.indexOfSelectedItem {
        case 1:
            return ("openai", "gpt-4o-mini", "general")
        case 2:
            return ("anthropic", "claude-3-5-sonnet-latest", "general")
        case 3:
            return ("anthropic", "claude-3-7-sonnet-latest", "general")
        default:
            return ("auto", nil, "general")
        }
    }
}

// MARK: - Coordinator

private final class OverlayApp {
    private var pillWindow: OverlayWindow!
    private var expandedWindow: OverlayWindow?
    private var pillView: PillView!
    private var expandedView: ExpandedPanelView?
    private var expandedPassthroughTimer: Timer?

    /// Pending boot card, if a Boot command arrived before windows materialised.
    private var pendingBoot: (title: String, lines: [String])?

    func start() {
        // Pill window: compact launcher, centered by default.
        let pillSize = PillMetrics.size
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        pillWindow = OverlayWindow(
            contentRect: PillMetrics.centeredFrame(in: screen),
            draggable: true)

        pillView = PillView(frame: NSRect(origin: .zero, size: pillSize))
        pillWindow.contentView = pillView
        pillView.statusText = "Bluey"
        pillView.onClick = { [weak self] in self?.expand() }

        if captureVisibleForDebug {
            NSApp.activate(ignoringOtherApps: true)
        }
        bringPillToFront()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in
            self?.bringPillToFront()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.75) { [weak self] in
            self?.bringPillToFront()
        }

        emitReady()
        emitLifecycle("started", detail: "capture_excluded=\(!captureVisibleForDebug)")
        startParentWatchdog()
        startExpandedPassthroughTracking()
        startIpcLoop()
    }

    private func bringPillToFront() {
        centerPillOnMainScreen()
        pillWindow.setIsVisible(true)
        pillWindow.orderFrontRegardless()
        pillWindow.makeKeyAndOrderFront(nil)
        pillView.needsDisplay = true
        pillView.needsLayout = true
        pillView.layoutSubtreeIfNeeded()
        pillView.displayIfNeeded()
        pillWindow.displayIfNeeded()
    }

    private func centerPillOnMainScreen() {
        let screen = pillWindow.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        pillWindow.setFrame(PillMetrics.centeredFrame(in: screen), display: true)
    }

    private func startParentWatchdog() {
        // LaunchServices helper apps are reparented by macOS, so getppid()
        // is not a reliable daemon-liveness signal in socket IPC mode.
        // The socket reader below exits on EOF when the daemon goes away.
        if overlaySocketPath != nil { return }

        Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { timer in
            if getppid() == 1 {
                timer.invalidate()
                NSApp.terminate(nil)
            }
        }
    }

    private func startExpandedPassthroughTracking() {
        expandedPassthroughTimer?.invalidate()
        expandedPassthroughTimer = Timer.scheduledTimer(withTimeInterval: 0.06, repeats: true) { [weak self] _ in
            guard
                let self,
                let expandedWindow = self.expandedWindow,
                expandedWindow.isVisible,
                let expandedView = self.expandedView
            else { return }

            let mouse = NSEvent.mouseLocation
            let insideWindow = expandedWindow.frame.contains(mouse)
            let shouldAcceptMouse = insideWindow && expandedView.isInteractiveAtScreenPoint(mouse)
            let shouldIgnoreMouse = insideWindow && !shouldAcceptMouse

            if expandedWindow.ignoresMouseEvents != shouldIgnoreMouse {
                expandedWindow.ignoresMouseEvents = shouldIgnoreMouse
            }
        }
    }

    private func expand() {
        ensureExpandedWindow()
        guard let expandedWindow else { return }
        pillWindow?.orderOut(nil)
        expandedWindow.ignoresMouseEvents = false
        expandedWindow.orderFrontRegardless()
        expandedWindow.makeKeyAndOrderFront(nil)
        emitSimple("shown")
        emitLifecycle("expanded")
    }

    private func ensureExpandedWindow() {
        guard expandedWindow == nil else { return }
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
        let expandedWidth = ExpandedPanelMetrics.fittingWidth(
            for: screen,
            preferred: ExpandedPanelMetrics.maxCompactWidth)
        let expandedHeight = ExpandedPanelMetrics.fittingHeight(for: screen)
        let minimumWidth = ExpandedPanelMetrics.fittingMinimumWidth(for: screen, targetWidth: expandedWidth)
        let expandedSize = NSSize(width: expandedWidth, height: expandedHeight)
        let expandedFrame = ExpandedPanelMetrics.fitExpandedFrameToVisibleScreen(
            NSRect(
                x: screen.midX - expandedSize.width / 2,
                y: screen.midY - expandedSize.height / 2,
                width: expandedSize.width,
                height: expandedSize.height),
            visibleFrame: screen)
        let window = OverlayWindow(
            contentRect: expandedFrame,
            draggable: true,
            resizable: true)
        let maxExpandedWidth = max(minimumWidth, screen.width - ExpandedPanelMetrics.screenInset * 2)
        let maxExpandedHeight = max(ExpandedPanelMetrics.minHeight, screen.height - ExpandedPanelMetrics.screenInset * 2)
        window.minimumFrameWidth = minimumWidth
        window.maximumFrameWidth = maxExpandedWidth
        window.minimumFrameHeight = ExpandedPanelMetrics.minHeight
        window.maximumFrameHeight = maxExpandedHeight
        window.minSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.maxSize = NSSize(width: maxExpandedWidth, height: maxExpandedHeight)
        window.contentMinSize = NSSize(width: minimumWidth, height: ExpandedPanelMetrics.minHeight)
        window.contentMaxSize = NSSize(width: maxExpandedWidth, height: maxExpandedHeight)
        let view = ExpandedPanelView(frame: NSRect(origin: .zero, size: expandedFrame.size))
        view.autoresizingMask = [.width, .height]
        window.contentView = view
        view.onClose = { [weak self] in self?.collapse() }
        view.onOpacityChanged = { [weak self] opacity in
            let value = CGFloat(opacity)
            self?.pillWindow?.alphaValue = value
            self?.expandedWindow?.alphaValue = value
        }
        view.onAgentAttachmentChanged = { [weak self] attached in
            self?.pillView?.agentAttached = attached
        }
        expandedWindow = window
        expandedView = view
        if let pending = pendingBoot {
            pushBootCard(title: pending.title, lines: pending.lines)
            pendingBoot = nil
        }
    }

    private func collapse() {
        expandedWindow?.ignoresMouseEvents = false
        expandedWindow?.orderOut(nil)
        bringPillToFront()
        emitSimple("hidden")
        emitLifecycle("collapsed")
    }

    func handleCommand(_ cmd: OverlayCommand) {
        switch cmd {
        case .ping:
            emitSimple("pong")
        case .show:
            expand()
        case .hide:
            // Codex Stage 18 commit 5: smooth fade on the visible windows
            // + center-screen restore-toast for 2s.
            pillWindow?.fadeOutAndHide()
            expandedWindow?.fadeOutAndHide()
            RestoreToast.shared.show()
            emitLifecycle("hidden")

        case .toggle:
            if expandedWindow?.isVisible == true { collapse() } else { expand() }
        case .clear:
            expandedView?.resetSessionSurface()
        case .boot(let title, let lines):
            pushBootCard(title: title, lines: lines)
        case .setOpacity(let o):
            let value = min(max(o, 0.50), 1.0)
            pillWindow.alphaValue = CGFloat(value)
            expandedWindow?.alphaValue = CGFloat(value)
            expandedView?.applyOpacity(value)
        case .setPosition(let pos):
            applyPosition(pos)
        case .setBalance(let label):
            expandedView?.setBalanceLabel(label)
            pillView?.setBalanceLabel(label)
        case .setContextItems(let items):
            expandedView?.setContextItems(items)
        case .setSessions(let sessions):
            expandedView?.setSessions(sessions)
        case .pushCard(let card):
            ensureExpandedWindow()
            expandedView?.pushCard(RenderedCard(
                id: card.id, kind: card.kind, title: card.title,
                body: card.body, done: true, costLabel: card.costLabel,
                artifact: card.artifact, source: card.source))
        case .updateCard(let id, let body, let done, let costLabel, let artifact):
            ensureExpandedWindow()
            expandedView?.updateCard(id: id, body: body, done: done, costLabel: costLabel, artifact: artifact)
        case .setAgents(let agents):
            ensureExpandedWindow()
            expandedView?.setAgents(agents)
        case .setAgentSessions(let kind, let sessions):
            expandedView?.setAgentSessions(kind: kind, sessions: sessions)
        case .setAgentConnectors(let kind, let connectors):
            expandedView?.setAgentConnectors(kind: kind, connectors: connectors)
        case .pushFixProposal(let proposal):
            ensureExpandedWindow()
            expandedView?.pushFixProposal(proposal)
        case .shutdown:
            emitLifecycle("shutdown")
            NSApp.terminate(nil)
        case .unknown:
            break // log-only on stderr happens elsewhere; silently drop.
        }
    }

    private func pushBootCard(title: String, lines: [String]) {
        guard let view = expandedView else {
            pendingBoot = (title, lines)
            return
        }
        let body = lines.joined(separator: "\n")
        let card = RenderedCard(
            id: UUID().uuidString,
            kind: "system",
            title: title,
            body: body,
            done: true,
            costLabel: nil,
            artifact: nil,
            source: nil)
        view.pushCard(card)
        pillView?.dotColor = NSColor.systemGreen
    }

    private func applyPosition(_ pos: String) {
        guard let screen = NSScreen.main?.visibleFrame else { return }
        let pillSize = pillWindow.frame.size
        let inset: CGFloat = 12
        let origin: NSPoint
        switch pos {
        case "top_left":     origin = NSPoint(x: screen.minX + inset,                  y: screen.maxY - pillSize.height - inset)
        case "top_right":    origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.maxY - pillSize.height - inset)
        case "bottom_left":  origin = NSPoint(x: screen.minX + inset,                  y: screen.minY + inset)
        case "bottom_right": origin = NSPoint(x: screen.maxX - pillSize.width - inset, y: screen.minY + inset)
        case "center":       origin = NSPoint(x: screen.midX - pillSize.width / 2,     y: screen.midY - pillSize.height / 2)
        default:             origin = PillMetrics.centeredFrame(in: screen).origin
        }
        pillWindow.setFrameOrigin(origin)
    }

    private func startIpcLoop() {
        // Background thread reads NDJSON from the socket in production, or
        // stdin in tests/manual protocol checks, then dispatches commands onto
        // the main thread because AppKit must run on main.
        let handle = ipcInputHandle ?? FileHandle.standardInput
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var buffer = Data()
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty {
                    DispatchQueue.main.async { NSApp.terminate(nil) }
                    return
                }
                buffer.append(chunk)
                while let nl = buffer.firstIndex(of: 0x0A) {
                    let lineData = buffer.subdata(in: 0..<nl)
                    buffer.removeSubrange(0...nl)
                    if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                        let cmd = parseCommand(line)
                        DispatchQueue.main.async { [weak self] in
                            self?.handleCommand(cmd)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Entry point

private final class AppDelegate: NSObject, NSApplicationDelegate {
    private let coord = OverlayApp()
    func applicationDidFinishLaunching(_ notification: Notification) {
        connectIpcIfNeeded()
        coord.start()
    }
}

private let app = NSApplication.shared
app.setActivationPolicy(captureVisibleForDebug ? .regular : .accessory)
private let delegate = AppDelegate()
app.delegate = delegate
if captureVisibleForDebug {
    app.activate(ignoringOtherApps: true)
}
app.run()


// ─── Codex Stage 18 commit 5: smooth fade + restore toast ───────────────

extension NSWindow {
    func fadeOutAndHide(duration: TimeInterval = 0.25) {
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = duration
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            self.animator().alphaValue = 0
        }, completionHandler: {
            self.orderOut(nil)
            self.alphaValue = 1.0  // restore for next show
        })
    }

    func fadeInAndShow(duration: TimeInterval = 0.18) {
        self.alphaValue = 0
        self.makeKeyAndOrderFront(nil)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = duration
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            self.animator().alphaValue = 1.0
        }
    }
}

private final class RestoreToast {
    static let shared = RestoreToast()
    private var window: NSWindow?
    private var dismissTimer: Timer?

    func show() {
        // Singleton: dismiss any existing toast first.
        dismiss(animated: false)

        guard let mainScreen = NSScreen.main else { return }
        let screenFrame = mainScreen.visibleFrame
        let toastWidth: CGFloat = 280
        let toastHeight: CGFloat = 44
        let frame = NSRect(
            x: screenFrame.midX - toastWidth / 2,
            y: screenFrame.minY + 80,
            width: toastWidth,
            height: toastHeight
        )

        let win = NSWindow(
            contentRect: frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        win.isOpaque = false
        win.backgroundColor = .clear
        win.level = .statusBar
        win.ignoresMouseEvents = true
        win.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        let bg = NSVisualEffectView(frame: NSRect(origin: .zero, size: frame.size))
        bg.material = .hudWindow
        bg.blendingMode = .behindWindow
        bg.state = .active
        bg.wantsLayer = true
        bg.layer?.cornerRadius = 12
        bg.layer?.masksToBounds = true

        let label = NSTextField(labelWithString: "Bluey hidden — press F19 to restore")
        label.alignment = .center
        label.font = NSFont.systemFont(ofSize: 13, weight: .medium)
        label.textColor = NSColor(white: 0.95, alpha: 1.0)
        label.frame = NSRect(x: 12, y: 12, width: frame.size.width - 24, height: 20)
        bg.addSubview(label)

        win.contentView = bg
        win.alphaValue = 0
        win.orderFront(nil)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.18
            ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
            win.animator().alphaValue = 1.0
        }
        self.window = win

        // Dismiss after 2 seconds.
        dismissTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: false) { [weak self] _ in
            self?.dismiss(animated: true)
        }
    }

    func dismiss(animated: Bool) {
        dismissTimer?.invalidate()
        dismissTimer = nil
        guard let win = window else { return }
        if animated {
            NSAnimationContext.runAnimationGroup({ ctx in
                ctx.duration = 0.18
                ctx.timingFunction = CAMediaTimingFunction(name: .easeOut)
                win.animator().alphaValue = 0
            }, completionHandler: {
                win.orderOut(nil)
                self.window = nil
            })
        } else {
            win.orderOut(nil)
            window = nil
        }
    }
}

// Convenience for OverlayWindowController to call.
extension NSWindow {
    func showRestoreToast() {
        RestoreToast.shared.show()
    }
}
