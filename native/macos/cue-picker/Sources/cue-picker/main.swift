import AppKit
import Foundation
import UniformTypeIdentifiers

private let allowedExtensions: Set<String> = [
    "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc",
    "rs", "swift", "c", "h", "cpp", "hpp", "js", "jsx", "ts", "tsx",
    "py", "go", "java", "kt", "kts", "cs", "rb", "php", "sql", "sh",
    "ps1", "toml", "yaml", "yml", "json", "html", "css", "scss",
    "pdf", "doc", "docx", "rtf",
    // Images — sent to the agent as pixels over ACP (the "+"-menu attach path).
    "png", "jpg", "jpeg", "gif", "webp", "heic", "bmp",
]

private func isAllowedContextFile(_ url: URL) -> Bool {
    let resourceValues = try? url.resourceValues(forKeys: [.isDirectoryKey])
    if resourceValues?.isDirectory == true {
        return true
    }
    return allowedExtensions.contains(url.pathExtension.lowercased())
}

private final class ContextFilePanelDelegate: NSObject, NSOpenSavePanelDelegate {
    func panel(_ sender: Any, shouldEnable url: URL) -> Bool {
        isAllowedContextFile(url)
    }

    func panel(_ sender: Any, validate url: URL) throws {
        guard isAllowedContextFile(url) else {
            throw NSError(
                domain: "sh.bluey.file-picker",
                code: 1,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Bluey can attach readable text, code, PDF, DOC, DOCX, CSV/TSV, JSON/YAML/TOML, HTML/CSS, shell/SQL, or RTF files only."
                ]
            )
        }
    }
}

@main
private enum BlueyContextPicker {
    // The picker is a short-lived helper the daemon spawns via `open -W`. It must
    // show a FRONTMOST NSOpenPanel, write the chosen paths, and quit.
    //
    // CRITICAL: a real NSApplication RUN LOOP is required. Calling `runModal()`
    // directly from `main()` (without `app.run()`) returns before the panel's
    // window is ever created (window count = 0) — the modal resolves instantly.
    // So we keep the delegate + `app.run()` lifecycle and present the panel from
    // `applicationDidFinishLaunching`, where the app is fully initialized and the
    // window server connection is live. The one real fix over the original is the
    // activation POLICY: `.regular` (not `.accessory`/LSUIElement), so the panel
    // gets a normal frontmost window the user can actually see.
    static func main() {
        let outputPath = outputPathFromArguments()
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)
        let delegate = PickerAppDelegate(outputPath: outputPath)
        app.delegate = delegate
        withExtendedLifetime(delegate) {
            app.run()
        }
    }
}

private final class PickerAppDelegate: NSObject, NSApplicationDelegate {
    private let filterDelegate = ContextFilePanelDelegate()
    private let outputPath: String?

    init(outputPath: String?) {
        self.outputPath = outputPath
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Bring the app forward, then present the panel on the next run-loop pass
        // so the window server connection is fully live before the modal draws.
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
            self?.runPanel()
        }
    }

    private func runPanel() {
        NSApp.activate(ignoringOtherApps: true)

        let panel = NSOpenPanel()
        panel.title = "Attach files to Bluey"
        panel.message = "Choose text, code, PDF, DOC/DOCX, CSV/TSV, JSON/YAML/TOML, HTML/CSS, or images (PNG/JPG/GIF/WebP/HEIC). Images are sent to your agent as-is. Video, audio, apps, and certificates are skipped."
        panel.prompt = "Attach"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.allowsOtherFileTypes = false
        panel.treatsFilePackagesAsDirectories = false
        panel.delegate = filterDelegate

        let contentTypes = Array(Set(allowedExtensions.compactMap { UTType(filenameExtension: $0) }))
        if !contentTypes.isEmpty {
            panel.allowedContentTypes = contentTypes
        }

        // Force the panel window to materialize + take focus BEFORE runModal. For
        // a freshly-launched (Dock-visible) app the modal loop otherwise runs with
        // no drawn window — the "Dock icon appears but no dialog" symptom. center()
        // + makeKeyAndOrderFront place a real, focused window on screen.
        panel.center()
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

        var paths: [String] = []
        if panel.runModal() == .OK {
            paths = panel.urls
                .filter(isAllowedContextFile)
                .map(\.path)
        }

        write(paths: paths)
        NSApp.terminate(nil)
    }

    private func write(paths: [String]) {
        let body = paths.joined(separator: "\n")
        if let outputPath {
            try? body.write(toFile: outputPath, atomically: true, encoding: .utf8)
        } else if !body.isEmpty {
            print(body)
        }
    }
}

private func outputPathFromArguments() -> String? {
    let args = CommandLine.arguments
    for index in args.indices where args[index] == "--output" {
        let next = args.index(after: index)
        guard next < args.endIndex else { return nil }
        return args[next]
    }
    return nil
}
