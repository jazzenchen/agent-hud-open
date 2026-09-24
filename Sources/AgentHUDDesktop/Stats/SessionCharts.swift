import SwiftUI
import AgentHUDCore

/// One bar of a session chart: a turn, or a 15-minute period for a log that does not mark its prompts.
struct SessionBar: Identifiable {
    let id: Int
    let start: Date
    let end: Date
    let kinds: TokenKinds
    let calls: Int
    let context: Int?
    let compacted: Bool

    static func turns(_ usage: SessionUsage) -> [SessionBar] {
        let skipped = usage.turnCount - usage.turns.count
        return usage.turns.enumerated().map { index, turn in
            SessionBar(id: skipped + index + 1, start: turn.start, end: turn.end, kinds: turn.tokens.kinds, calls: turn.calls,
                       context: turn.contextTokens, compacted: turn.compacted)
        }
    }

    static func periods(_ usage: SessionUsage) -> [SessionBar] {
        var kinds: [Date: TokenKinds] = [:]
        for period in usage.periods { kinds[period.start, default: .init()] += period.tokens.kinds }
        return kinds.keys.sorted().enumerated().map { index, start in
            SessionBar(id: index + 1, start: start, end: start.addingTimeInterval(UsageBucket.duration), kinds: kinds[start]!, calls: 0,
                       context: nil, compacted: false)
        }
    }
}

/// Turns stand side by side; 15-minute periods, for a log that does not mark its prompts, sit at their times.
enum SessionChartAxis: Hashable { case turn, time }

/// The x positions of a session's bars: evenly spaced turns, or each bar at its time and as wide as it lasted.
struct SessionChartLayout {
    let bars: [SessionBar]
    let axis: SessionChartAxis
    let width: CGFloat

    private var span: (start: Date, duration: TimeInterval) {
        let start = bars.first?.start ?? Date(), end = bars.map(\.end).max() ?? start
        return (start, max(60, end.timeIntervalSince(start)))
    }

    func frame(_ index: Int) -> (x: CGFloat, width: CGFloat, center: CGFloat) {
        guard !bars.isEmpty else { return (0, 0, 0) }
        switch axis {
        case .turn:
            let slot = width / CGFloat(bars.count), bar = max(1, slot * 0.64)
            return (CGFloat(index) * slot + (slot - bar) / 2, bar, CGFloat(index) * slot + slot / 2)
        case .time:
            let span = span, bar = bars[index]
            let x = CGFloat(bar.start.timeIntervalSince(span.start) / span.duration) * width
            let w = max(3, CGFloat(bar.end.timeIntervalSince(bar.start) / span.duration) * width)
            return (min(x, width - w), w, min(x, width - w) + w / 2)
        }
    }

    func index(at x: CGFloat) -> Int? {
        guard !bars.isEmpty, width > 0 else { return nil }
        switch axis {
        case .turn: return min(bars.count - 1, max(0, Int(x / (width / CGFloat(bars.count)))))
        case .time: return bars.indices.min { abs(frame($0).center - x) < abs(frame($1).center - x) }
        }
    }

    /// Three or four labels: turn numbers, or clock times.
    func ticks() -> [(x: CGFloat, label: String)] {
        guard let first = bars.first, let last = bars.last else { return [] }
        switch axis {
        case .turn:
            let positions = Array(Set([0, (bars.count - 1) / 3, 2 * (bars.count - 1) / 3, bars.count - 1])).sorted()
            return positions.map { (frame($0).center, "T\(bars[$0].id)") }
        case .time:
            let span = span
            let formatter = Date.FormatStyle(date: .omitted, time: .shortened)
            let dates = [span.start, span.start.addingTimeInterval(span.duration / 2), max(last.end, first.start)]
            return dates.enumerated().map { index, date in
                (CGFloat(date.timeIntervalSince(span.start) / span.duration) * width, date.formatted(formatter))
            }
        }
    }
}

/// A step that divides `peak` into about `count` round intervals.
func niceStep(_ peak: Int, count: Int = 3) -> Int {
    let raw = max(1, Double(peak) / Double(count)), power = pow(10, floor(log10(raw)))
    let multiple = [1, 2, 2.5, 5, 10].first { $0 * power >= raw } ?? 10
    return max(1, Int(multiple * power))
}

/// The new tokens of each bar, stacked by kind; cache reads are drawn apart as the context.
struct SessionBarsChart: View {
    static let stacked: [TokenKind] = [.cacheWrite, .input, .reasoning, .output]

    let bars: [SessionBar]
    let axis: SessionChartAxis
    /// Outlines the last bar while its turn runs.
    let running: Bool
    let theme: Theme
    @Binding var inspected: Int?
    private let axisWidth: CGFloat = 36
    private let height: CGFloat = 150

    var body: some View {
        let peak = bars.map(\.kinds.new).max() ?? 0
        let step = niceStep(peak), top = max(step, Int((Double(peak) / Double(step)).rounded(.up)) * step)
        GeometryReader { proxy in
            let plot = proxy.size.width - axisWidth
            let layout = SessionChartLayout(bars: bars, axis: axis, width: plot)
            ZStack(alignment: .topLeading) {
                Canvas { context, _ in
                    let scale = (height - 6) / CGFloat(top)
                    for value in stride(from: 0, through: top, by: step) {
                        let y = height - CGFloat(value) * scale
                        context.stroke(Path { $0.move(to: CGPoint(x: 0, y: y)); $0.addLine(to: CGPoint(x: plot, y: y)) },
                                       with: .color(value == 0 ? theme.divider : theme.divider.opacity(0.6)), lineWidth: 1)
                    }
                    for (index, bar) in bars.enumerated() {
                        let frame = layout.frame(index)
                        var y = height
                        var barContext = context
                        barContext.opacity = inspected == nil || inspected == index ? 1 : 0.5
                        for kind in Self.stacked where bar.kinds[kind] > 0 {
                            let h = CGFloat(bar.kinds[kind]) * scale
                            y -= h
                            barContext.fill(Path(CGRect(x: frame.x, y: y + 0.4, width: frame.width, height: max(0.6, h - 0.8))),
                                            with: .color(theme.kind(kind)))
                        }
                    }
                    if running, let last = bars.indices.last {
                        let frame = layout.frame(last), h = CGFloat(bars[last].kinds.new) * scale
                        context.stroke(Path(roundedRect: CGRect(x: frame.x - 1.5, y: height - h - 1.5, width: frame.width + 3, height: h + 1.5),
                                            cornerRadius: 1.5), with: .color(theme.text.opacity(0.9)), lineWidth: 1)
                    }
                }
                .frame(width: plot, height: height)
                ForEach(Array(stride(from: 0, through: top, by: step)), id: \.self) { value in
                    Text(value == 0 ? "0" : TokenFormat.short(value))
                        .font(.tabular(9)).foregroundStyle(theme.secondary)
                        .position(x: plot + 6 + 14, y: height - CGFloat(value) * (height - 6) / CGFloat(top))
                }
                ForEach(Array(layout.ticks().enumerated()), id: \.offset) { index, tick in
                    Text(tick.label).font(.tabular(9)).foregroundStyle(theme.secondary).fixedSize()
                        .position(x: min(max(tick.x, 12), plot - 12), y: height + 10)
                }
            }
            .contentShape(Rectangle())
            .onContinuousHover { phase in
                switch phase {
                case .active(let point): inspected = point.x <= plot ? layout.index(at: point.x) : nil
                case .ended: inspected = nil
                }
            }
        }
        .frame(height: height + 18)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L10n.text("每轮 Token 图表", "Tokens per turn chart"))
    }
}

/// Each turn's context — cache reads and new input of its last call — against the model's window.
struct ContextWindowChart: View {
    let bars: [SessionBar]
    let axis: SessionChartAxis
    let window: Int?
    let theme: Theme
    private let axisWidth: CGFloat = 36
    private let height: CGFloat = 58

    var body: some View {
        let peak = bars.compactMap(\.context).max() ?? 0
        let top = window ?? max(1, niceStep(peak, count: 2) * 2)
        GeometryReader { proxy in
            let plot = proxy.size.width - axisWidth
            let layout = SessionChartLayout(bars: bars, axis: axis, width: plot)
            let scale = height / CGFloat(top)
            let points = bars.indices.compactMap { index -> (index: Int, point: CGPoint)? in
                bars[index].context.map { (index, CGPoint(x: layout.frame(index).center, y: height - min(CGFloat($0), CGFloat(top)) * scale)) }
            }
            ZStack(alignment: .topLeading) {
                Canvas { context, _ in
                    if window != nil {
                        let warning = theme.statusText(.warning)
                        context.fill(Path(CGRect(x: 0, y: 0, width: plot, height: height * 0.2)), with: .color(warning.opacity(0.1)))
                        context.stroke(Path { $0.move(to: .zero); $0.addLine(to: CGPoint(x: plot, y: 0)) }, with: .color(warning),
                                       style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                    }
                    context.stroke(Path { $0.move(to: CGPoint(x: 0, y: height / 2)); $0.addLine(to: CGPoint(x: plot, y: height / 2)) },
                                   with: .color(theme.divider.opacity(0.6)), lineWidth: 1)
                    context.stroke(Path { $0.move(to: CGPoint(x: 0, y: height)); $0.addLine(to: CGPoint(x: plot, y: height)) },
                                   with: .color(theme.divider), lineWidth: 1)
                    // A compaction starts a new stretch of the line; the area under each stretch is filled on its own.
                    var line = Path(), area = Path(), stretch: [CGPoint] = []
                    func close() {
                        guard let first = stretch.first, let last = stretch.last else { return }
                        area.move(to: CGPoint(x: first.x, y: height))
                        stretch.forEach { area.addLine(to: $0) }
                        area.addLine(to: CGPoint(x: last.x, y: height))
                        area.closeSubpath()
                        stretch = []
                    }
                    for (offset, entry) in points.enumerated() {
                        if offset == 0 || bars[entry.index].compacted {
                            close()
                            line.move(to: entry.point)
                        } else {
                            line.addLine(to: entry.point)
                        }
                        stretch.append(entry.point)
                    }
                    close()
                    let color = theme.kind(.cacheRead)
                    context.fill(area, with: .color(color.opacity(0.2)))
                    context.stroke(line, with: .color(color), style: StrokeStyle(lineWidth: 1.5, lineJoin: .round))
                    for entry in points where bars[entry.index].compacted {
                        context.stroke(Path { $0.move(to: CGPoint(x: entry.point.x, y: 8)); $0.addLine(to: CGPoint(x: entry.point.x, y: height)) },
                                       with: .color(theme.secondary), style: StrokeStyle(lineWidth: 1, dash: [2, 2]))
                    }
                    if let last = points.last {
                        let dot = CGRect(x: last.point.x - 3, y: last.point.y - 3, width: 6, height: 6)
                        context.fill(Path(ellipseIn: dot), with: .color(color))
                        context.stroke(Path(ellipseIn: dot), with: .color(theme.windowBackground), lineWidth: 1.5)
                    }
                }
                .frame(width: plot, height: height)
                ForEach(points.filter { bars[$0.index].compacted }, id: \.index) { entry in
                    Text(L10n.text("压缩", "Compacted")).font(.ui(9)).foregroundStyle(theme.secondary).fixedSize()
                        .position(x: min(entry.point.x + 22, plot - 20), y: 14)
                }
                Text(TokenFormat.short(top)).font(.tabular(9))
                    .foregroundStyle(window != nil ? theme.statusText(.warning) : theme.secondary)
                    .position(x: plot + 20, y: 3)
                Text(TokenFormat.short(top / 2)).font(.tabular(9)).foregroundStyle(theme.secondary).position(x: plot + 20, y: height / 2)
                Text("0").font(.tabular(9)).foregroundStyle(theme.secondary).position(x: plot + 20, y: height)
            }
        }
        .frame(height: height + 4)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(L10n.text("上下文窗口图表", "Context window chart"))
    }
}
