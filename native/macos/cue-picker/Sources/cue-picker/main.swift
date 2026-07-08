import AppKit
import Foundation

private let allowedExtensions: Set<String> = [
    "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc",
    "rs", "swift", "c", "h", "cpp", "hpp", "js", "jsx", "ts", "tsx",
    "py", "go", "java", "kt", "kts", "cs", "rb", "php", "sql", "sh",
    "ps1", "toml", "yaml", "yml", "json", "html", "css", "scss",
    "pdf", "doc", "docx", "rtf", "ppt", "pptx", "xls", "xlsx", "xlsm", "xlsb", "ods",
    "png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "bmp", "tiff", "tif",
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
                        "Bluey can attach readable text, code, PDF, Word, PowerPoint, Excel/ODS, CSV/TSV, JSON/YAML/TOML, HTML/CSS, shell/SQL, RTF, or image files only. Video files are not readable context yet."
                ]
            )
        }
    }
}

@main
private enum BlueyContextPicker {
    static func main() {
        let outputPath = outputPathFromArguments()
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let delegate = PickerAppDelegate(outputPath: outputPath)
        app.delegate = delegate
        app.activate(ignoringOtherApps: true)
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
        DispatchQueue.main.async { [weak self] in
            self?.runPanel()
        }
    }

    private func runPanel() {
        let panel = NSOpenPanel()
        panel.title = "Attach files to Bluey"
        panel.message = "Choose readable text, code, PDF/Word/PowerPoint, Excel/ODS, code/data files, RTF, or images. Video, audio, apps, and certificates are skipped."
        panel.prompt = "Attach"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.allowsOtherFileTypes = true
        panel.treatsFilePackagesAsDirectories = false
        panel.delegate = filterDelegate

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
