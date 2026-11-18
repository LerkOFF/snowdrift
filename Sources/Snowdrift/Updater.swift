import AppKit

/// "Check for Updates…": asks GitHub for the latest release, and on a yes downloads `Snowdrift.zip`,
/// swaps it in for the running app and starts the new copy.
@MainActor
enum Updater {
    private static let latest = URL(string: "https://api.github.com/repos/LerkOFF/snowdrift/releases/latest")!
    private static let archive = "Snowdrift.zip"
    private static var busy = false

    static func check() {
        guard !busy else { return }
        busy = true
        Task {
            defer { busy = false }
            let release: Release
            let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
            do {
                release = try await fetchLatest()
                guard isNewer(release.version, than: current) else {
                    show(tr("update.latest").replacingOccurrences(of: "{current}", with: current))
                    return
                }
                let question = tr("update.ask")
                    .replacingOccurrences(of: "{version}", with: release.version)
                    .replacingOccurrences(of: "{current}", with: current)
                guard confirm(question) else { return }
            } catch {
                NSLog("Snowdrift: update check failed: \(error)")
                show("\(tr("update.failed"))\n\n\(error.localizedDescription)")
                return
            }
            do {
                try await install(release)
            } catch {
                NSLog("Snowdrift: update install failed: \(error)")
                show("\(tr("update.install.failed"))\n\n\(error.localizedDescription)")
            }
        }
    }

    // MARK: Looking up the release

    private struct Release {
        let version: String
        let download: URL
    }

    private struct Failure: LocalizedError {
        let errorDescription: String?
        init(_ text: String) { errorDescription = text }
    }

    private static func fetchLatest() async throws -> Release {
        var request = URLRequest(url: latest, timeoutInterval: 15)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        if let status = (response as? HTTPURLResponse)?.statusCode, status != 200 {
            throw Failure("GitHub answered \(status)")
        }
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tag = json["tag_name"] as? String,
              let assets = json["assets"] as? [[String: Any]],
              let asset = assets.first(where: { $0["name"] as? String == archive }),
              let address = asset["browser_download_url"] as? String,
              let download = URL(string: address), download.scheme == "https"
        else { throw Failure("the latest release has no \(archive)") }
        return Release(version: tag.hasPrefix("v") ? String(tag.dropFirst()) : tag, download: download)
    }

    private static func isNewer(_ candidate: String, than current: String) -> Bool {
        func parts(_ version: String) -> [Int] {
            let numbers = version.split(separator: ".").map { Int($0) ?? 0 }
            return Array((numbers + [0, 0, 0]).prefix(3))
        }
        return parts(current).lexicographicallyPrecedes(parts(candidate))
    }

    // MARK: Replacing the app

    private static func install(_ release: Release) async throws {
        let files = FileManager.default
        let target = Bundle.main.bundleURL
        guard !target.path.hasPrefix("/Volumes/"), !target.path.contains("/AppTranslocation/") else {
            throw Failure("Snowdrift runs from a disk image; move it to Applications first")
        }
        guard files.isWritableFile(atPath: target.deletingLastPathComponent().path) else {
            throw Failure("\(target.deletingLastPathComponent().path) isn't writable")
        }
        // On the target's volume, so swapping the app in is one atomic step.
        let work = try files.url(for: .itemReplacementDirectory, in: .userDomainMask, appropriateFor: target, create: true)
        defer { try? files.removeItem(at: work) }

        let (downloaded, response) = try await URLSession.shared.download(from: release.download)
        if let status = (response as? HTTPURLResponse)?.statusCode, status != 200 {
            throw Failure("the download answered \(status)")
        }
        let zip = work.appendingPathComponent(archive)
        try files.moveItem(at: downloaded, to: zip)
        let unpacked = work.appendingPathComponent("new", isDirectory: true)
        try files.createDirectory(at: unpacked, withIntermediateDirectories: true)
        try run("/usr/bin/ditto", ["-x", "-k", zip.path, unpacked.path])

        let fresh = unpacked.appendingPathComponent(target.lastPathComponent)
        guard files.fileExists(atPath: fresh.path) else { throw Failure("the download has no \(target.lastPathComponent)") }
        try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", fresh.path])
        let info = NSDictionary(contentsOf: fresh.appendingPathComponent("Contents/Info.plist"))
        guard info?["CFBundleIdentifier"] as? String == Bundle.main.bundleIdentifier,
              info?["CFBundleShortVersionString"] as? String == release.version
        else { throw Failure("the downloaded app isn't Snowdrift \(release.version)") }
        _ = try? run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", fresh.path])

        _ = try files.replaceItemAt(target, withItemAt: fresh)
        relaunch(target)
    }

    /// Starts the new copy once this one is gone: two copies at once would show the snow twice.
    private static func relaunch(_ app: URL) {
        let script = """
        while /bin/kill -0 \(ProcessInfo.processInfo.processIdentifier) 2>/dev/null; do /bin/sleep 0.1; done
        /usr/bin/open "$1"
        """
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", script, "updater", app.path]
        try? process.run()
        NSApp.terminate(nil)
    }

    @discardableResult
    private static func run(_ path: String, _ arguments: [String]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        let output = Pipe()
        process.standardOutput = output
        process.standardError = output
        try process.run()
        process.waitUntilExit()
        let text = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        guard process.terminationStatus == 0 else {
            throw Failure("\((path as NSString).lastPathComponent): \(text.trimmingCharacters(in: .whitespacesAndNewlines))")
        }
        return text
    }

    // MARK: Dialogs

    private static func confirm(_ text: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = "Snowdrift"
        alert.informativeText = text
        alert.addButton(withTitle: tr("setup.update"))
        alert.addButton(withTitle: tr("update.later"))
        NSApp.activate(ignoringOtherApps: true)
        return alert.runModal() == .alertFirstButtonReturn
    }

    private static func show(_ text: String) {
        let alert = NSAlert()
        alert.messageText = "Snowdrift"
        alert.informativeText = text
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }
}
