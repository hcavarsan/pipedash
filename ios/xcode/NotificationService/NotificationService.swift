import Foundation
import Security
import UserNotifications

private final class RedirectRefusal: NSObject, URLSessionTaskDelegate {
  func urlSession(
    _ session: URLSession,
    task: URLSessionTask,
    willPerformHTTPRedirection response: HTTPURLResponse,
    newRequest request: URLRequest,
    completionHandler: @escaping (URLRequest?) -> Void
  ) {
    completionHandler(nil)
  }
}

final class NotificationService: UNNotificationServiceExtension {
  private static let keychainService = "pipedash.background-notifications"
  private static let keychainAccount = "alert-host"
  private static let failureCategory = "pipedash-failure"

  private let lock = NSLock()
  private var handler: ((UNNotificationContent) -> Void)?
  private var bestAttempt: UNMutableNotificationContent?
  private var session: URLSession?

  override func didReceive(
    _ request: UNNotificationRequest,
    withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
  ) {
    guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else {
      contentHandler(request.content)
      return
    }
    lock.lock()
    handler = contentHandler
    bestAttempt = content
    lock.unlock()

    guard let alertId = (content.userInfo["pipedash"] as? [String: Any])?["alertId"] as? String,
      let item = Self.hostItem(),
      let config = AlertFetch.config(from: item),
      let fetch = AlertFetch.request(config: config, alertId: alertId)
    else {
      finish(nil)
      return
    }

    let configuration = URLSessionConfiguration.ephemeral
    configuration.timeoutIntervalForResource = AlertFetch.timeout
    let session = URLSession(configuration: configuration, delegate: RedirectRefusal(), delegateQueue: nil)
    self.session = session
    session.dataTask(with: fetch) { [weak self] data, response, _ in
      let ok = (response as? HTTPURLResponse)?.statusCode == 200
      self?.finish(ok ? data.flatMap(AlertFetch.decode) : nil, standalone: config.standalone)
    }.resume()
  }

  override func serviceExtensionTimeWillExpire() {
    finish(nil)
  }

  private func finish(_ alert: HostAlert?, standalone: Bool = false) {
    lock.lock()
    guard let handler, let content = bestAttempt else {
      lock.unlock()
      return
    }
    self.handler = nil
    if let alert { Self.apply(alert, standalone: standalone, to: content) }
    lock.unlock()
    session?.invalidateAndCancel()
    handler(content)
  }

  private static func apply(
    _ alert: HostAlert, standalone: Bool, to content: UNMutableNotificationContent
  ) {
    content.title = alert.title
    content.body = alert.body
    let route = AlertFetch.route(for: alert, standalone: standalone)
    content.threadIdentifier = route.key
    if route.pipelineId != nil, alert.kind == "failed" || alert.kind == "degraded" {
      content.categoryIdentifier = failureCategory
    }
    var fields = ["key": route.key]
    if let pipelineId = route.pipelineId { fields["pipelineId"] = pipelineId }
    if let providerId = route.providerId { fields["providerId"] = String(providerId) }
    var userInfo = content.userInfo
    userInfo["pipedash"] = fields
    content.userInfo = userInfo
  }

  private static func hostItem() -> Data? {
    guard let group = Bundle.main.object(forInfoDictionaryKey: "PipedashKeychainGroup") as? String,
      !group.isEmpty
    else { return nil }
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: keychainService,
      kSecAttrAccount as String: keychainAccount,
      kSecAttrAccessGroup as String: group,
      kSecUseDataProtectionKeychain as String: true,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    var result: AnyObject?
    guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
    return result as? Data
  }
}
