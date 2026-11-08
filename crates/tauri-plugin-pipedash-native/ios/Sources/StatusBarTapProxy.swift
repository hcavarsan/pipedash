import UIKit

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
