import UIKit
import WebKit
import XCTest

@testable import tauri_plugin_pipedash_native

final class SafeAreaBridgeTests: XCTestCase, WKNavigationDelegate {
  private var window: UIWindow!
  private var controller: UIViewController!
  private var webview: WKWebView!
  private var bridge: SafeAreaBridge!
  private var pageLoaded: (() -> Void)?

  override func setUp() {
    super.setUp()
    window = UIWindow(frame: UIScreen.main.bounds)
    controller = UIViewController()
    window.rootViewController = controller
    webview = WKWebView(frame: window.bounds)
    webview.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    webview.scrollView.contentInsetAdjustmentBehavior = .never
    webview.navigationDelegate = self
    controller.view.addSubview(webview)
    bridge = SafeAreaBridge(webview: webview)
    window.makeKeyAndVisible()
    controller.view.layoutIfNeeded()
  }

  override func tearDown() {
    bridge.detach()
    bridge = nil
    webview.navigationDelegate = nil
    webview = nil
    window.isHidden = true
    window = nil
    controller = nil
    pageLoaded = nil
    super.tearDown()
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    pageLoaded?()
  }

  private func assertInsets(_ expected: UIEdgeInsets, then completion: @escaping () -> Void) {
    webview.evaluateJavaScript("""
      ['top', 'right', 'bottom', 'left'].map(edge =>
        parseFloat(document.documentElement.style.getPropertyValue('--native-safe-area-' + edge)))
      """) { result, error in
      XCTAssertNil(error)
      XCTAssertEqual(result as? [Double], [expected.top, expected.right, expected.bottom, expected.left].map(Double.init))
      completion()
    }
  }

  func testDocumentReadinessLayoutChangesAndReloadKeepActualInsets() {
    let finished = expectation(description: "native insets reach each document and follow layout")
    pageLoaded = { [self] in
      assertInsets(webview.safeAreaInsets) { [self] in
        let original = webview.safeAreaInsets
        controller.additionalSafeAreaInsets = UIEdgeInsets(top: 11, left: 7, bottom: 13, right: 17)
        controller.view.layoutIfNeeded()
        XCTAssertGreaterThan(webview.safeAreaInsets.top, original.top)
        assertInsets(webview.safeAreaInsets) { [self] in
          pageLoaded = { [self] in
            assertInsets(webview.safeAreaInsets) {
              finished.fulfill()
            }
          }
          webview.loadHTMLString("<meta name='viewport' content='width=device-width,initial-scale=1,viewport-fit=cover'><body>Reload</body>", baseURL: nil)
        }
      }
    }
    webview.loadHTMLString("<meta name='viewport' content='width=device-width,initial-scale=1,viewport-fit=cover'><body>Ready</body>", baseURL: nil)
    wait(for: [finished], timeout: 10)
  }
}
