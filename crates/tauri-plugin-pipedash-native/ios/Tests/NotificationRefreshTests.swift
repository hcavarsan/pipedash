import Foundation
import UIKit
import UserNotifications
import XCTest

@testable import tauri_plugin_pipedash_native

final class NotificationRefreshTests: XCTestCase {
  private var defaults: UserDefaults!
  private var suite: String!

  override func setUp() {
    super.setUp()
    suite = "pipedash-notification-refresh-tests-\(UUID().uuidString)"
    defaults = UserDefaults(suiteName: suite)!
  }

  override func tearDown() {
    defaults.removePersistentDomain(forName: suite)
    defaults = nil
    suite = nil
    super.tearDown()
  }

  func testNotificationsAreOffByDefaultAndTheChoiceSurvivesARelaunch() {
    XCTAssertFalse(NotificationRefreshPreference(defaults: defaults).enabled)
    NotificationRefreshPreference(defaults: defaults).enabled = true
    XCTAssertTrue(NotificationRefreshPreference(defaults: defaults).enabled)
    NotificationRefreshPreference(defaults: defaults).enabled = false
    XCTAssertFalse(NotificationRefreshPreference(defaults: defaults).enabled)
  }

  func testColdLaunchWaitsForTheRustListenerAndDeliversOnce() {
    var state = NotificationRefreshState()
    XCTAssertEqual(state.receive("task-1"), .queued)
    XCTAssertEqual(state.pendingId, "task-1")
    XCTAssertNil(state.activeId)
    XCTAssertEqual(state.listenerReady(), "task-1")
    XCTAssertEqual(state.activeId, "task-1")
    XCTAssertNil(state.pendingId)
    XCTAssertNil(state.listenerReady())
    XCTAssertEqual(state.activeId, "task-1")
  }

  func testLaunchAfterTheListenerIsReadyDeliversImmediately() {
    var state = NotificationRefreshState()
    XCTAssertNil(state.listenerReady())
    XCTAssertEqual(state.receive("task-1"), .deliver)
    XCTAssertEqual(state.activeId, "task-1")
    XCTAssertNil(state.pendingId)
  }

  func testOnlyOneTaskCanBeQueuedOrActive() {
    var queued = NotificationRefreshState()
    XCTAssertEqual(queued.receive("task-1"), .queued)
    XCTAssertEqual(queued.receive("task-2"), .reject)
    XCTAssertEqual(queued.pendingId, "task-1")

    var active = NotificationRefreshState()
    _ = active.listenerReady()
    XCTAssertEqual(active.receive("task-1"), .deliver)
    XCTAssertEqual(active.receive("task-2"), .reject)
    XCTAssertEqual(active.activeId, "task-1")
    XCTAssertTrue(active.finish("task-1"))
    XCTAssertEqual(active.receive("task-2"), .deliver)
  }

  func testCompletionIsAcceptedOnceAndOnlyForTheActiveTask() {
    var state = NotificationRefreshState()
    _ = state.listenerReady()
    XCTAssertEqual(state.receive("task-1"), .deliver)
    XCTAssertFalse(state.finish("other"))
    XCTAssertEqual(state.activeId, "task-1")
    XCTAssertTrue(state.finish("task-1"))
    XCTAssertFalse(state.finish("task-1"))
  }

  func testExpirationReleasesTheTaskAndLateCompletionCannotTouchTheNextOne() {
    var state = NotificationRefreshState()
    _ = state.listenerReady()
    XCTAssertEqual(state.receive("task-1"), .deliver)
    XCTAssertEqual(state.expire("task-1"), .delivered)
    XCTAssertNil(state.activeId)
    XCTAssertEqual(state.receive("task-2"), .deliver)
    XCTAssertFalse(state.finish("task-1"))
    XCTAssertEqual(state.activeId, "task-2")
    XCTAssertEqual(state.expire("task-1"), .unknown)
    XCTAssertEqual(state.activeId, "task-2")
    XCTAssertTrue(state.finish("task-2"))
  }

  func testExpiringAQueuedTaskNeverReachesRustAfterTheListenerIsReady() {
    var state = NotificationRefreshState()
    XCTAssertEqual(state.receive("task-1"), .queued)
    XCTAssertEqual(state.expire("task-1"), .queued)
    XCTAssertNil(state.listenerReady())
    XCTAssertNil(state.activeId)
    XCTAssertEqual(state.receive("task-2"), .deliver)
  }

  func testDisablingReportsOnlyTheTaskRustAlreadyHolds() {
    var delivered = NotificationRefreshState()
    _ = delivered.listenerReady()
    XCTAssertEqual(delivered.receive("task-1"), .deliver)
    XCTAssertEqual(delivered.cancelAll(), "task-1")
    XCTAssertFalse(delivered.finish("task-1"))
    XCTAssertNil(delivered.cancelAll())

    var queued = NotificationRefreshState()
    XCTAssertEqual(queued.receive("task-1"), .queued)
    XCTAssertNil(queued.cancelAll())
    XCTAssertNil(queued.listenerReady())
    XCTAssertNil(queued.activeId)
  }

  func testAvailabilityRequiresNotificationPermissionAndBackgroundRefresh() {
    func blocker(
      _ notifications: UNAuthorizationStatus, _ refresh: UIBackgroundRefreshStatus,
      registered: Bool = true
    ) -> String? {
      NotificationRefreshAvailability.blocker(
        notifications: notifications, backgroundRefresh: refresh, registered: registered)
    }
    XCTAssertNil(blocker(.authorized, .available))
    XCTAssertNil(blocker(.provisional, .available))
    XCTAssertNotNil(blocker(.notDetermined, .available))
    XCTAssertNotNil(blocker(.denied, .available))
    XCTAssertNotNil(blocker(.authorized, .denied))
    XCTAssertNotNil(blocker(.authorized, .restricted))
    XCTAssertNotEqual(blocker(.authorized, .denied), blocker(.authorized, .restricted))
    XCTAssertNotNil(blocker(.authorized, .available, registered: false))
    XCTAssertEqual(blocker(.denied, .denied), blocker(.denied, .available))
  }

  func testLowPowerModeIsReportedWithoutBlockingSchedulingOrConsent() {
    XCTAssertNil(
      NotificationRefreshAvailability.blocker(
        notifications: .authorized, backgroundRefresh: .available, registered: true))
    XCTAssertNotNil(NotificationRefreshAvailability.advisory(lowPowerMode: true, scheduleError: nil))
    XCTAssertEqual(
      NotificationRefreshAvailability.advisory(lowPowerMode: true, scheduleError: "x"),
      NotificationRefreshAvailability.advisory(lowPowerMode: true, scheduleError: nil))
    XCTAssertEqual(
      NotificationRefreshAvailability.advisory(lowPowerMode: false, scheduleError: "x"), "x")
    XCTAssertNil(NotificationRefreshAvailability.advisory(lowPowerMode: false, scheduleError: nil))
  }

  func testForegroundResumeExpiresADeliveredTaskOnceAndKeepsTheListenerReady() {
    var state = NotificationRefreshState()
    _ = state.listenerReady()
    XCTAssertEqual(state.receive("task-1"), .deliver)
    XCTAssertEqual(state.cancelAll(), "task-1")
    XCTAssertNil(state.cancelAll())
    XCTAssertTrue(state.ready)
    XCTAssertFalse(state.finish("task-1"))
    XCTAssertEqual(state.receive("task-2"), .deliver)
  }

  func testCompletionArgumentsRejectBlankAndOversizedIdentifiers() throws {
    XCTAssertNoThrow(try FinishNotificationRefreshArgs(id: "task", success: true).validate())
    XCTAssertThrowsError(try FinishNotificationRefreshArgs(id: "", success: true).validate())
    XCTAssertThrowsError(try FinishNotificationRefreshArgs(id: "  ", success: true).validate())
    XCTAssertThrowsError(
      try FinishNotificationRefreshArgs(id: String(repeating: "a", count: 129), success: true)
        .validate())
    let decoded = try JSONDecoder().decode(
      FinishNotificationRefreshArgs.self, from: Data(#"{"id":"task","success":false}"#.utf8))
    XCTAssertEqual(decoded.id, "task")
    XCTAssertFalse(decoded.success)
    XCTAssertTrue(
      try JSONDecoder().decode(
        ConfigureNotificationsArgs.self, from: Data(#"{"enabled":true}"#.utf8)
      ).enabled)
  }

  func testPayloadsUseTheKeysTheRustPluginReads() throws {
    let event = try XCTUnwrap(
      JSONSerialization.jsonObject(
        with: JSONEncoder().encode(NotificationRefreshEvent(id: "task", expired: true)))
        as? [String: Any])
    XCTAssertEqual(Set(event.keys), ["id", "expired"])
    XCTAssertEqual(event["id"] as? String, "task")
    XCTAssertEqual(event["expired"] as? Bool, true)
    let settings = try XCTUnwrap(
      JSONSerialization.jsonObject(
        with: JSONEncoder().encode(
          LocalNotificationSettings(enabled: true, available: false, reason: "Off")))
        as? [String: Any])
    XCTAssertEqual(Set(settings.keys), ["enabled", "available", "reason"])
    XCTAssertEqual(settings["enabled"] as? Bool, true)
    XCTAssertEqual(settings["available"] as? Bool, false)
    XCTAssertEqual(settings["reason"] as? String, "Off")
  }

  func testTheTaskIdentifierMatchesTheBundleDeclaration() {
    XCTAssertEqual(
      NotificationRefreshController.taskIdentifier, "com.henrique.pipedash.notifications-refresh")
    XCTAssertGreaterThan(NotificationRefreshController.refreshInterval, 0)
  }
}
