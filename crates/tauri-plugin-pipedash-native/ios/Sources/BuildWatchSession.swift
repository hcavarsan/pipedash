import Foundation
import PipedashActivityModels

struct StopBuildWatchArgs: Decodable {
  let sessionId: String
  let reason: BuildWatchMonitoring

  func validate() throws {
    guard !sessionId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
      sessionId.utf8.count <= 128,
      reason == .stopped || reason == .interrupted
    else { throw BuildWatchValidationError.invalid("The build watch stop request is invalid") }
  }
}

struct BuildWatchLaunchState {
  private(set) var pendingSessionId: String?

  mutating func submit(_ sessionId: String) throws {
    guard pendingSessionId == nil else {
      throw BuildWatchValidationError.invalid("iOS is finishing the previous build watch. Try again in a moment.")
    }
    pendingSessionId = sessionId
  }

  mutating func receive() -> String? {
    let sessionId = pendingSessionId
    pendingSessionId = nil
    return sessionId
  }

  mutating func submissionFailed(_ sessionId: String) {
    if pendingSessionId == sessionId { pendingSessionId = nil }
  }
}

struct BuildWatchTaskPresentation: Equatable {
  let title: String
  let subtitle: String

  init(_ snapshot: BuildWatchSnapshot) {
    title = "Pipedash · \(snapshot.pipelineName) #\(snapshot.runNumber)"
    switch snapshot.monitoring {
    case .watching:
      subtitle = "Watching · \(snapshot.status.title) · \(snapshot.providerName)"
    case .completed:
      subtitle = "Build \(snapshot.status.title.lowercased())"
    case .interrupted:
      subtitle = "Monitoring interrupted · build may still be running"
    case .stopped:
      subtitle = "Monitoring stopped · build may still be running"
    }
  }
}

final class BuildWatchSession {
  private static let snapshotKey = "pipedash.native.build-watch.snapshot"
  private static let reasonKey = "pipedash.native.build-watch.reason"

  private let defaults: UserDefaults
  private var cancelledSessions: [String] = []
  private(set) var watch: BuildWatchSnapshot?
  private(set) var reason: String?

  init(defaults: UserDefaults = .standard) {
    self.defaults = defaults
    if let data = defaults.data(forKey: Self.snapshotKey),
      let snapshot = try? JSONDecoder().decode(BuildWatchSnapshot.self, from: data),
      (try? snapshot.validate()) != nil
    {
      watch = snapshot
      reason = defaults.string(forKey: Self.reasonKey)
      if snapshot.monitoring == .watching {
        finish(
          sessionId: snapshot.sessionId, monitoring: .interrupted,
          reason: "Monitoring ended when Pipedash closed. Start watching again to resume.", now: snapshot.updatedAt)
      }
    } else {
      defaults.removeObject(forKey: Self.snapshotKey)
      defaults.removeObject(forKey: Self.reasonKey)
    }
  }

  func begin(_ snapshot: BuildWatchSnapshot) throws {
    try snapshot.validate()
    guard snapshot.monitoring == .watching, !snapshot.status.isTerminal else {
      throw BuildWatchValidationError.invalid("Only a queued or running build can be watched")
    }
    guard !cancelledSessions.contains(snapshot.sessionId) else {
      throw BuildWatchValidationError.invalid("This build watch was already stopped")
    }
    guard watch?.monitoring != .watching else {
      throw BuildWatchValidationError.invalid("Stop the current build watch before starting another")
    }
    guard watch?.sessionId != snapshot.sessionId else {
      throw BuildWatchValidationError.invalid("This build watch already ended. Start a new watch session.")
    }
    watch = snapshot
    reason = nil
    persist()
  }

  @discardableResult
  func update(_ snapshot: BuildWatchSnapshot) throws -> Bool {
    try snapshot.validate()
    guard let current = watch, current.monitoring == .watching,
      current.sessionId == snapshot.sessionId,
      current.pipelineId == snapshot.pipelineId, current.runNumber == snapshot.runNumber,
      current.providerType == snapshot.providerType, current.url == snapshot.url,
      snapshot.updatedAt >= current.updatedAt
    else { return false }
    watch = snapshot
    reason = nil
    persist()
    return true
  }

  @discardableResult
  func finish(
    sessionId: String, monitoring: BuildWatchMonitoring, reason: String?,
    now: Int64 = Int64(Date().timeIntervalSince1970)
  ) -> Bool {
    guard monitoring == .stopped || monitoring == .interrupted else { return false }
    if !cancelledSessions.contains(sessionId) {
      cancelledSessions.append(sessionId)
      if cancelledSessions.count > 32 { cancelledSessions.removeFirst() }
    }
    guard let current = watch, current.sessionId == sessionId,
      current.monitoring == .watching
    else { return false }
    watch = current.ending(monitoring, at: now)
    self.reason = reason
    persist()
    return true
  }

  private func persist() {
    if let watch = watch, let data = try? JSONEncoder().encode(watch) {
      defaults.set(data, forKey: Self.snapshotKey)
    }
    defaults.set(reason, forKey: Self.reasonKey)
  }
}
