import ActivityKit
import PipedashActivityModels
import SwiftUI
import WidgetKit

@main
struct PipedashBuildActivityBundle: WidgetBundle {
  var body: some Widget {
    PipedashBuildActivity()
  }
}

struct PipedashBuildActivity: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: PipedashBuildActivityAttributes.self) { context in
      BuildLockScreenView(snapshot: context.state, isStale: context.isStale)
        .activityBackgroundTint(Color(uiColor: .secondarySystemBackground))
        .activitySystemActionForegroundColor(.primary)
        .widgetURL(URL(string: context.state.url))
    } dynamicIsland: { context in
      DynamicIsland {
        DynamicIslandExpandedRegion(.leading) {
          Label("Pipedash", systemImage: "point.3.connected.trianglepath.dotted")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
        }
        DynamicIslandExpandedRegion(.trailing) {
          Text("#\(context.state.runNumber)")
            .font(.caption.monospacedDigit().weight(.semibold))
            .foregroundStyle(.secondary)
        }
        DynamicIslandExpandedRegion(.center) {
          Text(context.state.pipelineName)
            .font(.headline)
            .lineLimit(2)
            .multilineTextAlignment(.center)
        }
        DynamicIslandExpandedRegion(.bottom) {
          BuildExpandedView(snapshot: context.state, isStale: context.isStale)
        }
      } compactLeading: {
        HStack(spacing: 4) {
          BuildStatusSymbol(snapshot: context.state, isStale: context.isStale)
            .font(.body.weight(.semibold))
          Text(context.state.pipelineName)
            .font(.caption2)
            .lineLimit(1)
        }
        .frame(maxWidth: 84, alignment: .leading)
        .accessibilityLabel("\(context.state.pipelineName), run \(context.state.runNumber), \(context.state.status.title)")
      } compactTrailing: {
        BuildElapsedView(snapshot: context.state, isStale: context.isStale)
          .font(.caption2.monospacedDigit())
          .frame(maxWidth: 54)
      } minimal: {
        BuildStatusSymbol(snapshot: context.state, isStale: context.isStale)
          .font(.body.weight(.semibold))
          .accessibilityLabel("\(context.state.pipelineName), run \(context.state.runNumber), \(context.state.status.title)")
      }
      .widgetURL(URL(string: context.state.url))
      .keylineTint(BuildActivityStyle.color(context.state, isStale: context.isStale))
    }
  }
}

private struct BuildLockScreenView: View {
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  let snapshot: BuildWatchSnapshot
  let isStale: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      if !dynamicTypeSize.isAccessibilitySize {
        HStack(alignment: .firstTextBaseline) {
          Label("Pipedash", systemImage: "point.3.connected.trianglepath.dotted")
            .font(.caption.weight(.semibold))
          Spacer(minLength: 8)
          Text(snapshot.monitoring == .watching && !isStale
            ? snapshot.providerName : BuildActivityStyle.monitoringLabel(snapshot, isStale: isStale))
            .font(.caption)
            .lineLimit(1)
        }
        .foregroundStyle(.secondary)
      }

      HStack(alignment: .top, spacing: 12) {
        BuildStatusSymbol(snapshot: snapshot, isStale: isStale)
          .font(.title2.weight(.medium))
          .accessibilityHidden(true)
        VStack(alignment: .leading, spacing: 4) {
          Text(snapshot.pipelineName)
            .font(.headline)
            .lineLimit(1)
          HStack(spacing: 6) {
            Text("#\(snapshot.runNumber)")
              .monospacedDigit()
            Text("·")
              .accessibilityHidden(true)
            Text(dynamicTypeSize.isAccessibilitySize && (isStale || snapshot.monitoring == .interrupted)
              ? BuildActivityStyle.monitoringLabel(snapshot, isStale: isStale) : snapshot.status.title)
              .lineLimit(1)
          }
          .font(.subheadline)
          .foregroundStyle(.secondary)
        }
        Spacer(minLength: 0)
        BuildElapsedView(snapshot: snapshot, isStale: isStale)
          .font(.subheadline.monospacedDigit().weight(.medium))
      }

      HStack(spacing: 10) {
        if !dynamicTypeSize.isAccessibilitySize {
          BuildBranchView(branch: snapshot.branch)
        }
        Spacer(minLength: 0)
        BuildRunLink(snapshot: snapshot)
      }
      .font(.caption)
    }
    .padding(12)
    .accessibilityElement(children: .contain)
  }
}

private struct BuildExpandedView: View {
  let snapshot: BuildWatchSnapshot
  let isStale: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 8) {
        BuildStatusSymbol(snapshot: snapshot, isStale: isStale)
          .accessibilityHidden(true)
        Text(snapshot.status.title)
          .fontWeight(.medium)
        Spacer(minLength: 8)
        BuildElapsedView(snapshot: snapshot, isStale: isStale)
          .monospacedDigit()
      }
      .font(.subheadline)

      HStack(alignment: .firstTextBaseline, spacing: 12) {
        VStack(alignment: .leading, spacing: 4) {
          BuildBranchView(branch: snapshot.branch)
          Text(snapshot.providerName)
            .lineLimit(1)
            .foregroundStyle(.secondary)
        }
        Spacer(minLength: 0)
        BuildRunLink(snapshot: snapshot)
      }
      .font(.caption)

      HStack(spacing: 4) {
        Text(BuildActivityStyle.monitoringLabel(snapshot, isStale: isStale))
        if snapshot.monitoring == .watching {
          Text("·")
          Text(Date(timeIntervalSince1970: TimeInterval(snapshot.updatedAt)), style: .time)
        }
      }
      .font(.caption2)
      .foregroundStyle(.secondary)
    }
    .padding(.top, 4)
  }
}

private struct BuildStatusSymbol: View {
  let snapshot: BuildWatchSnapshot
  let isStale: Bool

  var body: some View {
    Image(systemName: BuildActivityStyle.symbol(snapshot, isStale: isStale))
      .foregroundStyle(BuildActivityStyle.color(snapshot, isStale: isStale))
  }
}

private struct BuildElapsedView: View {
  let snapshot: BuildWatchSnapshot
  let isStale: Bool

  var body: some View {
    Group {
      if snapshot.monitoring == .watching && !isStale {
        Text(Date(timeIntervalSince1970: TimeInterval(snapshot.startedAt)), style: .timer)
      } else {
        Text(snapshot.elapsedLabel)
      }
    }
    .lineLimit(1)
    .minimumScaleFactor(0.75)
  }
}

private struct BuildBranchView: View {
  let branch: String?

  var body: some View {
    if let branch = branch, !branch.isEmpty {
      Label(branch, systemImage: "arrow.triangle.branch")
        .lineLimit(1)
        .foregroundStyle(.secondary)
        .accessibilityLabel("Branch \(branch)")
    }
  }
}

private struct BuildRunLink: View {
  let snapshot: BuildWatchSnapshot

  var body: some View {
    if let url = URL(string: snapshot.url) {
      Link(destination: url) {
        Label("Open run", systemImage: "arrow.up.right")
          .fontWeight(.semibold)
          .frame(minHeight: 44)
      }
      .accessibilityLabel("Open \(snapshot.pipelineName) run \(snapshot.runNumber) in Pipedash")
    }
  }
}

private enum BuildActivityStyle {
  static func symbol(_ snapshot: BuildWatchSnapshot, isStale: Bool) -> String {
    if snapshot.monitoring == .interrupted || (snapshot.monitoring == .watching && isStale) {
      return "exclamationmark.arrow.triangle.2.circlepath"
    }
    if snapshot.monitoring == .stopped { return "pause.circle" }
    return snapshot.status.symbol
  }

  static func color(_ snapshot: BuildWatchSnapshot, isStale: Bool) -> Color {
    if snapshot.monitoring == .interrupted || snapshot.monitoring == .stopped || isStale {
      return .secondary
    }
    switch snapshot.status {
    case .success: return .green
    case .failed: return .red
    case .running: return .accentColor
    case .pending, .cancelled, .skipped: return .secondary
    }
  }

  static func monitoringLabel(_ snapshot: BuildWatchSnapshot, isStale: Bool) -> String {
    switch snapshot.monitoring {
    case .watching: return isStale ? "Updates delayed · last checked" : "Last checked"
    case .completed: return "Build finished"
    case .interrupted: return "Monitoring interrupted · open Pipedash to resume"
    case .stopped: return "Monitoring stopped · build not cancelled"
    }
  }
}
