import Tauri
import UIKit
import WebKit

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

extension NativePlugin {
  func topViewController() -> UIViewController? {
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
