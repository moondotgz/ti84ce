import Foundation
import Ti84Core
import UIKit

/// Owns the Rust calculator core and republishes its screen for SwiftUI.
@MainActor
final class Calculator: ObservableObject {
    @Published private(set) var screen: ViewModel = .blank
    let keypad: Keypad

    private let handle: OpaquePointer
    private var timer: Timer?
    private var ticks = 0
    private var blink = true
    private let haptics = UIImpactFeedbackGenerator(style: .light)

    init() {
        // Python scripts live in Documents/Python, visible in the Files app.
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Python", isDirectory: true)
        handle = docs.path.withCString { ti84_new($0) }
        keypad = Calculator.decodeKeypad()
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
    }

    deinit {
        timer?.invalidate()
        ti84_free(handle)
    }

    private static func decodeKeypad() -> Keypad {
        guard let raw = ti84_keypad_json() else { fatalError("keypad layout unavailable") }
        defer { ti84_string_free(raw) }
        let data = Data(String(cString: raw).utf8)
        do {
            return try JSONDecoder().decode(Keypad.self, from: data)
        } catch {
            fatalError("bad keypad JSON: \(error)")
        }
    }

    private func refresh() {
        guard let raw = ti84_view_json(handle) else { return }
        defer { ti84_string_free(raw) }
        let data = Data(String(cString: raw).utf8)
        if let model = try? JSONDecoder().decode(ViewModel.self, from: data) {
            screen = model
        }
    }

    private func tick() {
        ticks += 1
        var changed = ti84_tick(handle)
        if ticks % 10 == 0 {
            blink.toggle()
            ti84_set_blink(handle, blink)
            changed = true
        }
        if changed { refresh() }
    }

    /// Show the cursor immediately after input, then resume blinking.
    private func afterInput() {
        ticks = 0
        blink = true
        ti84_set_blink(handle, true)
        refresh()
    }

    func press(_ code: UInt32) {
        haptics.impactOccurred()
        ti84_press(handle, code)
        afterInput()
    }

    /// Press a key without haptic feedback (hardware keyboard).
    func pressQuietly(_ code: UInt32) {
        ti84_press(handle, code)
        afterInput()
    }

    func type(_ text: String) {
        text.withCString { ti84_type(handle, $0) }
        afterInput()
    }

    func paste(_ text: String) {
        text.withCString { ti84_paste(handle, $0) }
        afterInput()
    }

    func backspace() {
        ti84_backspace(handle)
        afterInput()
    }
}
