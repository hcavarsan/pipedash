import UIKit
import WebKit

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
