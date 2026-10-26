#if canImport(ActivityKit)
import ActivityKit

// Retain the original type identity only to dismiss activities left by older app versions.
@available(iOS 16.2, *)
public struct PipedashBuildActivityAttributes: ActivityAttributes {
  public typealias ContentState = BuildWatchSnapshot

  public let sessionId: String
}
#endif
