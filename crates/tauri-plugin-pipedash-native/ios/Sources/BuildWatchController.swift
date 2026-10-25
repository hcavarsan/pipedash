import Foundation
import PipedashActivityModels
import UIKit

func unsupportedBuildWatchStatus() -> BuildWatchStatus {
  #if targetEnvironment(simulator) || targetEnvironment(macCatalyst)
  let reason = "Build watching requires a physical iPhone or iPad running iOS 26 or later."
  #elseif compiler(<6.2)
  let reason = "This Pipedash build does not include the iOS 26 background task APIs."
  #else
  let reason = "Build watching requires a physical iPhone or iPad running iOS 26 or later."
  #endif
  return BuildWatchStatus(supported: false, available: false, reason: reason, watch: nil)
}

#if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
import ActivityKit
import BackgroundTasks

@available(iOS 26.0, *)
@MainActor
final class BuildWatchController {
  static let shared = BuildWatchController()
  static let taskIdentifier = "com.henrique.pipedash.build-watch"

  private let session = BuildWatchSession()
  private var launch = BuildWatchLaunchState()
  private var registered = false
  private var backgroundTask: BGContinuedProcessingTask?
  private var activity: Activity<PipedashBuildActivityAttributes>?
  private var activityObserver: Task<Void, Never>?
  private var activityOperation: Task<Void, Never>?
  private var foregroundObserver: NSObjectProtocol?
  private var permissionObserver: Task<Void, Never>?
  var onChange: (() -> Void)?

  private init() {
    registered = BGTaskScheduler.shared.register(
      forTaskWithIdentifier: Self.taskIdentifier, using: .main
    ) { [weak self] task in
      guard let continued = task as? BGContinuedProcessingTask else {
        task.setTaskCompleted(success: false)
        return
      }
      MainActor.assumeIsolated {
        guard let self = self else {
          continued.setTaskCompleted(success: false)
          return
        }
        self.accept(continued)
      }
    }
    let recovered = Activity<PipedashBuildActivityAttributes>.activities
    for orphan in recovered {
      let previous = orphan.content.state
      let final = previous.monitoring == .watching
        ? previous.ending(.interrupted, at: previous.updatedAt) : previous
      Task {
        await orphan.end(
          content(final), dismissalPolicy: .after(Date().addingTimeInterval(60)))
      }
    }
    foregroundObserver = NotificationCenter.default.addObserver(
      forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
    ) { [weak self] _ in
      Task { @MainActor [weak self] in
        self?.reconcile()
        self?.onChange?()
      }
    }
    permissionObserver = Task { [weak self] in
      for await allowed in ActivityAuthorizationInfo().activityEnablementUpdates {
        guard let self = self else { return }
        if !allowed, let current = self.session.watch, current.monitoring == .watching {
          self.interrupt(current.sessionId, reason: "Live Activities were turned off in Settings.")
        }
        self.onChange?()
      }
    }
  }

  func status() -> BuildWatchStatus {
    reconcile()
    let unavailable: String?
    if !registered {
      unavailable = "iOS could not register background build watching."
    } else if launch.pendingSessionId != nil, session.watch?.monitoring != .watching {
      unavailable = "iOS is finishing the previous build watch. Try again in a moment."
    } else if !ActivityAuthorizationInfo().areActivitiesEnabled {
      unavailable = "Allow Live Activities for Pipedash in Settings to watch builds."
    } else if UIApplication.shared.backgroundRefreshStatus != .available {
      unavailable = "Background execution is unavailable. Check Background App Refresh and Low Power Mode."
    } else {
      unavailable = nil
    }
    return BuildWatchStatus(
      supported: true, available: unavailable == nil,
      reason: unavailable ?? session.reason, watch: session.watch)
  }

  func start(_ snapshot: BuildWatchSnapshot) throws -> BuildWatchStatus {
    guard UIApplication.shared.applicationState == .active else {
      throw BuildWatchValidationError.invalid("Start watching while Pipedash is in the foreground")
    }
    let current = status()
    guard current.available else {
      throw BuildWatchValidationError.invalid(current.reason ?? "Build watching is unavailable")
    }
    try session.begin(snapshot)
    do {
      try launch.submit(snapshot.sessionId)
      let created = try Activity<PipedashBuildActivityAttributes>.request(
        attributes: PipedashBuildActivityAttributes(sessionId: snapshot.sessionId),
        content: content(snapshot), pushType: nil)
      activity = created
      observe(created)
      let request = BGContinuedProcessingTaskRequest(
        identifier: Self.taskIdentifier,
        title: systemTitle(snapshot), subtitle: systemSubtitle(snapshot))
      request.strategy = .fail
      try BGTaskScheduler.shared.submit(request)
    } catch {
      launch.submissionFailed(snapshot.sessionId)
      interrupt(
        snapshot.sessionId,
        reason: "iOS could not start background monitoring: \(error.localizedDescription)")
      throw error
    }
    onChange?()
    return status()
  }

  func update(_ snapshot: BuildWatchSnapshot) throws -> BuildWatchStatus {
    reconcile()
    guard try session.update(snapshot) else { return status() }
    if snapshot.monitoring == .watching {
      backgroundTask?.updateTitle(systemTitle(snapshot), subtitle: systemSubtitle(snapshot))
      if let activity = activity {
        enqueue { await activity.update(self.content(snapshot)) }
      }
    } else {
      finishPresentation(snapshot, completed: snapshot.monitoring == .completed)
    }
    onChange?()
    return status()
  }

  func stop(_ args: StopBuildWatchArgs) throws -> BuildWatchStatus {
    try args.validate()
    let message = args.reason == .interrupted
      ? "Monitoring was interrupted. Open Pipedash to start watching again."
      : "Monitoring stopped. The build continues on its provider."
    if session.finish(sessionId: args.sessionId, monitoring: args.reason, reason: message),
      let snapshot = session.watch
    {
      finishPresentation(snapshot, completed: false)
      onChange?()
    }
    return status()
  }

  private func accept(_ task: BGContinuedProcessingTask) {
    let requestedSession = launch.receive()
    defer { onChange?() }
    guard let snapshot = session.watch,
      requestedSession == snapshot.sessionId, backgroundTask == nil
    else {
      task.setTaskCompleted(success: false)
      return
    }
    task.progress.totalUnitCount = 1
    task.progress.completedUnitCount = snapshot.monitoring == .completed ? 1 : 0
    task.updateTitle(systemTitle(snapshot), subtitle: systemSubtitle(snapshot))
    guard snapshot.monitoring == .watching else {
      task.setTaskCompleted(success: snapshot.monitoring == .completed)
      return
    }
    backgroundTask = task
    let sessionId = snapshot.sessionId
    task.expirationHandler = { [weak self] in
      Task { @MainActor [weak self] in
        self?.interrupt(
          sessionId,
          reason: "iOS ended background monitoring or it was cancelled in the system interface. The build continues on its provider.")
      }
    }
  }

  private func observe(_ activity: Activity<PipedashBuildActivityAttributes>) {
    activityObserver?.cancel()
    activityObserver = Task { [weak self] in
      for await state in activity.activityStateUpdates {
        guard !Task.isCancelled, let self = self,
          self.session.watch?.sessionId == activity.attributes.sessionId,
          self.session.watch?.monitoring == .watching
        else { return }
        switch state {
        case .dismissed:
          _ = try? self.stop(StopBuildWatchArgs(sessionId: activity.attributes.sessionId, reason: .stopped))
        case .ended:
          self.interrupt(activity.attributes.sessionId, reason: "The Live Activity ended. Start watching again in Pipedash.")
        default:
          break
        }
      }
    }
  }

  private func reconcile() {
    guard let snapshot = session.watch, snapshot.monitoring == .watching else { return }
    guard ActivityAuthorizationInfo().areActivitiesEnabled,
      UIApplication.shared.backgroundRefreshStatus == .available
    else {
      interrupt(snapshot.sessionId, reason: "Live Activities or background execution are no longer available.")
      return
    }
    if let activity = activity, activity.activityState == .dismissed || activity.activityState == .ended {
      interrupt(snapshot.sessionId, reason: "The Live Activity is no longer active. Start watching again in Pipedash.")
    }
  }

  private func interrupt(_ sessionId: String, reason: String) {
    guard session.finish(sessionId: sessionId, monitoring: .interrupted, reason: reason),
      let snapshot = session.watch
    else { return }
    finishPresentation(snapshot, completed: false)
    onChange?()
  }

  private func finishPresentation(_ snapshot: BuildWatchSnapshot, completed: Bool) {
    if launch.pendingSessionId == nil {
      BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: Self.taskIdentifier)
    }
    if let task = backgroundTask {
      task.expirationHandler = nil
      if completed { task.progress.completedUnitCount = 1 }
      task.updateTitle(systemTitle(snapshot), subtitle: systemSubtitle(snapshot))
      task.setTaskCompleted(success: completed)
      backgroundTask = nil
    }
    activityObserver?.cancel()
    activityObserver = nil
    if let activity = activity {
      self.activity = nil
      let policy: ActivityUIDismissalPolicy = snapshot.monitoring == .stopped
        ? .immediate : .after(Date().addingTimeInterval(completed ? 120 : 60))
      enqueue { await activity.end(self.content(snapshot), dismissalPolicy: policy) }
    }
  }

  private func enqueue(_ operation: @escaping @MainActor () async -> Void) {
    let previous = activityOperation
    activityOperation = Task {
      await previous?.value
      await operation()
    }
  }

  private func content(_ snapshot: BuildWatchSnapshot) -> ActivityContent<BuildWatchSnapshot> {
    let stale = snapshot.monitoring == .watching
      ? Date(timeIntervalSince1970: TimeInterval(snapshot.updatedAt)).addingTimeInterval(90) : nil
    return ActivityContent(state: snapshot, staleDate: stale)
  }

  private func systemTitle(_ snapshot: BuildWatchSnapshot) -> String {
    "Pipedash · \(snapshot.pipelineName) #\(snapshot.runNumber)"
  }

  private func systemSubtitle(_ snapshot: BuildWatchSnapshot) -> String {
    switch snapshot.monitoring {
    case .watching: return "Watching · \(snapshot.status.title) · \(snapshot.providerName)"
    case .completed: return "Build \(snapshot.status.title.lowercased())"
    case .interrupted: return "Monitoring interrupted · build may still be running"
    case .stopped: return "Monitoring stopped · build may still be running"
    }
  }
}
#endif
