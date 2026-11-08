import Tauri
import UIKit
import UserNotifications

private let failureCategoryId = "pipedash-failure"
private let openRunActionId = "open-run"

final class NotificationRouter: NSObject, UNUserNotificationCenterDelegate {
  static let shared = NotificationRouter()

  private weak var plugin: Plugin?

  func install(plugin: Plugin) {
    self.plugin = plugin
    let center = UNUserNotificationCenter.current()
    center.delegate = self

    let openRun = UNNotificationAction(
      identifier: openRunActionId, title: "Open Run", options: [.foreground])
    let failure = UNNotificationCategory(
      identifier: failureCategoryId, actions: [openRun], intentIdentifiers: [], options: [])
    center.setNotificationCategories([failure])
  }

  private static func fields(from userInfo: [AnyHashable: Any]) -> [String: String] {
    let source =
      (userInfo["__EXTRA__"] as? [String: Any]) ?? (userInfo["pipedash"] as? [String: Any]) ?? [:]
    var fields: [String: String] = [:]
    for (name, value) in source {
      if let text = value as? String {
        fields[name] = text
      } else if let number = value as? NSNumber {
        fields[name] = number.stringValue
      }
    }
    return fields
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    let remote = notification.request.trigger is UNPushNotificationTrigger
    if #available(iOS 14.0, *) {
      completionHandler(remote ? [.list] : [.banner, .list, .sound])
    } else {
      completionHandler(remote ? [] : [.alert, .sound])
    }
  }

  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    defer { completionHandler() }

    let action: String
    switch response.actionIdentifier {
    case UNNotificationDismissActionIdentifier:
      return
    case UNNotificationDefaultActionIdentifier:
      action = "tap"
    default:
      action = response.actionIdentifier
    }

    let fields = NotificationRouter.fields(from: response.notification.request.content.userInfo)
    let payload = NotificationActionPayload(
      actionId: action,
      key: fields["key"],
      pipelineId: fields["pipelineId"],
      providerId: fields["providerId"],
      runNumber: fields["runNumber"])
    try? plugin?.trigger("notificationAction", data: payload)
  }
}
