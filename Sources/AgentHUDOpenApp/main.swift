import AppKit
import AgentHUDCore
import AgentHUDDesktop

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var desktop: DesktopApplication?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let options = DesktopLaunchOptions.parse(CommandLine.arguments)
        if let directory = options.snapshotDirectory {
            Task { @MainActor in
                await SnapshotRunner.run(language: options.language, into: directory)
                NSApp.terminate(nil)
            }
            return
        }
        if options.resetDefaults, let bundleID = Bundle.main.bundleIdentifier {
            UserDefaults.standard.removePersistentDomain(forName: bundleID)
        }
        let defaults = options.demo ? UserDefaults(suiteName: "app.agenthud.open.demo")! : .standard
        let settings = SettingsStore(defaults: defaults)
        if let language = options.language { settings.update { $0.language = language } }
        L10n.setLanguage(settings.settings.language)
        let provider: any UsageProvider = options.demo ? DemoUsageProvider() : CombinedUsageProvider.standard()
        if options.probe {
            Task { @MainActor in
                do {
                    let report = try await provider.fetchUsage(agents: settings.agents, historyHours: 48)
                    print("Quota windows: \(report.snapshots.count); sessions: \(report.sessions.count); live: \(report.sessions.filter(\.isLive).count); billing accounts: \(report.billing.count)")
                    exit(0)
                } catch {
                    FileHandle.standardError.write(Data("Usage probe failed: \(error.localizedDescription)\n".utf8))
                    exit(1)
                }
            }
            return
        }
        let retained = RetainedUsageProvider(provider: provider, cacheURL: options.demo ? nil
            : AppSupport.directory.appendingPathComponent("last-usage-report.json"))
        let store = UsageStore(provider: retained, settings: settings)
        if let report = retained.initialReport { store.replace(report: report) }
        let desktop = DesktopApplication(options: options, settings: settings, store: store)
        self.desktop = desktop
        desktop.start()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationWillTerminate(_ notification: Notification) { desktop?.stop() }
}

if CommandLine.arguments.contains("--probe-open-agents") {
    Task { print(await OpenAgentDiagnostics.localSummary()); exit(0) }
    dispatchMain()
}

if CommandLine.arguments.count == 3, CommandLine.arguments[1] == "--completion-hook",
   let source = CompletionHooks.Source(rawValue: CommandLine.arguments[2]) {
    var data = Data()
    do {
        while let chunk = try FileHandle.standardInput.read(upToCount: 64 * 1024), !chunk.isEmpty {
            data.append(chunk)
            if data.count > 1024 * 1024 { break }
        }
        try CompletionHooks.record(source: source, data: data)
    } catch { /* Local status tracking must not affect the agent's execution. */ }
    print(source == .antigravity ? #"{"decision":"stop"}"# : "{}")
    exit(0)
}

if CommandLine.arguments.count == 3, CommandLine.arguments[1] == "--install-completion-hook",
   let source = CompletionHooks.Source(rawValue: CommandLine.arguments[2]) {
    do {
        try CompletionHooks.configure(source, enabled: true,
            executable: URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL)
        print("Completion hook installed: \(source.rawValue)")
        exit(0)
    } catch {
        FileHandle.standardError.write(Data("Could not install completion hook: \(error.localizedDescription)\n".utf8))
        exit(1)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
