import AuthenticationServices
import Tauri
import UIKit

final class AuthSessionPresenter: NSObject, ASWebAuthenticationPresentationContextProviding {
  private let anchor: ASPresentationAnchor

  init(anchor: ASPresentationAnchor) {
    self.anchor = anchor
  }

  func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
    anchor
  }
}

extension NativePlugin {
  @objc func openAuthSession(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(AuthSessionArgs.self)
    DispatchQueue.main.async {
      guard let url = URL(string: args.url), url.scheme?.lowercased() == "https" else {
        invoke.reject("The sign-in page must be an HTTPS URL")
        return
      }
      guard let anchor = self.topViewController()?.view.window else {
        invoke.reject("Nothing is available to present the sign-in page")
        return
      }
      self.authSession?.cancel()
      self.finishAuthSession(cancelled: false)

      var started: ASWebAuthenticationSession?
      let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "pipedash") {
        [weak self] _, error in
        DispatchQueue.main.async {
          guard let self = self, self.authSession === started else { return }
          let cancelled =
            (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin
          self.finishAuthSession(cancelled: cancelled)
        }
      }
      started = session
      let presenter = AuthSessionPresenter(anchor: anchor)
      session.presentationContextProvider = presenter
      session.prefersEphemeralWebBrowserSession = false
      self.authSession = session
      self.authPresenter = presenter
      self.authInvoke = invoke
      if !session.start() {
        self.authSession = nil
        self.authPresenter = nil
        self.authInvoke = nil
        invoke.reject("Couldn't open the sign-in page")
      }
    }
  }

  @objc func closeAuthSession(_ invoke: Invoke) {
    DispatchQueue.main.async {
      self.authSession?.cancel()
      self.finishAuthSession(cancelled: false)
      invoke.resolve()
    }
  }

  private func finishAuthSession(cancelled: Bool) {
    authSession = nil
    authPresenter = nil
    let invoke = authInvoke
    authInvoke = nil
    invoke?.resolve(AuthSessionResult(cancelled: cancelled))
  }
}
