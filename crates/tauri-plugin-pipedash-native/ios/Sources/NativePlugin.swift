import PipedashActivityModels
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


  private func show() {
    guard armed, cover == nil, let window = hostWindow() else { return }
    safetyRelease?.cancel()
    safetyRelease = nil

    let view = UIView(frame: window.bounds)
    view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    view.backgroundColor = .systemGroupedBackground
    view.accessibilityViewIsModal = true


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
    show()
    if armed && holdForFrontend && cover != nil {
      awaitingFrontend = true
    }
  }

  private func didBecomeActive() {
    guard cover != nil else { return }
    safetyRelease?.cancel()
    let work = DispatchWorkItem { [weak self] in self?.hide() }
    safetyRelease = work
    DispatchQueue.main.asyncAfter(deadline: .now() + PrivacyCoverController.safetyTimeout, execute: work)
    if !awaitingFrontend {
      hide()
    }
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

final class SafeAreaBridge: UIView, WKScriptMessageHandler {
  private static let handlerName = "pipedashSafeArea"
  private weak var webview: WKWebView?
  private var navigationObservation: NSKeyValueObservation?
  private var lastInsets: UIEdgeInsets?

  init(webview: WKWebView) {
    self.webview = webview
    super.init(frame: webview.bounds)
    autoresizingMask = [.flexibleWidth, .flexibleHeight]
    isUserInteractionEnabled = false
    accessibilityElementsHidden = true
    let controller = webview.configuration.userContentController
    controller.add(self, name: Self.handlerName)
    controller.addUserScript(WKUserScript(
      source: "window.webkit.messageHandlers.\(Self.handlerName).postMessage(null)",
      injectionTime: .atDocumentStart, forMainFrameOnly: true))
    navigationObservation = webview.observe(\.isLoading, options: [.new]) { [weak self] webview, _ in
      if !webview.isLoading {
        self?.publish(force: true)
      }
    }
    webview.addSubview(self)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  func detach() {
    navigationObservation = nil
    webview?.configuration.userContentController.removeScriptMessageHandler(forName: Self.handlerName)
    removeFromSuperview()
  }

  override func safeAreaInsetsDidChange() {
    super.safeAreaInsetsDidChange()
    publish()
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    publish()
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    publish(force: true)
  }

  func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
    guard message.frameInfo.isMainFrame else { return }
    publish(force: true)
  }

  private func publish(force: Bool = false) {
    guard let webview = webview, webview.window != nil else { return }
    let insets = webview.safeAreaInsets
    guard force || insets != lastInsets else { return }
    lastInsets = insets
    webview.evaluateJavaScript("""
      (() => {
        const apply = () => {
          const style = document.documentElement.style;
          style.setProperty('--native-safe-area-top', '\(insets.top)px');
          style.setProperty('--native-safe-area-right', '\(insets.right)px');
          style.setProperty('--native-safe-area-bottom', '\(insets.bottom)px');
          style.setProperty('--native-safe-area-left', '\(insets.left)px');
        };
        if (document.documentElement) apply();
        else document.addEventListener('DOMContentLoaded', apply, { once: true });
      })();
      """, completionHandler: nil)
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

final class ExportPickerDelegate: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
  private var completion: ((Bool) -> Void)?

  init(completion: @escaping (Bool) -> Void) {
    self.completion = completion
    super.init()
  }

  private func finish(_ completed: Bool) {
    let callback = completion
    completion = nil
    callback?(completed)
  }

  func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
    finish(!urls.isEmpty)
  }

  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    finish(false)
  }

  func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
    finish(false)
  }
}

class NativePlugin: Plugin {
  private var statusBarProxy: StatusBarTapProxy?
  private var safeAreaBridge: SafeAreaBridge?
  private var exportPickerDelegate: ExportPickerDelegate?
  private weak var shareWebView: WKWebView?
  private var accessibilityObservers: [NSObjectProtocol] = []

  override init() {
    super.init()
    NotificationRouter.shared.install(plugin: self)
    AppearanceController.shared.start()
    PrivacyCoverController.shared.start()
    NotificationRefreshController.shared.deliver = { [weak self] event in
      try? self?.trigger("notification-refresh", data: event)
    }
    NotificationRefreshController.shared.start()
    #if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
    if #available(iOS 26.0, *), !ProcessInfo.processInfo.isiOSAppOnMac {
      DispatchQueue.main.async { [weak self] in
        BuildWatchController.shared.onChange = { [weak self] in
          try? self?.trigger("build-watch-changed", data: BuildWatchController.shared.status())
        }
      }
    }
    #endif
    for name in [
      UIAccessibility.reduceTransparencyStatusDidChangeNotification,
      UIAccessibility.darkerSystemColorsStatusDidChangeNotification,
      UIApplication.didBecomeActiveNotification,
    ] {
      accessibilityObservers.append(NotificationCenter.default.addObserver(
        forName: name, object: nil, queue: .main
      ) { [weak self] _ in
        self?.shareWebView?.evaluateJavaScript(
          "window.dispatchEvent(new Event('pipedash:accessibility-change'))", completionHandler: nil)
      })
    }
  }

  deinit {
    for observer in accessibilityObservers {
      NotificationCenter.default.removeObserver(observer)
    }
    safeAreaBridge?.detach()
  }

  public override func load(webview: WKWebView) {
    if shareWebView !== webview {
      safeAreaBridge?.detach()
      safeAreaBridge = SafeAreaBridge(webview: webview)
    }
    shareWebView = webview
    webview.isOpaque = false
    webview.backgroundColor = .systemGroupedBackground
    webview.scrollView.backgroundColor = .systemGroupedBackground
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

  @objc func buildWatchStatus(_ invoke: Invoke) {
    DispatchQueue.main.async {
      #if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
      if #available(iOS 26.0, *), !ProcessInfo.processInfo.isiOSAppOnMac {
        invoke.resolve(BuildWatchController.shared.status())
        return
      }
      #endif
      invoke.resolve(unsupportedBuildWatchStatus())
    }
  }

  @objc func startBuildWatch(_ invoke: Invoke) throws {
    let snapshot = try invoke.parseArgs(BuildWatchSnapshot.self)
    try snapshot.validate()
    DispatchQueue.main.async {
      #if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
      if #available(iOS 26.0, *), !ProcessInfo.processInfo.isiOSAppOnMac {
        do {
          invoke.resolve(try BuildWatchController.shared.start(snapshot))
        } catch {
          invoke.reject(error.localizedDescription)
        }
        return
      }
      #endif
      invoke.reject(unsupportedBuildWatchStatus().reason ?? "Build watching is unavailable")
    }
  }

  @objc func updateBuildWatch(_ invoke: Invoke) throws {
    let snapshot = try invoke.parseArgs(BuildWatchSnapshot.self)
    try snapshot.validate()
    DispatchQueue.main.async {
      #if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
      if #available(iOS 26.0, *), !ProcessInfo.processInfo.isiOSAppOnMac {
        do {
          invoke.resolve(try BuildWatchController.shared.update(snapshot))
        } catch {
          invoke.reject(error.localizedDescription)
        }
        return
      }
      #endif
      invoke.resolve(unsupportedBuildWatchStatus())
    }
  }

  @objc func stopBuildWatch(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(StopBuildWatchArgs.self)
    try args.validate()
    DispatchQueue.main.async {
      #if compiler(>=6.2) && !targetEnvironment(simulator) && !targetEnvironment(macCatalyst)
      if #available(iOS 26.0, *), !ProcessInfo.processInfo.isiOSAppOnMac {
        do {
          invoke.resolve(try BuildWatchController.shared.stop(args))
        } catch {
          invoke.reject(error.localizedDescription)
        }
        return
      }
      #endif
      invoke.resolve(unsupportedBuildWatchStatus())
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
    DispatchQueue.main.async {
      invoke.resolve(
        AccessibilityState(
          reduceTransparency: UIAccessibility.isReduceTransparencyEnabled,
          increaseContrast: UIAccessibility.isDarkerSystemColorsEnabled))
    }
  }

  @objc func localNotificationSettings(_ invoke: Invoke) {
    DispatchQueue.main.async {
      NotificationRefreshController.shared.settings { invoke.resolve($0) }
    }
  }

  @objc func configureLocalNotifications(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ConfigureNotificationsArgs.self)
    DispatchQueue.main.async {
      NotificationRefreshController.shared.configure(enabled: args.enabled) { result in
        switch result {
        case .success(let settings):
          invoke.resolve(settings)
        case .failure(let error):
          invoke.reject(error.localizedDescription)
        }
      }
    }
  }

  @objc func notificationRefreshReady(_ invoke: Invoke) {
    invoke.resolve()
    DispatchQueue.main.async { NotificationRefreshController.shared.listenerReady() }
  }

  @objc func finishNotificationRefresh(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FinishNotificationRefreshArgs.self)
    try args.validate()
    invoke.resolve()
    DispatchQueue.main.async {
      NotificationRefreshController.shared.finish(args.id, success: args.success)
    }
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

      guard self.exportPickerDelegate == nil else {
        invoke.reject("An export picker is already open")
        return
      }
      let controller = UIDocumentPickerViewController(
        forExporting: [URL(fileURLWithPath: args.path)], asCopy: true)
      let delegate = ExportPickerDelegate { [weak self] completed in
        self?.exportPickerDelegate = nil
        invoke.resolve(ShareResult(completed: completed))
      }
      self.exportPickerDelegate = delegate
      controller.delegate = delegate
      controller.presentationController?.delegate = delegate
      presenter.present(controller, animated: true)
    }
  }

  @objc func shareLink(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ShareLinkArgs.self)
    DispatchQueue.main.async {
      guard let url = URL(string: args.url),
        let scheme = url.scheme?.lowercased(), ["http", "https"].contains(scheme)
      else {
        invoke.reject("The link must be an HTTP or HTTPS URL")
        return
      }
      guard let presenter = self.topViewController() else {
        invoke.reject("Nothing is available to present the share sheet")
        return
      }
      let rect = args.sourceRect
      guard rect.x.isFinite, rect.y.isFinite, rect.width.isFinite, rect.height.isFinite,
        rect.width > 0, rect.height > 0
      else {
        invoke.reject("The share button position isn't valid")
        return
      }
      let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
      if let popover = controller.popoverPresentationController {
        let source: UIView = self.shareWebView ?? presenter.view
        popover.sourceView = source
        popover.sourceRect = CGRect(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
          .intersection(source.bounds)
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
