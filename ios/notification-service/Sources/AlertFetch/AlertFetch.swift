import Foundation

public struct HostConfig: Decodable {
  public let serverUrl: String
  public let token: String
  public let allowPrivateHttp: Bool
  public let standalone: Bool

  private enum CodingKeys: String, CodingKey {
    case serverUrl, token, allowPrivateHttp, standalone
  }

  public init(from decoder: Decoder) throws {
    let values = try decoder.container(keyedBy: CodingKeys.self)
    serverUrl = try values.decode(String.self, forKey: .serverUrl)
    token = try values.decode(String.self, forKey: .token)
    allowPrivateHttp = try values.decode(Bool.self, forKey: .allowPrivateHttp)
    standalone = try values.decodeIfPresent(Bool.self, forKey: .standalone) ?? false
  }
}

public struct AlertRoute: Equatable {
  public let key: String
  public let pipelineId: String?
  public let providerId: Int64?
}

public struct HostAlert: Decodable {
  public let id: String
  public let key: String
  public let kind: String
  public let title: String
  public let body: String
  public let pipelineId: String?
  public let providerId: Int64?
}

public enum AlertFetch {
  public static let timeout: TimeInterval = 20

  private static let decoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return decoder
  }()

  public static func config(from data: Data) -> HostConfig? {
    try? decoder.decode(HostConfig.self, from: data)
  }

  public static func route(for alert: HostAlert, standalone: Bool) -> AlertRoute {
    standalone
      ? AlertRoute(key: "alert-host:\(alert.key)", pipelineId: nil, providerId: nil)
      : AlertRoute(key: alert.key, pipelineId: alert.pipelineId, providerId: alert.providerId)
  }

  public static func decode(_ data: Data) -> HostAlert? {
    try? decoder.decode(HostAlert.self, from: data)
  }

  public static func isValidAlertId(_ id: String) -> Bool {
    (1...64).contains(id.utf8.count)
      && id.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (65...90).contains(byte) || (97...122).contains(byte)
          || byte == 45 || byte == 95
      }
  }

  public static func isPrivateLiteral(_ host: String) -> Bool {
    let bare = host.trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
    var v4 = in_addr()
    if inet_pton(AF_INET, bare, &v4) == 1 {
      let value = UInt32(bigEndian: v4.s_addr)
      let first = value >> 24
      let second = (value >> 16) & 0xff
      return first == 10 || first == 127
        || (first == 172 && (16...31).contains(second))
        || (first == 192 && second == 168)
        || (first == 100 && (64...127).contains(second))
        || (first == 169 && second == 254)
    }
    var v6 = in6_addr()
    if inet_pton(AF_INET6, bare, &v6) == 1 {
      let bytes = withUnsafeBytes(of: &v6) { Array($0) }
      let loopback = bytes.dropLast().allSatisfy { $0 == 0 } && bytes[15] == 1
      return loopback || bytes[0] & 0xfe == 0xfc || (bytes[0] == 0xfe && bytes[1] & 0xc0 == 0x80)
    }
    return false
  }

  public static func isAllowed(_ url: URL, allowPrivateHttp: Bool) -> Bool {
    guard let host = url.host?.lowercased(), !host.isEmpty else { return false }
    switch url.scheme?.lowercased() {
    case "https":
      return true
    case "http":
      return allowPrivateHttp && (isPrivateLiteral(host) || host.hasSuffix(".local"))
    default:
      return false
    }
  }

  public static func request(config: HostConfig, alertId: String) -> URLRequest? {
    guard isValidAlertId(alertId) else { return nil }
    let base = config.serverUrl.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    guard let url = URL(string: "\(base)/api/v1/alerts/\(alertId)"),
      isAllowed(url, allowPrivateHttp: config.allowPrivateHttp)
    else { return nil }
    var request = URLRequest(url: url, timeoutInterval: timeout)
    request.setValue("Bearer \(config.token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    return request
  }
}
