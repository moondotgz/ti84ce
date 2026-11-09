import SwiftUI

/// The 320×240 color LCD. `s` is the number of points per LCD pixel.
struct LCDView: View {
    let model: ViewModel
    let s: CGFloat

    static let width: CGFloat = 320
    static let height: CGFloat = 240
    static let statusHeight: CGFloat = 30
    static let rowHeight: CGFloat = 21
    static let rows = 10
    static let graphHeight: CGFloat = 210

    var body: some View {
        VStack(spacing: 0) {
            if case .off = model.body {
                Color(hex: 0x0B0D0B)
            } else {
                StatusBar(status: model.status, s: s)
                content
                    .frame(width: Self.width * s, height: Self.graphHeight * s, alignment: .topLeading)
            }
        }
        .frame(width: Self.width * s, height: Self.height * s)
        .background(Color.white)
        .clipped()
    }

    @ViewBuilder
    private var content: some View {
        switch model.body {
        case .off:
            EmptyView()
        case .text(let rows):
            VStack(spacing: 0) {
                TextRows(rows: Array(rows.prefix(model.softkeys == nil ? Self.rows : Self.rows - 1)), s: s)
                Spacer(minLength: 0)
                if let keys = model.softkeys {
                    SoftKeys(labels: keys, s: s)
                }
            }
        case .graph(let g):
            GraphView(g: g, s: s)
        }
    }
}

/// Rows of monospaced text; each span carries its own colors.
struct TextRows: View {
    let rows: [Row]
    let s: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                Text(attributed(row))
                    .font(.custom("Menlo", size: 16.6 * s))
                    .lineLimit(1)
                    .fixedSize()
                    .frame(height: LCDView.rowHeight * s, alignment: .leading)
            }
        }
    }

    private func attributed(_ row: Row) -> AttributedString {
        var out = AttributedString()
        for span in row {
            var a = AttributedString(span.t)
            a.foregroundColor = Color(hex: span.fg)
            if let bg = span.bg {
                a.backgroundColor = Color(hex: bg)
            }
            out += a
        }
        return out
    }
}

struct StatusBar: View {
    let status: Status
    let s: CGFloat

    private var indicator: (String, UInt32)? {
        switch status.indicator {
        case "second": return ("2ND", 0x2F9BD6)
        case "alpha": return ("A", 0x4E9F3D)
        case "alphaLock": return ("A▪", 0x4E9F3D)
        case "lower": return ("a", 0x4E9F3D)
        case "lowerLock": return ("a▪", 0x4E9F3D)
        default: return nil
        }
    }

    var body: some View {
        HStack(spacing: 4 * s) {
            ZStack {
                if let ind = indicator {
                    RoundedRectangle(cornerRadius: 3 * s).fill(Color(hex: ind.1))
                    Text(ind.0).font(.system(size: 10 * s, weight: .bold)).foregroundStyle(.white)
                }
            }
            .frame(width: 30 * s, height: 16 * s)
            Text(status.text)
                .font(.system(size: 10 * s, weight: .semibold))
                .foregroundStyle(Color(hex: 0x2D4E78))
                .lineLimit(1)
            Spacer(minLength: 0)
            if status.busy {
                Text("BUSY").font(.system(size: 10 * s, weight: .bold)).foregroundStyle(Color(hex: 0xC03030))
            }
            HStack(spacing: 0) {
                RoundedRectangle(cornerRadius: 1)
                    .stroke(Color(hex: 0x2D4E78), lineWidth: s)
                    .overlay(Rectangle().fill(Color(hex: 0x3BA55C)).padding(2 * s))
                    .frame(width: 20 * s, height: 10 * s)
                Rectangle().fill(Color(hex: 0x2D4E78)).frame(width: 2 * s, height: 4 * s)
            }
        }
        .padding(.horizontal, 4 * s)
        .frame(height: LCDView.statusHeight * s)
        .background(Color(hex: 0xD8DEE6))
        .overlay(alignment: .bottom) { Rectangle().fill(Color(hex: 0xA8B0BA)).frame(height: s) }
    }
}

struct SoftKeys: View {
    let labels: [String]
    let s: CGFloat

    var body: some View {
        HStack(spacing: 2 * s) {
            ForEach(Array(labels.enumerated()), id: \.offset) { _, label in
                Text(label)
                    .font(.system(size: 10.5 * s))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(
                        UnevenRoundedRectangle(topLeadingRadius: 4 * s, topTrailingRadius: 4 * s)
                            .fill(label.isEmpty ? Color.clear : Color(hex: 0x2D4E78))
                    )
            }
        }
        .padding(.horizontal, s)
        .frame(height: LCDView.rowHeight * s)
    }
}

struct GraphView: View {
    let g: GraphModel
    let s: CGFloat

    var body: some View {
        ZStack(alignment: .topLeading) {
            Canvas { ctx, size in draw(ctx, size) }
            if let top = g.top {
                TextRows(rows: [top], s: s).background(Color.white.opacity(0.78))
            }
            if !g.bottom.isEmpty {
                VStack {
                    Spacer(minLength: 0)
                    TextRows(rows: g.bottom, s: s).background(Color.white.opacity(0.78))
                }
            }
        }
    }

    private func draw(_ ctx: GraphicsContext, _ size: CGSize) {
        let k = size.width / LCDView.width
        let w = LCDView.width, h = LCDView.graphHeight
        func p(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: x * k, y: y * k) }
        func rect(_ x: Double, _ y: Double, _ rw: Double, _ rh: Double) -> Path {
            Path(CGRect(x: x * k, y: y * k, width: rw * k, height: rh * k))
        }
        func line(_ a: CGPoint, _ b: CGPoint, _ width: Double, _ color: Color) {
            var path = Path()
            path.move(to: a)
            path.addLine(to: b)
            ctx.stroke(path, with: .color(color), lineWidth: width * k)
        }

        // Grid.
        var xs = g.xTicks, ys = g.yTicks
        if let y = g.yAxis { xs.append(y) }
        if let x = g.xAxis { ys.append(x) }
        if g.grid == "line" {
            for x in xs { ctx.fill(rect(x, 0, 0.6, h), with: .color(Color(hex: 0xD5D9DE))) }
            for y in ys { ctx.fill(rect(0, y, w, 0.6), with: .color(Color(hex: 0xD5D9DE))) }
        } else if g.grid == "dot" {
            for x in xs { for y in ys { ctx.fill(rect(x - 0.6, y - 0.6, 1.2, 1.2), with: .color(Color(hex: 0x8A9099))) } }
        }

        // Shaded integral.
        if let shade = g.shade, shade.points.count >= 3 {
            var path = Path()
            path.addLines(shade.points.map { p($0.x, $0.y) })
            path.closeSubpath()
            ctx.fill(path, with: .color(Color(hex: shade.color, opacity: 0.3)))
        }

        // Axes and ticks.
        if let y = g.xAxis {
            ctx.fill(rect(0, y - 0.5, w, 1), with: .color(.black))
            for x in g.xTicks { ctx.fill(rect(x - 0.5, y - 2.5, 1, 5), with: .color(.black)) }
        }
        if let x = g.yAxis {
            ctx.fill(rect(x - 0.5, 0, 1, h), with: .color(.black))
            for y in g.yTicks { ctx.fill(rect(x - 2.5, y - 0.5, 5, 1), with: .color(.black)) }
        }

        // Curves.
        for c in g.curves {
            let color = Color(hex: c.color)
            let lw = c.thick ? 2.2 : 1.2
            for seg in c.segments {
                if c.dots || seg.count == 1 {
                    for pt in seg where pt.y >= 0 && pt.y <= h {
                        ctx.fill(rect(pt.x - lw / 2, pt.y - lw / 2, lw, lw), with: .color(color))
                    }
                    continue
                }
                var path = Path()
                path.addLines(seg.map { p($0.x, $0.y) })
                ctx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: lw * k, lineCap: .round, lineJoin: .round))
            }
        }

        // Bound markers.
        for m in g.marks {
            var path = Path()
            path.addLines([p(m.x - 4, 0), p(m.x + 4, 0), p(m.x, 7)])
            path.closeSubpath()
            ctx.fill(path, with: .color(.black))
        }

        // Cursor.
        if let c = g.cursor {
            let (x, y) = (c.x, c.y)
            if c.style == "trace" {
                let color = Color(hex: c.color)
                let r = 4.5
                ctx.stroke(rect(x - r, y - r, 2 * r, 2 * r), with: .color(color), lineWidth: 1.4 * k)
                line(p(x - r - 3, y), p(x - r, y), 1.2, color)
                line(p(x + r, y), p(x + r + 3, y), 1.2, color)
                line(p(x, y - r - 3), p(x, y - r), 1.2, color)
                line(p(x, y + r), p(x, y + r + 3), 1.2, color)
            } else {
                line(p(x - 7, y), p(x - 2, y), 1.2, .black)
                line(p(x + 2, y), p(x + 7, y), 1.2, .black)
                line(p(x, y - 7), p(x, y - 2), 1.2, .black)
                line(p(x, y + 2), p(x, y + 7), 1.2, .black)
            }
        }
    }
}
