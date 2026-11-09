import SwiftUI

private let secondLabel: UInt32 = 0x6EC3F0
private let alphaLabel: UInt32 = 0x8CCB5E

/// Cap, text and pressed colors for each key style.
private func colors(_ style: String) -> (UInt32, UInt32, UInt32) {
    switch style {
    case "top": return (0xE9EAEC, 0x1B1C1E, 0xC5C7CA)
    case "number": return (0xEDEDED, 0x111111, 0xC9C9C9)
    case "op", "enter": return (0xD3D5D8, 0x111111, 0xB0B2B5)
    case "second": return (0x2F9BD6, 0xFFFFFF, 0x2380B3)
    case "alpha": return (0x4E9F3D, 0xFFFFFF, 0x3D8030)
    default: return (0x34373C, 0xF2F2F2, 0x24262A)
    }
}

/// A key cap that fires on touch-down, like the desktop version.
private struct CapStyle: ButtonStyle {
    let bg: UInt32
    let pressed: UInt32
    let radius: CGFloat
    let edge: CGFloat

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(
                RoundedRectangle(cornerRadius: radius)
                    .fill(Color(hex: configuration.isPressed ? pressed : bg))
                    .shadow(color: .black.opacity(0.45), radius: 0, x: 0, y: configuration.isPressed ? 0 : edge)
            )
            .offset(y: configuration.isPressed ? edge / 2 : 0)
    }
}

private struct KeyButton: View {
    let def: KeyDef
    let colW: CGFloat
    let capH: CGFloat
    let u: CGFloat
    let action: (UInt32) -> Void

    var body: some View {
        let (bg, fg, pressed) = colors(def.style)
        let capW = colW - 12
        let small = def.style == "top" || def.label.count > 4
        VStack(spacing: 0) {
            HStack {
                Text(def.second).foregroundStyle(Color(hex: secondLabel))
                Spacer(minLength: 0)
                Text(def.alpha).foregroundStyle(Color(hex: alphaLabel))
            }
            .font(.system(size: 9 * u))
            .lineLimit(1)
            .frame(width: (capW + 4) * u, height: 13 * u)
            TouchDownButton(code: def.code, action: action) {
                Text(def.label)
                    .font(.system(size: (small ? 11 : 14.5) * u, weight: .medium))
                    .foregroundStyle(Color(hex: fg))
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
            }
            .buttonStyle(CapStyle(bg: bg, pressed: pressed, radius: 7 * u, edge: 2.5 * u))
            .frame(width: capW * u, height: capH * u)
        }
        .frame(width: colW * u)
    }
}

/// A button that triggers on touch-down so fast typing feels like a real keypad.
private struct TouchDownButton<Label: View>: View {
    let code: UInt32
    let action: (UInt32) -> Void
    @ViewBuilder let label: () -> Label
    @State private var down = false

    var body: some View {
        Button(action: {}, label: label)
            .simultaneousGesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !down {
                            down = true
                            action(code)
                        }
                    }
                    .onEnded { _ in down = false }
            )
    }
}

struct KeypadView: View {
    @ObservedObject var calc: Calculator
    let u: CGFloat
    private let col: CGFloat = 76

    var body: some View {
        let pad = calc.keypad
        let press: (UInt32) -> Void = { calc.press($0) }
        VStack(spacing: 6 * u) {
            HStack(spacing: 0) {
                ForEach(pad.top) { KeyButton(def: $0, colW: col, capH: 24, u: u, action: press) }
            }
            HStack(alignment: .center, spacing: 0) {
                VStack(spacing: 4 * u) {
                    ForEach(Array(pad.upperLeft.enumerated()), id: \.offset) { _, row in
                        HStack(spacing: 0) {
                            ForEach(row) { KeyButton(def: $0, colW: col, capH: 30, u: u, action: press) }
                        }
                    }
                }
                DPad(arrows: pad.arrows, u: u, action: press)
            }
            VStack(spacing: 4 * u) {
                ForEach(Array(pad.grid.enumerated()), id: \.offset) { _, row in
                    HStack(spacing: 0) {
                        ForEach(row) { KeyButton(def: $0, colW: col, capH: 32, u: u, action: press) }
                    }
                }
            }
        }
        .frame(width: 380 * u)
    }
}

private struct DPad: View {
    let arrows: Keypad.Arrows
    let u: CGFloat
    let action: (UInt32) -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            Capsule().fill(Color(hex: 0x18191B))
                .frame(width: 44 * u, height: 34 * u)
                .offset(x: 54 * u, y: 32 * u)
            arrow(arrows.up, "arrowtriangle.up.fill", x: 51, y: 2, w: 50, h: 30)
            arrow(arrows.down, "arrowtriangle.down.fill", x: 51, y: 66, w: 50, h: 30)
            arrow(arrows.left, "arrowtriangle.left.fill", x: 8, y: 29, w: 44, h: 40)
            arrow(arrows.right, "arrowtriangle.right.fill", x: 100, y: 29, w: 44, h: 40)
        }
        .frame(width: 152 * u, height: 98 * u, alignment: .topLeading)
    }

    private func arrow(_ code: UInt32, _ symbol: String, x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat) -> some View {
        TouchDownButton(code: code, action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11 * u))
                .foregroundStyle(Color(hex: 0xE8E8E8))
        }
        .buttonStyle(CapStyle(bg: 0x3B3E44, pressed: 0x2A2C30, radius: 10 * u, edge: 2.5 * u))
        .frame(width: w * u, height: h * u)
        .offset(x: x * u, y: y * u)
    }
}
