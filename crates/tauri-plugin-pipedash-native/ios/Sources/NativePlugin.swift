import AuthenticationServices
import PipedashActivityModels
import SwiftRs
import Tauri
import UIKit
import UserNotifications
import WebKit

class NativePlugin: Plugin {
  private var statusBarProxy: StatusBarTapProxy?
  private var safeAreaBridge: SafeAreaBridge?
  var exportPickerDelegate: ExportPickerDelegate?
  weak var shareWebView: WKWebView?
  private var accessibilityObservers: [NSObjectProtocol] = []
  var authSession: ASWebAuthenticationSession?
  var authPresenter: AuthSessionPresenter?
  var authInvoke: Invoke?

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
}

@_cdecl("init_plugin_pipedash_native")
public func initPlugin() -> Plugin {
  return NativePlugin()
}
