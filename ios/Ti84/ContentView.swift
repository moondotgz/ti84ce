import SwiftUI
import UIKit

private let lcdScale: CGFloat = 1.15

struct ContentView: View {
    @StateObject private var calc = Calculator()
    @FocusState private var focused: Bool

    var body: some View {
        GeometryReader { geo in
            let u = min(geo.size.width / 420, geo.size.height / 944)
            ZStack {
                Color(hex: 0x14161A).ignoresSafeArea()
                VStack(spacing: 4 * u) {
                    header(u)
                    LCDView(model: calc.screen, s: lcdScale * u)
                        .padding(12 * u)
                        .background(RoundedRectangle(cornerRadius: 12 * u).fill(Color(hex: 0x0B0B0C)))
                        .contextMenu {
                            Button("Paste", systemImage: "doc.on.clipboard") {
                                if let text = UIPasteboard.general.string { calc.paste(text) }
                            }
                        }
                    Spacer().frame(height: 14 * u)
                    KeypadView(calc: calc, u: u)
                }
                .padding(.top, 10 * u)
                .frame(width: 420 * u, height: 944 * u, alignment: .top)
                .background(
                    RoundedRectangle(cornerRadius: 38 * u)
                        .fill(Color(hex: 0x1F2125))
                        .overlay(RoundedRectangle(cornerRadius: 38 * u).stroke(Color(hex: 0x3A3D42), lineWidth: 2 * u))
                )
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onAppear { focused = true }
        .onKeyPress(phases: .down) { press in handleKey(press) }
        .persistentSystemOverlays(.hidden)
    }

    private func header(_ u: CGFloat) -> some View {
        HStack(alignment: .lastTextBaseline) {
            Text("TI-84 Plus CE-T").font(.system(size: 15 * u, weight: .bold)).foregroundStyle(Color(hex: 0xF4F4F4))
            Spacer()
            Text("PYTHON EDITION").font(.system(size: 11 * u, weight: .bold)).foregroundStyle(Color(hex: 0xF2C744))
        }
        .frame(width: 372 * u, height: 30 * u, alignment: .bottom)
        .padding(.bottom, 4 * u)
    }

    /// Hardware keyboard support (iPad keyboards, Bluetooth keyboards).
    private func handleKey(_ press: KeyPress) -> KeyPress.Result {
        let pad = calc.keypad
        let shift = press.modifiers.contains(.shift)
        func key(_ code: UInt32?) -> KeyPress.Result {
            guard let code else { return .ignored }
            if shift, let second = pad.code("2nd") { calc.pressQuietly(second) }
            calc.pressQuietly(code)
            return .handled
        }
        if press.modifiers.contains(.command) {
            if press.characters == "v", let text = UIPasteboard.general.string {
                calc.paste(text)
                return .handled
            }
            return .ignored
        }
        switch press.key {
        case .return: return key(pad.code("enter"))
        case .escape: return key(pad.code("clear"))
        case .delete:
            calc.backspace()
            return .handled
        case .deleteForward: return key(pad.code("del"))
        case .upArrow: return key(pad.arrows.up)
        case .downArrow: return key(pad.arrows.down)
        case .leftArrow: return key(pad.arrows.left)
        case .rightArrow: return key(pad.arrows.right)
        case .tab:
            calc.paste("  ")
            return .handled
        default:
            guard !press.characters.isEmpty else { return .ignored }
            calc.type(press.characters)
            return .handled
        }
    }
}
