import Carbon.HIToolbox
import Foundation

/// Global hot keys via Carbon `RegisterEventHotKey` (works without Accessibility permission).
final class HotKeyCenter {
    static let shared = HotKeyCenter()

    static let keyH = UInt32(kVK_ANSI_H)
    static let commandOption = UInt32(cmdKey | optionKey)

    private var handlers: [UInt32: () -> Void] = [:]
    private var references: [EventHotKeyRef?] = []
    private var installed = false
    private static let signature: OSType = 0x4148_5544 // 'AHUD'

    func register(id: UInt32, keyCode: UInt32, modifiers: UInt32, handler: @escaping () -> Void) {
        installHandlerIfNeeded()
        handlers[id] = handler
        var reference: EventHotKeyRef?
        let hotKeyID = EventHotKeyID(signature: Self.signature, id: id)
        let status = RegisterEventHotKey(keyCode, modifiers, hotKeyID, GetApplicationEventTarget(), 0, &reference)
        if status != noErr {
            NSLog("[AgentHUD] RegisterEventHotKey failed: %d", status)
        }
        references.append(reference)
    }

    fileprivate func dispatch(id: UInt32) {
        handlers[id]?()
    }

    private func installHandlerIfNeeded() {
        guard !installed else { return }
        installed = true
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(GetApplicationEventTarget(), { _, event, _ -> OSStatus in
            var hotKeyID = EventHotKeyID()
            GetEventParameter(
                event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                nil, MemoryLayout<EventHotKeyID>.size, nil, &hotKeyID
            )
            HotKeyCenter.shared.dispatch(id: hotKeyID.id)
            return noErr
        }, 1, &spec, nil, nil)
    }
}
