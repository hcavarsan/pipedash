#if canImport(ActivityKit)
import ActivityKit

@available(iOS 16.2, *)
public struct PipedashBuildActivityAttributes: ActivityAttributes {
  public typealias ContentState = BuildWatchSnapshot

  public let sessionId: String

  public init(sessionId: String) {
    self.sessionId = sessionId
  }
}
#endif
