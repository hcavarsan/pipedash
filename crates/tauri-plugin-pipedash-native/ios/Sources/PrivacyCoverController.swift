import UIKit

final class PrivacyCoverController {
  static let shared = PrivacyCoverController()

  private var armed = false
  private var holdForFrontend = false
  private var awaitingFrontend = false
  private var cover: UIView?
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

    let view = UIView(frame: window.bounds)
    view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    view.backgroundColor = .systemGroupedBackground
    view.accessibilityViewIsModal = true


    window.addSubview(view)
    cover = view
  }

  private func hide() {
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
    if !awaitingFrontend {
      hide()
    }
  }
}
