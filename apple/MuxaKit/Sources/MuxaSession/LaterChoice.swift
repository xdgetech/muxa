import Foundation

/// The times Later offers. Each is in the future, as the host requires.
public enum LaterChoice: CaseIterable, Sendable {
    case inAnHour, tomorrowMorning

    public var title: String {
        switch self {
        case .inAnHour: "In 1 hour"
        case .tomorrowMorning: "Tomorrow 9:00"
        }
    }

    /// Epoch seconds. Tomorrow is the next day in `calendar`'s time zone.
    public func until(now: Date = Date(), calendar: Calendar = .current) -> UInt64 {
        switch self {
        case .inAnHour:
            return UInt64(now.timeIntervalSince1970) + 3600
        case .tomorrowMorning:
            let today = calendar.startOfDay(for: now)
            let tomorrow = calendar.date(byAdding: .day, value: 1, to: today) ?? today.addingTimeInterval(86_400)
            let morning = calendar.date(bySettingHour: 9, minute: 0, second: 0, of: tomorrow) ?? tomorrow
            return UInt64(morning.timeIntervalSince1970)
        }
    }
}

/// The host refuses a Later time in the past or more than 366 days ahead.
public func laterProblem(_ date: Date, now: Date = Date()) -> String? {
    if date <= now { return "Pick a time in the future." }
    if date.timeIntervalSince(now) > 366 * 86_400 { return "Pick a time within a year." }
    return nil
}
