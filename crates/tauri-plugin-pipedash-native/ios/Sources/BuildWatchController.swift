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
  private var taskPresentation: BuildWatchTaskPresentation?
  private var foregroundObserver: NSObjectProtocol?
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
    // End custom activities left by older app versions; new watches use only the system task.
    for orphan in Activity<PipedashBuildActivityAttributes>.activities {
      Task { await orphan.end(nil, dismissalPolicy: .immediate) }
    }
    foregroundObserver = NotificationCenter.default.addObserver(
      forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
    ) { [weak self] _ in
      Task { @MainActor [weak self] in
        self?.reconcile()
        self?.onChange?()
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
      let presentation = BuildWatchTaskPresentation(snapshot)
      taskPresentation = presentation
      let request = BGContinuedProcessingTaskRequest(
        identifier: Self.taskIdentifier,
        title: presentation.title, subtitle: presentation.subtitle)
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
      updateTaskPresentation(snapshot)
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
    updateTaskPresentation(snapshot, task: task)
    guard snapshot.monitoring == .watching else {
      task.setTaskCompleted(success: snapshot.monitoring == .completed)
      taskPresentation = nil
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

  private func reconcile() {
    guard let snapshot = session.watch, snapshot.monitoring == .watching else { return }
    if UIApplication.shared.backgroundRefreshStatus != .available {
      interrupt(snapshot.sessionId, reason: "Background execution is no longer available.")
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
      updateTaskPresentation(snapshot)
      task.setTaskCompleted(success: completed)
      backgroundTask = nil
      taskPresentation = nil
    }
  }

  private func updateTaskPresentation(
    _ snapshot: BuildWatchSnapshot, task: BGContinuedProcessingTask? = nil
  ) {
    guard let task = task ?? backgroundTask else { return }
    let presentation = BuildWatchTaskPresentation(snapshot)
    guard taskPresentation != presentation else { return }
    task.updateTitle(presentation.title, subtitle: presentation.subtitle)
    taskPresentation = presentation
  }
}
#endif
