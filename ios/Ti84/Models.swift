import SwiftUI

// Decodable mirror of the JSON produced by `ti84_view_json` / `ti84_keypad_json`
// (see ffi/src/lib.rs).

struct Span: Decodable, Hashable {
    let t: String
    let fg: UInt32
    let bg: UInt32?
}

typealias Row = [Span]

struct Status: Decodable {
    let text: String
    let indicator: String
    let busy: Bool
}

/// A point encoded as a two-element JSON array `[x, y]`.
struct Pt: Decodable {
    let x: Double
    let y: Double

    init(from decoder: Decoder) throws {
        var c = try decoder.unkeyedContainer()
        x = try c.decode(Double.self)
        y = try c.decode(Double.self)
    }
}

struct Curve: Decodable {
    let color: UInt32
    let thick: Bool
    let dots: Bool
    let segments: [[Pt]]
}

struct GraphCursor: Decodable {
    let x: Double
    let y: Double
    let style: String
    let color: UInt32
}

struct Shade: Decodable {
    let color: UInt32
    let points: [Pt]
}

struct GraphModel: Decodable {
    let curves: [Curve]
    let xAxis: Double?
    let yAxis: Double?
    let xTicks: [Double]
    let yTicks: [Double]
    let grid: String
    let cursor: GraphCursor?
    let marks: [Pt]
    let shade: Shade?
    let top: Row?
    let bottom: [Row]
}

enum Body: Decodable {
    case off
    case text([Row])
    case graph(GraphModel)

    private enum Keys: String, CodingKey { case kind, rows }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "text": self = .text(try c.decode([Row].self, forKey: .rows))
        case "graph": self = .graph(try GraphModel(from: decoder))
        default: self = .off
        }
    }
}

struct ViewModel: Decodable {
    let status: Status
    let softkeys: [String]?
    let body: Body

    static let blank = ViewModel(status: Status(text: "", indicator: "none", busy: false), softkeys: nil, body: .text([]))
}

struct KeyDef: Decodable, Identifiable {
    let code: UInt32
    let label: String
    let second: String
    let alpha: String
    let style: String
    var id: UInt32 { code }
}

struct Keypad: Decodable {
    struct Arrows: Decodable {
        let up: UInt32
        let down: UInt32
        let left: UInt32
        let right: UInt32
    }

    let top: [KeyDef]
    let upperLeft: [[KeyDef]]
    let grid: [[KeyDef]]
    let arrows: Arrows

    /// Look up a key code by its printed label (e.g. "enter", "clear").
    func code(_ label: String) -> UInt32? {
        (top + upperLeft.flatMap { $0 } + grid.flatMap { $0 }).first { $0.label == label }?.code
    }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}
