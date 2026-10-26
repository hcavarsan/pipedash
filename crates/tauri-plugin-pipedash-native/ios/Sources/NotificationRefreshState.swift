import Foundation
import UIKit
import UserNotifications

enum NotificationRefreshError: LocalizedError {
  case invalid(String)

  var errorDescription: String? {
    switch self {
    case .invalid(let message): return message
    }
  }
}

struct LocalNotificationSettings: Encodable, Equatable {
  let enabled: Bool
  let available: Bool
  let reason: String?
}

struct NotificationRefreshEvent: Encodable, Equatable {
  let id: String
  let expired: Bool
}

struct ConfigureNotificationsArgs: Decodable {
  let enabled: Bool
}

struct FinishNotificationRefreshArgs: Decodable {
  let id: String
  let success: Bool

  func validate() throws {
    guard !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, id.utf8.count <= 128 else {
      throw NotificationRefreshError.invalid("The notification refresh is invalid")
    }
  }
}

struct NotificationRefreshPreference {
  static let key = "pipedash.native.notifications.enabled"

  private let defaults: UserDefaults

  init(defaults: UserDefaults = .standard) {
    self.defaults = defaults
  }

  var enabled: Bool {
    get { defaults.bool(forKey: Self.key) }
    nonmutating set { defaults.set(newValue, forKey: Self.key) }
  }
}

enum NotificationRefreshAvailability {
  static func notificationsAllowed(_ status: UNAuthorizationStatus) -> Bool {
    status == .authorized || status == .provisional
  }

  static func blocker(
    notifications: UNAuthorizationStatus, backgroundRefresh: UIBackgroundRefreshStatus,
    registered: Bool
  ) -> String? {
    guard notificationsAllowed(notifications) else {
      return "Allow notifications for Pipedash in Settings to receive alerts."
    }
    switch backgroundRefresh {
    case .available:
      break
    case .restricted:
      return "Background App Refresh is restricted on this device."
    default:
      return "Turn on Background App Refresh for Pipedash in Settings."
    }
    return registered ? nil : "iOS could not register background refresh."
  }

  static func advisory(lowPowerMode: Bool, scheduleError: String?) -> String? {
    lowPowerMode ? "Low Power Mode pauses background refresh while your device is locked." : scheduleError
  }
}

struct NotificationRefreshState {
  enum Launch: Equatable {
    case deliver, queued, reject
  }

  enum Expiry: Equatable {
    case unknown, queued, delivered
  }

  private(set) var ready = false
  private(set) var pendingId: String?
  private(set) var activeId: String?

  mutating func receive(_ id: String) -> Launch {
    guard pendingId == nil, activeId == nil else { return .reject }
    guard ready else {
      pendingId = id
      return .queued
    }
    activeId = id
    return .deliver
  }

  mutating func listenerReady() -> String? {
    ready = true
    guard let id = pendingId else { return nil }
    pendingId = nil
    activeId = id
    return id
  }

  mutating func expire(_ id: String) -> Expiry {
    if activeId == id {
      activeId = nil
      return .delivered
    }
    if pendingId == id {
      pendingId = nil
      return .queued
    }
    return .unknown
  }

  mutating func finish(_ id: String) -> Bool {
    guard activeId == id else { return false }
    activeId = nil
    return true
  }

  mutating func cancelAll() -> String? {
    pendingId = nil
    let delivered = activeId
    activeId = nil
    return delivered
  }
}
