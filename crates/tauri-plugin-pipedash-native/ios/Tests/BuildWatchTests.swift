import Foundation
import PipedashActivityModels
import XCTest

@testable import tauri_plugin_pipedash_native

final class BuildWatchTests: XCTestCase {
  private var defaults: UserDefaults!
  private var suite: String!

  override func setUp() {
    super.setUp()
    suite = "pipedash-build-watch-tests-\(UUID().uuidString)"
    defaults = UserDefaults(suiteName: suite)!
  }

  override func tearDown() {
    defaults.removePersistentDomain(forName: suite)
    defaults = nil
    suite = nil
    super.tearDown()
  }

  private func snapshot(_ sessionId: String = "session-1") -> BuildWatchSnapshot {
    BuildWatchSnapshot(
      sessionId: sessionId, pipelineId: "provider:app/main", pipelineName: "Ship app",
      providerName: "Work CI", providerType: "github", runNumber: 42,
      branch: "main", status: .running, startedAt: 100, concludedAt: nil,
      updatedAt: 130, monitoring: .watching,
      url: "pipedash://pipelines/provider%3Aapp%2Fmain/runs/42")
  }

  func testDisabledWatchRejectsStartsAndUpdates() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: false)
    XCTAssertThrowsError(try session.begin(snapshot()))
    XCTAssertFalse(try session.update(snapshot()))
    XCTAssertNil(session.watch)
  }

  func testDisablingWatchInterruptsPersistedSessionAndReenablingAllowsANewWatch() throws {
    let enabled = BuildWatchSession(defaults: defaults, enabled: true)
    try enabled.begin(snapshot())
    let disabled = BuildWatchSession(defaults: defaults, enabled: false)
    XCTAssertEqual(disabled.watch?.monitoring, .interrupted)
    XCTAssertEqual(disabled.watch?.status, .running)
    XCTAssertThrowsError(try disabled.begin(snapshot("session-2")))
    XCTAssertFalse(try disabled.update(snapshot()))
    let reenabled = BuildWatchSession(defaults: defaults, enabled: true)
    XCTAssertEqual(reenabled.watch?.monitoring, .interrupted)
    try reenabled.begin(snapshot("session-2"))
    XCTAssertEqual(reenabled.watch?.sessionId, "session-2")
    XCTAssertEqual(reenabled.watch?.monitoring, .watching)
  }

  func testOnlyOneSessionCanWatchAtATime() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    XCTAssertThrowsError(try session.begin(snapshot("session-2")))
    XCTAssertEqual(session.watch?.sessionId, "session-1")
    XCTAssertEqual(session.watch?.monitoring, .watching)
  }

  func testOldSessionOlderTimestampAndChangedRunCannotReplaceTheWatch() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    XCTAssertFalse(try session.update(snapshot("old-session")))
    var old = snapshot()
    old.updatedAt = 129
    XCTAssertFalse(try session.update(old))
    var wrongRun = snapshot()
    wrongRun.runNumber = 43
    wrongRun.url = "pipedash://pipelines/provider%3Aapp%2Fmain/runs/43"
    XCTAssertFalse(try session.update(wrongRun))
    XCTAssertEqual(session.watch, snapshot())
  }

  func testUpdatedProviderLabelsAndBranchAreAccepted() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    var update = snapshot()
    update.pipelineName = "Release app"
    update.providerName = "Renamed CI"
    update.branch = "release/2.0"
    update.updatedAt = 140
    XCTAssertTrue(try session.update(update))
    XCTAssertEqual(session.watch, update)
  }

  func testStopBeforeStartTombstonesOnlyThatSession() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    XCTAssertFalse(session.finish(sessionId: "session-1", monitoring: .stopped, reason: nil, now: 131))
    XCTAssertThrowsError(try session.begin(snapshot()))
    try session.begin(snapshot("session-2"))
    XCTAssertFalse(session.finish(sessionId: "session-1", monitoring: .stopped, reason: nil, now: 132))
    XCTAssertEqual(session.watch?.monitoring, .watching)
  }

  func testExpirationOrSchedulerFailurePersistsInterruptionAndRejectsLateUpdates() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    XCTAssertTrue(session.finish(
      sessionId: "session-1", monitoring: .interrupted, reason: "iOS ended monitoring", now: 160))
    var late = snapshot()
    late.updatedAt = 170
    XCTAssertFalse(try session.update(late))
    let restored = BuildWatchSession(defaults: defaults, enabled: true)
    XCTAssertEqual(restored.watch?.monitoring, .interrupted)
    XCTAssertEqual(restored.watch?.status, .running)
    XCTAssertEqual(restored.watch?.elapsedSeconds, 60)
    XCTAssertEqual(restored.reason, "iOS ended monitoring")
  }

  func testStopWatchingDoesNotChangeTheProviderStatus() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    XCTAssertTrue(session.finish(sessionId: "session-1", monitoring: .stopped, reason: nil, now: 150))
    XCTAssertEqual(session.watch?.monitoring, .stopped)
    XCTAssertEqual(session.watch?.status, .running)
    XCTAssertNil(session.watch?.concludedAt)
    XCTAssertEqual(session.watch?.elapsedLabel, "0:50")
    try session.begin(snapshot("session-2"))
    XCTAssertEqual(session.watch?.sessionId, "session-2")
  }

  func testForceQuitRecoveryNeverRestartsWatching() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    let restored = BuildWatchSession(defaults: defaults, enabled: true)
    XCTAssertEqual(restored.watch?.monitoring, .interrupted)
    XCTAssertEqual(restored.watch?.updatedAt, 130)
    XCTAssertEqual(restored.watch?.elapsedSeconds, 30)
    XCTAssertEqual(restored.watch?.status, .running)
    XCTAssertNotNil(restored.reason)
    XCTAssertFalse(try restored.update(snapshot()))
    XCTAssertEqual(BuildWatchSession(defaults: defaults, enabled: true).watch?.updatedAt, 130)
  }

  func testTerminalResultFreezesAtProviderConclusionAndCannotBeOverwritten() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    try session.begin(snapshot())
    var result = snapshot()
    result.status = .failed
    result.monitoring = .completed
    result.concludedAt = 180
    result.updatedAt = 190
    XCTAssertTrue(try session.update(result))
    XCTAssertEqual(session.watch?.elapsedSeconds, 80)
    XCTAssertFalse(session.finish(sessionId: "session-1", monitoring: .interrupted, reason: nil, now: 300))
    XCTAssertFalse(try session.update(snapshot()))
    XCTAssertEqual(BuildWatchSession(defaults: defaults, enabled: true).watch, result)
    XCTAssertThrowsError(try session.begin(snapshot()))
  }

  func testTerminalResultWithoutConclusionFreezesAtLastUpdate() throws {
    var result = snapshot()
    result.status = .success
    result.monitoring = .completed
    result.updatedAt = 200
    XCTAssertNoThrow(try result.validate())
    XCTAssertEqual(result.elapsedLabel, "1:40")
    XCTAssertThrowsError(try BuildWatchSession(defaults: defaults, enabled: true).begin(result))
  }

  func testActiveRunIgnoresProviderUpdateAsConclusion() {
    var running = snapshot()
    running.concludedAt = 110
    running.updatedAt = 180
    XCTAssertEqual(running.elapsedSeconds, 80)
  }

  func testInterruptedActiveRunFreezesAtObservationTime() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    var running = snapshot()
    running.concludedAt = 110
    try session.begin(running)
    XCTAssertTrue(session.finish(sessionId: "session-1", monitoring: .interrupted, reason: nil, now: 200))
    XCTAssertEqual(session.watch?.status, .running)
    XCTAssertEqual(session.watch?.elapsedSeconds, 100)
  }

  func testSnapshotRejectsExternalLinksUnsafeRunNumbersAndOversizedContent() throws {
    var invalid = snapshot()
    invalid.url = "https://example.com/runs/42?token=secret"
    XCTAssertThrowsError(try invalid.validate())
    invalid = snapshot()
    invalid.runNumber = 9_007_199_254_740_992
    XCTAssertThrowsError(try invalid.validate())
    invalid = snapshot()
    invalid.pipelineName = String(repeating: "a", count: 512)
    invalid.providerName = String(repeating: "b", count: 512)
    invalid.sessionId = String(repeating: "s", count: 128)
    invalid.providerType = String(repeating: "p", count: 128)
    invalid.branch = String(repeating: "/", count: 1024)
    XCTAssertThrowsError(try invalid.validate())
    invalid = snapshot()
    invalid.status = .success
    XCTAssertThrowsError(try invalid.validate())
  }

  func testEncodedContractKeepsOptionalNullsAndContainsNoCredentials() throws {
    var value = snapshot()
    value.branch = nil
    let encoded = try JSONEncoder().encode(value)
    let object = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    XCTAssertTrue(object["branch"] is NSNull)
    XCTAssertTrue(object["concludedAt"] is NSNull)
    XCTAssertNil(object["token"])
    XCTAssertNil(object["credentials"])
    let status = BuildWatchStatus(supported: true, available: true, reason: nil, watch: nil)
    let statusObject = try XCTUnwrap(
      JSONSerialization.jsonObject(with: JSONEncoder().encode(status)) as? [String: Any])
    XCTAssertTrue(statusObject["reason"] is NSNull)
    XCTAssertTrue(statusObject["watch"] is NSNull)
  }

  func testStopBeforeLaunchBlocksReplacementUntilTheOldHandlerArrives() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    var launch = BuildWatchLaunchState()
    try session.begin(snapshot())
    try launch.submit("session-1")
    XCTAssertTrue(session.finish(sessionId: "session-1", monitoring: .stopped, reason: nil, now: 140))
    XCTAssertThrowsError(try launch.submit("session-2"))
    XCTAssertEqual(launch.pendingSessionId, "session-1")
    XCTAssertEqual(launch.receive(), "session-1")
    XCTAssertEqual(session.watch?.monitoring, .stopped)
    XCTAssertNil(launch.pendingSessionId)
    try session.begin(snapshot("session-2"))
    try launch.submit("session-2")
    XCTAssertEqual(launch.receive(), "session-2")
    XCTAssertFalse(session.finish(
      sessionId: "session-1", monitoring: .interrupted, reason: "Late expiration", now: 200))
    XCTAssertEqual(session.watch?.sessionId, "session-2")
    XCTAssertEqual(session.watch?.monitoring, .watching)
    XCTAssertNil(launch.receive())
  }

  func testFailedSubmissionReleasesOnlyItsOwnPendingLaunch() throws {
    var launch = BuildWatchLaunchState()
    try launch.submit("session-1")
    launch.submissionFailed("other-session")
    XCTAssertEqual(launch.pendingSessionId, "session-1")
    launch.submissionFailed("session-1")
    XCTAssertNil(launch.pendingSessionId)
    try launch.submit("session-2")
    launch.submissionFailed("session-1")
    XCTAssertEqual(launch.receive(), "session-2")
  }

  func testTerminalResultBeforeLaunchKeepsTheRealResultForTheHandler() throws {
    let session = BuildWatchSession(defaults: defaults, enabled: true)
    var launch = BuildWatchLaunchState()
    try session.begin(snapshot())
    try launch.submit("session-1")
    var terminal = snapshot()
    terminal.status = .success
    terminal.monitoring = .completed
    terminal.updatedAt = 150
    XCTAssertTrue(try session.update(terminal))
    XCTAssertEqual(launch.receive(), terminal.sessionId)
    XCTAssertEqual(session.watch?.monitoring, .completed)
    XCTAssertEqual(session.watch?.status, .success)
    XCTAssertEqual(session.watch?.elapsedSeconds, 50)
  }

  func testTaskPresentationDoesNotChangeForTimestampOrNonDisplayedMetadata() {
    let original = BuildWatchTaskPresentation(snapshot())
    var refreshed = snapshot()
    refreshed.updatedAt = 160
    refreshed.branch = "release/2.0"
    refreshed.startedAt = 110
    XCTAssertEqual(BuildWatchTaskPresentation(refreshed), original)
    XCTAssertEqual(original.title, "Pipedash · Ship app #42")
    XCTAssertEqual(original.subtitle, "Watching · Running · Work CI")
  }

  func testTaskPresentationChangesForVisibleLabelsAndLifecycle() {
    let original = BuildWatchTaskPresentation(snapshot())
    var changed = snapshot()
    changed.pipelineName = "Release app"
    XCTAssertNotEqual(BuildWatchTaskPresentation(changed), original)
    changed = snapshot()
    changed.providerName = "Renamed CI"
    XCTAssertNotEqual(BuildWatchTaskPresentation(changed), original)
    changed = snapshot()
    changed.status = .pending
    XCTAssertNotEqual(BuildWatchTaskPresentation(changed), original)
    changed = snapshot()
    changed.status = .failed
    changed.monitoring = .completed
    XCTAssertEqual(BuildWatchTaskPresentation(changed).subtitle, "Build failed")
    changed.monitoring = .interrupted
    XCTAssertEqual(BuildWatchTaskPresentation(changed).subtitle, "Monitoring interrupted · build may still be running")
    changed.monitoring = .stopped
    XCTAssertEqual(BuildWatchTaskPresentation(changed).subtitle, "Monitoring stopped · build may still be running")
  }

  func testStopRejectsUnexpectedReasons() {
    XCTAssertThrowsError(try StopBuildWatchArgs(sessionId: "session", reason: .completed).validate())
    XCTAssertThrowsError(try StopBuildWatchArgs(sessionId: "", reason: .stopped).validate())
    XCTAssertThrowsError(try StopBuildWatchArgs(sessionId: "  ", reason: .stopped).validate())
  }
}
