import Foundation

struct HapticArgs: Decodable {
  let kind: String
}

struct AppearanceArgs: Decodable {
  let scheme: String
}

struct BadgeArgs: Decodable {
  let count: Int
}

struct ShareArgs: Decodable {
  let path: String
}

struct ShareRect: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double
}

struct ShareLinkArgs: Decodable {
  let url: String
  let sourceRect: ShareRect
}

struct PrivacyCoverArgs: Decodable {
  let armed: Bool
  let holdForFrontend: Bool
}

struct AccessibilityState: Encodable {
  let reduceTransparency: Bool
  let increaseContrast: Bool
}

struct ShareResult: Encodable {
  let completed: Bool
}

struct AuthSessionArgs: Decodable {
  let url: String
}

struct AuthSessionResult: Encodable {
  let cancelled: Bool
}

struct EmptyPayload: Encodable {}

struct NotificationActionPayload: Encodable {
  let actionId: String
  let key: String?
  let pipelineId: String?
  let providerId: String?
  let runNumber: String?
}
