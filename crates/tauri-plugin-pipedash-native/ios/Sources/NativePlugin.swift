import SwiftRs
import Tauri
import UIKit
import UserNotifications
import WebKit

private let failureCategoryId = "pipedash-failure"
private let openRunActionId = "open-run"

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

struct EmptyPayload: Encodable {}

struct NotificationActionPayload: Encodable {
  let actionId: String
  let key: String?
  let pipelineId: String?
  let providerId: String?
  let runNumber: String?
}

final class Haptics {
  static let shared = Haptics()

  private let selection = UISelectionFeedbackGenerator()
  private let notification = UINotificationFeedbackGenerator()
  private var impacts: [UIImpactFeedbackGenerator.FeedbackStyle: UIImpactFeedbackGenerator] = [:]

  private init() {}

  func fire(_ kind: String) {
    switch kind {
    case "selection":
      selection.selectionChanged()
      selection.prepare()
    case "light":
      impact(.light)
    case "medium":
      impact(.medium)
    case "heavy":
      impact(.heavy)
    case "soft":
      impact(.soft)
    case "rigid":
      impact(.rigid)
    case "success":
      outcome(.success)
    case "warning":
      outcome(.warning)
    case "error":
      outcome(.error)
    default:
      break
    }
  }

  private func impact(_ style: UIImpactFeedbackGenerator.FeedbackStyle) {
    let generator = impacts[style] ?? UIImpactFeedbackGenerator(style: style)
    impacts[style] = generator
    generator.impactOccurred()
    generator.prepare()
  }

  private func outcome(_ type: UINotificationFeedbackGenerator.FeedbackType) {
    notification.notificationOccurred(type)
    notification.prepare()
  }
}

final class AppearanceController {
  static let shared = AppearanceController()

  private static let defaultsKey = "pipedash.native.appearance"

  private var style: UIUserInterfaceStyle
  private var started = false

  private init() {
    style = AppearanceController.style(
      for: UserDefaults.standard.string(forKey: AppearanceController.defaultsKey) ?? "system")
  }

  private static func style(for scheme: String) -> UIUserInterfaceStyle {
    switch scheme {
    case "light":
      return .light
    case "dark":
      return .dark
    default:
      return .unspecified
    }
  }

  func start() {
    guard !started else { return }
    started = true
    NotificationCenter.default.addObserver(
      forName: UIWindow.didBecomeKeyNotification, object: nil, queue: nil
    ) { [weak self] note in
      guard let self = self, let window = note.object as? UIWindow else { return }
      window.overrideUserInterfaceStyle = self.style
    }
  }

  func set(_ scheme: String) {
    style = AppearanceController.style(for: scheme)
    UserDefaults.standard.set(scheme, forKey: AppearanceController.defaultsKey)
    applyToWindows()
  }

  func applyToWindows() {
    for scene in UIApplication.shared.connectedScenes {
      guard let windowScene = scene as? UIWindowScene else { continue }
      for window in windowScene.windows {
        window.overrideUserInterfaceStyle = style
      }
    }
  }
}

final class PrivacyCoverController {
  static let shared = PrivacyCoverController()

  private static let safetyTimeout: TimeInterval = 5

  private var armed = false
  private var holdForFrontend = false
  private var awaitingFrontend = false
  private var cover: UIView?
  private var safetyRelease: DispatchWorkItem?
  private var started = false

  private init() {}

  func start() {
    guard !started else { return }
    started = true
    let center = NotificationCenter.default
    for name in [UIApplication.willResignActiveNotification, UIScene.willDeactivateNotification] {
      center.addObserver(forName: name, object: nil, queue: nil) { [weak self] _ in
        self?.show()
      }
    }
    for name in [UIApplication.didEnterBackgroundNotification, UIScene.didEnterBackgroundNotification] {
      center.addObserver(forName: name, object: nil, queue: nil) { [weak self] _ in
        self?.didEnterBackground()
      }
    }
    for name in [UIApplication.didBecomeActiveNotification, UIScene.didActivateNotification] {
      center.addObserver(forName: name, object: nil, queue: nil) { [weak self] _ in
        self?.didBecomeActive()
      }
    }
  }

  func configure(armed: Bool, holdForFrontend: Bool) {
    self.armed = armed
    self.holdForFrontend = holdForFrontend
    if !armed {
      hide()
    }
  }

  func release() {
    awaitingFrontend = false
    if UIApplication.shared.applicationState == .active {
      hide()
    }
  }

  private func hostWindow() -> UIWindow? {
    let windows = UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }
      .flatMap { $0.windows }
    return windows.first(where: { $0.isKeyWindow }) ?? windows.first
  }

  private static func appIcon() -> UIImage? {
    guard
      let icons = Bundle.main.infoDictionary?["CFBundleIcons"] as? [String: Any],
      let primary = icons["CFBundlePrimaryIcon"] as? [String: Any],
      let files = primary["CFBundleIconFiles"] as? [String],
      let name = files.last
    else { return nil }
    return UIImage(named: name)
  }

  private func show() {
    guard armed, cover == nil, let window = hostWindow() else { return }
    safetyRelease?.cancel()
    safetyRelease = nil

    let view = UIView(frame: window.bounds)
    view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    view.backgroundColor = .systemBackground
    view.accessibilityViewIsModal = true

    if let icon = PrivacyCoverController.appIcon() {
      let side: CGFloat = 96
      let imageView = UIImageView(image: icon)
      imageView.frame = CGRect(
        x: view.bounds.midX - side / 2, y: view.bounds.midY - side / 2, width: side, height: side)
      imageView.autoresizingMask = [
        .flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin,
      ]
      imageView.layer.cornerRadius = side * 0.2237
      imageView.layer.masksToBounds = true
      imageView.isAccessibilityElement = false
      view.addSubview(imageView)
    }

    window.addSubview(view)
    cover = view
  }

  private func hide() {
    safetyRelease?.cancel()
    safetyRelease = nil
    awaitingFrontend = false
    guard let view = cover else { return }
    cover = nil
    UIView.animate(
      withDuration: 0.15,
      animations: { view.alpha = 0 },
      completion: { _ in view.removeFromSuperview() })
  }

  private func didEnterBackground() {
    if armed && holdForFrontend && cover != nil {
      awaitingFrontend = true
    }
  }

  private func didBecomeActive() {
    guard cover != nil else { return }
    guard awaitingFrontend else {
      hide()
      return
    }
    safetyRelease?.cancel()
    let work = DispatchWorkItem { [weak self] in self?.hide() }
    safetyRelease = work
    DispatchQueue.main.asyncAfter(deadline: .now() + PrivacyCoverController.safetyTimeout, execute: work)
  }
}

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

final class StatusBarTapProxy: NSObject, UIScrollViewDelegate {
  private let scrollView = UIScrollView()
  private let onTap: () -> Void

  init(host: UIView, onTap: @escaping () -> Void) {
    self.onTap = onTap
    super.init()

    scrollView.translatesAutoresizingMaskIntoConstraints = false
    scrollView.backgroundColor = .clear
    scrollView.isUserInteractionEnabled = false
    scrollView.isAccessibilityElement = false
    scrollView.accessibilityElementsHidden = true
    scrollView.showsVerticalScrollIndicator = false
    scrollView.showsHorizontalScrollIndicator = false
    scrollView.scrollsToTop = true
    scrollView.delegate = self
    scrollView.contentSize = CGSize(width: 10, height: 10_000)
    scrollView.contentOffset = CGPoint(x: 0, y: 1)

    host.addSubview(scrollView)
    NSLayoutConstraint.activate([
      scrollView.leadingAnchor.constraint(equalTo: host.leadingAnchor),
      scrollView.trailingAnchor.constraint(equalTo: host.trailingAnchor),
      scrollView.topAnchor.constraint(equalTo: host.topAnchor),
      scrollView.bottomAnchor.constraint(equalTo: host.bottomAnchor),
    ])
  }

  func scrollViewShouldScrollToTop(_ scrollView: UIScrollView) -> Bool {
    onTap()
    return false
  }
}

class NativePlugin: Plugin {
  private var statusBarProxy: StatusBarTapProxy?

  override init() {
    super.init()
    NotificationRouter.shared.install(plugin: self)
    AppearanceController.shared.start()
    PrivacyCoverController.shared.start()
  }

  public override func load(webview: WKWebView) {
    let scrollView = webview.scrollView
    scrollView.bounces = false
    scrollView.alwaysBounceVertical = false
    scrollView.alwaysBounceHorizontal = false
    scrollView.contentInsetAdjustmentBehavior = .never
    scrollView.showsVerticalScrollIndicator = false
    scrollView.showsHorizontalScrollIndicator = false
    scrollView.scrollsToTop = false
    webview.allowsBackForwardNavigationGestures = false
    webview.allowsLinkPreview = false

    AppearanceController.shared.applyToWindows()

    DispatchQueue.main.async { [weak self, weak webview] in
      guard let self = self, let webview = webview else { return }
      let host = webview.superview ?? webview
      self.statusBarProxy = StatusBarTapProxy(host: host) { [weak self] in
        try? self?.trigger("statusBarTap", data: EmptyPayload())
      }
    }
  }

  @objc func haptic(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(HapticArgs.self)
    invoke.resolve()
    DispatchQueue.main.async { Haptics.shared.fire(args.kind) }
  }

  @objc func setAppearance(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(AppearanceArgs.self)
    invoke.resolve()
    DispatchQueue.main.async { AppearanceController.shared.set(args.scheme) }
  }

  @objc func accessibilityState(_ invoke: Invoke) {
    invoke.resolve(
      AccessibilityState(
        reduceTransparency: UIAccessibility.isReduceTransparencyEnabled,
        increaseContrast: UIAccessibility.isDarkerSystemColorsEnabled))
  }

  @objc func openSettings(_ invoke: Invoke) {
    DispatchQueue.main.async {
      guard let url = URL(string: UIApplication.openSettingsURLString) else {
        invoke.reject("Couldn't build the Settings URL")
        return
      }
      UIApplication.shared.open(url, options: [:]) { opened in
        if opened {
          invoke.resolve()
        } else {
          invoke.reject("Couldn't open Settings")
        }
      }
    }
  }

  @objc func setBadge(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(BadgeArgs.self)
    invoke.resolve()
    let count = max(0, args.count)
    DispatchQueue.main.async {
      if #available(iOS 16.0, *) {
        UNUserNotificationCenter.current().setBadgeCount(count) { _ in }
      } else {
        UIApplication.shared.applicationIconBadgeNumber = count
      }
    }
  }

  @objc func configurePrivacyCover(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PrivacyCoverArgs.self)
    invoke.resolve()
    DispatchQueue.main.async {
      PrivacyCoverController.shared.configure(
        armed: args.armed, holdForFrontend: args.holdForFrontend)
    }
  }

  @objc func releasePrivacyCover(_ invoke: Invoke) {
    invoke.resolve()
    DispatchQueue.main.async { PrivacyCoverController.shared.release() }
  }

  private func topViewController() -> UIViewController? {
    var top =
      manager.viewController
      ?? UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }
      .flatMap { $0.windows }
      .first(where: { $0.isKeyWindow })?
      .rootViewController
    while let presented = top?.presentedViewController {
      top = presented
    }
    return top
  }

  @objc func shareFile(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ShareArgs.self)
    DispatchQueue.main.async {
      guard FileManager.default.fileExists(atPath: args.path) else {
        invoke.reject("The file to share doesn't exist")
        return
      }
      guard let presenter = self.topViewController() else {
        invoke.reject("Nothing is available to present the share sheet")
        return
      }

      let controller = UIActivityViewController(
        activityItems: [URL(fileURLWithPath: args.path)], applicationActivities: nil)
      if let popover = controller.popoverPresentationController {
        popover.sourceView = presenter.view
        popover.sourceRect = CGRect(
          x: presenter.view.bounds.midX, y: presenter.view.bounds.midY, width: 0, height: 0)
        popover.permittedArrowDirections = []
      }
      controller.completionWithItemsHandler = { _, completed, _, error in
        if let error = error {
          invoke.reject(error.localizedDescription)
        } else {
          invoke.resolve(ShareResult(completed: completed))
        }
      }
      presenter.present(controller, animated: true)
    }
  }
}

@_cdecl("init_plugin_pipedash_native")
public func initPlugin() -> Plugin {
  return NativePlugin()
}
