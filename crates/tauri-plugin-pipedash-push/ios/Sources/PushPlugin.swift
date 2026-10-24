import ObjectiveC
import SwiftRs
import Tauri
import UIKit
import UserNotifications
import WebKit

private typealias DelegateIMP = @convention(c) (AnyObject, Selector, UIApplication, AnyObject) -> Void

final class PushRegistry {
  static let shared = PushRegistry()

  // Main queue only.
  private var pending: [Invoke] = []
  private var installed = false
  // UIApplication.delegate is not retaining; when UIKit created the delegate, setting it to nil
  // releases UIKit's reference, so this keeps it alive across the reassignment below.
  private var appDelegate: UIApplicationDelegate?

  private init() {}

  func installDelegateHooks() {
    guard !installed, let delegate = UIApplication.shared.delegate else { return }
    installed = true
    appDelegate = delegate
    let delegateClass: AnyClass = type(of: delegate)

    hook(
      delegateClass, Selector(("application:didRegisterForRemoteNotificationsWithDeviceToken:"))
    ) { arg in
      guard let data = arg as? Data else { return }
      let token = data.map { String(format: "%02x", $0) }.joined()
      DispatchQueue.main.async { PushRegistry.shared.resolveAll(token: token) }
    }
    hook(
      delegateClass, Selector(("application:didFailToRegisterForRemoteNotificationsWithError:"))
    ) { arg in
      let message = (arg as? NSError)?.localizedDescription ?? "Registration failed"
      DispatchQueue.main.async {
        PushRegistry.shared.rejectAll(message, code: "registrationFailed")
      }
    }

    // UIKit caches respondsToSelector for the delegate; re-assigning it flushes that cache.
    UIApplication.shared.delegate = nil
    UIApplication.shared.delegate = delegate
  }

  private func hook(_ cls: AnyClass, _ selector: Selector, _ handler: @escaping (AnyObject) -> Void)
  {
    let types = "v@:@@"
    let originalIMP = class_getInstanceMethod(cls, selector).map(method_getImplementation)

    let block: @convention(block) (AnyObject, UIApplication, AnyObject) -> Void = {
      obj, application, arg in
      handler(arg)
      if let originalIMP = originalIMP {
        let original = unsafeBitCast(originalIMP, to: DelegateIMP.self)
        original(obj, selector, application, arg)
      }
    }
    let imp = imp_implementationWithBlock(unsafeBitCast(block, to: AnyObject.self))

    if !class_addMethod(cls, selector, imp, types), let method = class_getInstanceMethod(cls, selector) {
      method_setImplementation(method, imp)
    }
  }

  func begin(_ invoke: Invoke) {
    pending.append(invoke)

    DispatchQueue.main.asyncAfter(deadline: .now() + 20) { [weak self] in
      self?.reject(
        invoke, "Timed out waiting for the APNs device token", code: "timeout")
    }

    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) {
      granted, error in
      DispatchQueue.main.async {
        if granted && error == nil {
          UIApplication.shared.registerForRemoteNotifications()
        } else {
          PushRegistry.shared.rejectAll(
            "Notification permission was denied", code: "permissionDenied")
        }
      }
    }
  }

  private func resolveAll(token: String) {
    let invokes = pending
    pending = []
    for invoke in invokes {
      invoke.resolve(["token": token])
    }
  }

  private func rejectAll(_ message: String, code: String) {
    let invokes = pending
    pending = []
    for invoke in invokes {
      invoke.reject(message, code: code)
    }
  }

  private func reject(_ invoke: Invoke, _ message: String, code: String) {
    guard let index = pending.firstIndex(where: { $0 === invoke }) else { return }
    pending.remove(at: index)
    invoke.reject(message, code: code)
  }
}

class PushPlugin: Plugin {
  public override func load(webview: WKWebView) {
    PushRegistry.shared.installDelegateHooks()
  }

  @objc func register(_ invoke: Invoke) {
    DispatchQueue.main.async { PushRegistry.shared.begin(invoke) }
  }
}

@_cdecl("init_plugin_pipedash_push")
public func initPlugin() -> Plugin {
  return PushPlugin()
}
