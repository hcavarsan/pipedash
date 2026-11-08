import UIKit

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
