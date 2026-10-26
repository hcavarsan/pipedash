import BackgroundTasks
import Foundation
import UIKit
import UserNotifications

final class NotificationRefreshController {
  static let shared = NotificationRefreshController()
  static let taskIdentifier = "com.henrique.pipedash.notifications-refresh"
  static let refreshInterval: TimeInterval = 15 * 60

  var deliver: ((NotificationRefreshEvent) -> Void)?

  private let preference = NotificationRefreshPreference()
  private var state = NotificationRefreshState()
  private var tasks: [String: BGAppRefreshTask] = [:]
  private var registered = false
  private var started = false
  private var scheduleError: String?

  private init() {
    registered = BGTaskScheduler.shared.register(
      forTaskWithIdentifier: Self.taskIdentifier, using: .main
    ) { [weak self] task in
      guard let self = self else {
        task.setTaskCompleted(success: false)
        return
      }
      self.accept(task)
    }
  }

  func start() {
    guard !started else { return }
    started = true
    if !preference.enabled {
      cancelRequests()
    }
    let center = NotificationCenter.default
    center.addObserver(
      forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
    ) { [weak self] _ in
      self?.releaseTasks()
      self?.reconcile(replacing: false)
    }
    center.addObserver(
      forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main
    ) { [weak self] _ in
      self?.reconcile(replacing: true)
    }
    reconcile(replacing: false)
  }

  func settings(_ completion: @escaping (LocalNotificationSettings) -> Void) {
    authorization { status in
      completion(self.makeSettings(status))
    }
  }

  func configure(
    enabled: Bool, completion: @escaping (Result<LocalNotificationSettings, Error>) -> Void
  ) {
    guard enabled else {
      disable()
      settings { completion(.success($0)) }
      return
    }
    authorization { status in
      guard NotificationRefreshAvailability.notificationsAllowed(status) else {
        completion(
          .failure(
            NotificationRefreshError.invalid(
              "Allow notifications for Pipedash in Settings to turn on alerts.")))
        return
      }
      self.preference.enabled = true
      guard self.blocker(status) == nil else {
        completion(.success(self.makeSettings(status)))
        return
      }
      self.schedule(replacing: true) {
        completion(.success(self.makeSettings(status)))
      }
    }
  }

  func listenerReady() {
    if let id = state.listenerReady() {
      deliver?(NotificationRefreshEvent(id: id, expired: false))
    }
  }

  func finish(_ id: String, success: Bool) {
    guard state.finish(id) else { return }
    complete(id, success: success)
  }

  private func authorization(_ completion: @escaping (UNAuthorizationStatus) -> Void) {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      DispatchQueue.main.async { completion(settings.authorizationStatus) }
    }
  }

  private func blocker(_ status: UNAuthorizationStatus) -> String? {
    NotificationRefreshAvailability.blocker(
      notifications: status, backgroundRefresh: UIApplication.shared.backgroundRefreshStatus,
      registered: registered)
  }

  private func makeSettings(_ status: UNAuthorizationStatus) -> LocalNotificationSettings {
    let enabled = preference.enabled
    let reason =
      blocker(status)
      ?? (enabled
        ? NotificationRefreshAvailability.advisory(
          lowPowerMode: ProcessInfo.processInfo.isLowPowerModeEnabled, scheduleError: scheduleError)
        : nil)
    return LocalNotificationSettings(enabled: enabled, available: reason == nil, reason: reason)
  }

  private func disable() {
    preference.enabled = false
    scheduleError = nil
    cancelRequests()
    releaseTasks()
  }

  private func releaseTasks() {
    let delivered = state.cancelAll()
    for id in Array(tasks.keys) {
      complete(id, success: false)
    }
    if let id = delivered {
      deliver?(NotificationRefreshEvent(id: id, expired: true))
    }
  }

  private func accept(_ task: BGTask) {
    guard let refresh = task as? BGAppRefreshTask else {
      task.setTaskCompleted(success: false)
      return
    }
    let id = UUID().uuidString
    tasks[id] = refresh
    refresh.expirationHandler = { [weak self] in
      DispatchQueue.main.async { self?.expire(id) }
    }
    guard preference.enabled else {
      complete(id, success: false)
      cancelRequests()
      return
    }
    authorization { status in
      guard self.tasks[id] != nil else { return }
      guard UIApplication.shared.applicationState != .active else {
        self.complete(id, success: false)
        return
      }
      guard self.preference.enabled, self.blocker(status) == nil else {
        self.complete(id, success: false)
        self.cancelRequests()
        return
      }
      self.schedule(replacing: true)
      switch self.state.receive(id) {
      case .reject:
        self.complete(id, success: false)
      case .queued:
        break
      case .deliver:
        self.deliver?(NotificationRefreshEvent(id: id, expired: false))
      }
    }
  }

  private func expire(_ id: String) {
    let expiry = state.expire(id)
    complete(id, success: false)
    if expiry == .delivered {
      deliver?(NotificationRefreshEvent(id: id, expired: true))
    }
  }

  private func complete(_ id: String, success: Bool) {
    guard let task = tasks.removeValue(forKey: id) else { return }
    task.expirationHandler = nil
    task.setTaskCompleted(success: success)
  }

  private func reconcile(replacing: Bool) {
    guard preference.enabled else { return }
    authorization { status in
      guard self.preference.enabled else { return }
      if self.blocker(status) == nil {
        self.schedule(replacing: replacing)
      } else {
        self.cancelRequests()
      }
    }
  }

  private func cancelRequests() {
    BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: Self.taskIdentifier)
  }

  private func schedule(replacing: Bool, completion: (() -> Void)? = nil) {
    BGTaskScheduler.shared.getPendingTaskRequests { requests in
      DispatchQueue.main.async {
        defer { completion?() }
        guard self.preference.enabled else { return }
        if !replacing, requests.contains(where: { $0.identifier == Self.taskIdentifier }) {
          return
        }
        let request = BGAppRefreshTaskRequest(identifier: Self.taskIdentifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: Self.refreshInterval)
        do {
          try BGTaskScheduler.shared.submit(request)
          self.scheduleError = nil
        } catch {
          self.scheduleError =
            "iOS could not schedule background refresh: \(error.localizedDescription)"
        }
      }
    }
  }
}
