import Foundation

/// Formats durations the way the design shows them.
public enum Countdown {
    /// "2h 14m", "4h 02m", "51m". Never negative.
    public static func format(_ interval: TimeInterval) -> String {
        let total = max(0, Int(interval.rounded()))
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        if hours > 0 { return "\(hours)h \(String(format: "%02d", minutes))m" }
        return "\(minutes)m"
    }

    /// Menu-bar variant without the space: "2h14m", "51m".
    public static func compact(_ interval: TimeInterval) -> String {
        format(interval).replacingOccurrences(of: " ", with: "")
    }

    /// Time remaining until `date`, or "—" when unknown.
    public static func until(_ date: Date?, now: Date) -> String {
        guard let date else { return "—" }
        return format(date.timeIntervalSince(now))
    }

    /// Reset label: a countdown inside 24 hours ("3h 10m"), otherwise the weekday and time ("周日 02:00").
    public static func resetLabel(_ date: Date?, now: Date, calendar: Calendar = .current) -> String {
        guard let date else { return "—" }
        let interval = date.timeIntervalSince(now)
        if interval < 24 * 3600 { return format(interval) }
        return ChartData.weekdayTime(date, calendar: calendar)
    }

    /// Menu variant: "3h10m" or "周日 02:00".
    public static func resetLabelCompact(_ date: Date?, now: Date, calendar: Calendar = .current) -> String {
        guard let date else { return "—" }
        let interval = date.timeIntervalSince(now)
        if interval < 24 * 3600 { return compact(interval) }
        return ChartData.weekdayTime(date, calendar: calendar)
    }

    /// "刚刚更新", "2 分钟前更新", "3 小时前更新" / "Updated just now", "Updated 2 min ago", "Updated 3 h ago".
    public static func updatedLabel(since updatedAt: Date?, now: Date) -> String {
        guard let updatedAt else { return L10n.text("尚未更新", "Not updated yet") }
        let seconds = max(0, now.timeIntervalSince(updatedAt))
        if seconds < 60 { return L10n.text("刚刚更新", "Updated just now") }
        if seconds < 3600 {
            let minutes = Int(seconds / 60)
            return L10n.text("\(minutes) 分钟前更新", "Updated \(minutes) min ago")
        }
        let hours = Int(seconds / 3600)
        return L10n.text("\(hours) 小时前更新", "Updated \(hours) h ago")
    }

    /// Like `format` but drops a zero minute part: "2h" instead of "2h 00m".
    public static func formatRough(_ interval: TimeInterval) -> String {
        let total = max(0, Int(interval.rounded()))
        if total >= 3600, (total % 3600) / 60 == 0 { return "\(total / 3600)h" }
        return format(interval)
    }

    /// Session duration labels: "27m 进行中" / "27m running", "结束于 51m 前" / "ended 51m ago".
    public static func sessionLabel(_ session: LiveSession, now: Date) -> String {
        if session.isLive {
            let duration = format(session.duration(now: now))
            return L10n.text("\(duration) 进行中", "\(duration) running")
        }
        let ago = formatRough(now.timeIntervalSince(session.endedAt ?? now))
        return L10n.text("结束于 \(ago) 前", "ended \(ago) ago")
    }
}
