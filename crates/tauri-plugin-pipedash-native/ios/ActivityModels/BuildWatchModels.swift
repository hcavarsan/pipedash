import Foundation

public enum BuildWatchRunStatus: String, Codable, Hashable {
  case pending, running, success, failed, cancelled, skipped

  public var isTerminal: Bool {
    switch self {
    case .pending, .running: return false
    case .success, .failed, .cancelled, .skipped: return true
    }
  }

  public var title: String {
    switch self {
    case .pending: return "Queued"
    case .running: return "Running"
    case .success: return "Succeeded"
    case .failed: return "Failed"
    case .cancelled: return "Cancelled"
    case .skipped: return "Skipped"
    }
  }

  public var symbol: String {
    switch self {
    case .pending: return "clock"
    case .running: return "gearshape.2"
    case .success: return "checkmark.circle.fill"
    case .failed: return "xmark.circle.fill"
    case .cancelled: return "stop.circle"
    case .skipped: return "forward.end.circle"
    }
  }
}

public enum BuildWatchMonitoring: String, Codable, Hashable {
  case watching, completed, interrupted, stopped
}

public enum BuildWatchValidationError: LocalizedError {
  case invalid(String)

  public var errorDescription: String? {
    switch self {
    case .invalid(let message): return message
    }
  }
}

public struct BuildWatchSnapshot: Codable, Hashable {
  public var sessionId: String
  public var pipelineId: String
  public var pipelineName: String
  public var providerName: String
  public var providerType: String
  public var runNumber: Int64
  public var branch: String?
  public var status: BuildWatchRunStatus
  public var startedAt: Int64
  public var concludedAt: Int64?
  public var updatedAt: Int64
  public var monitoring: BuildWatchMonitoring
  public var url: String

  public init(
    sessionId: String, pipelineId: String, pipelineName: String, providerName: String,
    providerType: String, runNumber: Int64, branch: String?, status: BuildWatchRunStatus,
    startedAt: Int64, concludedAt: Int64?, updatedAt: Int64,
    monitoring: BuildWatchMonitoring, url: String
  ) {
    self.sessionId = sessionId
    self.pipelineId = pipelineId
    self.pipelineName = pipelineName
    self.providerName = providerName
    self.providerType = providerType
    self.runNumber = runNumber
    self.branch = branch
    self.status = status
    self.startedAt = startedAt
    self.concludedAt = concludedAt
    self.updatedAt = updatedAt
    self.monitoring = monitoring
    self.url = url
  }

  public func validate() throws {
    for (name, value, limit) in [
      ("sessionId", sessionId, 128), ("pipelineId", pipelineId, 2048),
      ("pipelineName", pipelineName, 512), ("providerName", providerName, 512),
      ("providerType", providerType, 128), ("url", url, 8192),
    ] {
      guard !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        value.utf8.count <= limit
      else { throw BuildWatchValidationError.invalid("The build watch \(name) is invalid") }
    }
    guard branch.map({ $0.utf8.count <= 1024 }) ?? true,
      runNumber > 0, runNumber <= 9_007_199_254_740_991,
      startedAt >= 0, updatedAt >= 0,
      concludedAt.map({ $0 >= startedAt }) ?? true,
      (monitoring != .watching || !status.isTerminal),
      (monitoring != .completed || status.isTerminal)
    else { throw BuildWatchValidationError.invalid("The build watch state is invalid") }
    let allowed = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
    guard let encodedId = pipelineId.addingPercentEncoding(withAllowedCharacters: allowed),
      url == "pipedash://pipelines/\(encodedId)/runs/\(runNumber)",
      URL(string: url) != nil
    else { throw BuildWatchValidationError.invalid("The build watch run link is invalid") }
    guard try JSONEncoder().encode(self).count <= 3500 else {
      throw BuildWatchValidationError.invalid("The build watch is too large for a Live Activity")
    }
  }

  public func ending(_ monitoring: BuildWatchMonitoring, at time: Int64) -> Self {
    var result = self
    result.monitoring = monitoring
    result.updatedAt = max(updatedAt, time)
    return result
  }

  public var elapsedSeconds: Int64 {
    max(0, (concludedAt ?? updatedAt) - startedAt)
  }

  public var elapsedLabel: String {
    let seconds = elapsedSeconds
    if seconds >= 3600 {
      return String(format: "%lld:%02lld:%02lld", seconds / 3600, seconds / 60 % 60, seconds % 60)
    }
    return String(format: "%lld:%02lld", seconds / 60, seconds % 60)
  }

  private enum CodingKeys: String, CodingKey {
    case sessionId, pipelineId, pipelineName, providerName, providerType, runNumber, branch
    case status, startedAt, concludedAt, updatedAt, monitoring, url
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(sessionId, forKey: .sessionId)
    try container.encode(pipelineId, forKey: .pipelineId)
    try container.encode(pipelineName, forKey: .pipelineName)
    try container.encode(providerName, forKey: .providerName)
    try container.encode(providerType, forKey: .providerType)
    try container.encode(runNumber, forKey: .runNumber)
    try container.encode(branch, forKey: .branch)
    try container.encode(status, forKey: .status)
    try container.encode(startedAt, forKey: .startedAt)
    try container.encode(concludedAt, forKey: .concludedAt)
    try container.encode(updatedAt, forKey: .updatedAt)
    try container.encode(monitoring, forKey: .monitoring)
    try container.encode(url, forKey: .url)
  }
}

public struct BuildWatchStatus: Encodable {
  public let supported: Bool
  public let available: Bool
  public let reason: String?
  public let watch: BuildWatchSnapshot?

  public init(supported: Bool, available: Bool, reason: String?, watch: BuildWatchSnapshot?) {
    self.supported = supported
    self.available = available
    self.reason = reason
    self.watch = watch
  }

  private enum CodingKeys: String, CodingKey {
    case supported, available, reason, watch
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(supported, forKey: .supported)
    try container.encode(available, forKey: .available)
    try container.encode(reason, forKey: .reason)
    try container.encode(watch, forKey: .watch)
  }
}
