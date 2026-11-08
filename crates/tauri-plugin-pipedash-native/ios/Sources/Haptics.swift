import UIKit

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
