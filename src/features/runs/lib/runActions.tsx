import { IconCheck, IconEye, IconFileText, IconPlayerPlay, IconSquare, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import type { SwipeAction } from '../../../components/ios/swipeAction'
import type { BuildWatchStatus } from '../../../types/notifications'
import type { Pipeline, PipelineRun, ReviewDecision } from '../../../types/pipelines'
import type { PluginPresentation } from '../../../types/plugin_presentation'
import type { PipelineTerminology } from '../../../types/presentation'
import { isDesktopTauri } from '../../../utils/platform'

export type RerunMode = 'all' | 'failed'

type RunActionId = 'watch' | 'approve' | 'reject' | 'cancel' | 'force-cancel' | 'rerun' | 'details'

export interface RunAction {
  id: RunActionId
  label: string
  icon: ReactNode
  disabled: boolean
  pending: boolean
  color?: 'red' | 'green' | undefined
  swipe?: { label: string; tone: SwipeAction['tone']; primary?: boolean | undefined } | undefined
  execute: () => void | Promise<void>
}

export interface RunActionsValue {
  pipeline: Pipeline
  presentation: PluginPresentation
  labels: PipelineTerminology
  canCancel: boolean
  cancellingRun: number | null
  forceCancellingRun: number | null
  reviewingRun: number | null
  rerunningFailedRun: number | null
  rerunningRuns: ReadonlySet<number>
  watch: { supported: boolean; available: boolean; reason: string | undefined; busy: boolean; disabled: boolean }
  startWatch: (runNumber: number) => void
  openRun: (run: PipelineRun) => void
  openTrigger: () => void
  cancel: (run: PipelineRun) => Promise<void>
  forceCancel: (run: PipelineRun) => Promise<void>
  review: (run: PipelineRun, decision: ReviewDecision) => Promise<void>
  requestRerun: (run: PipelineRun) => void
  rerunTarget: PipelineRun | null
  closeRerun: () => void
  confirmRerun: (mode: RerunMode) => Promise<void>
  triggerBlockedReason: string | undefined
}

export function buildRunActions(value: RunActionsValue, run: PipelineRun): RunAction[] {
  const { presentation } = value
  const number = run.run_number
  const state = run.actions
  const reviewBusy = value.reviewingRun !== null
  const actions: RunAction[] = []

  if (state.watch && value.watch.supported) {
    actions.push({
      id: 'watch',
      label: value.watch.available
        ? `Watch run #${number}`
        : `Watch unavailable: ${
            value.watch.reason ??
            (isDesktopTauri()
              ? 'Open General settings to check monitoring availability.'
              : 'Allow Live Activities and background execution in iOS Settings.')
          }`,
      icon: <IconEye size={16} aria-hidden />,
      disabled: value.watch.disabled,
      pending: value.watch.busy,
      execute: () => value.startWatch(number),
    })
  }

  if (state.review.allowed) {
    actions.push(
      {
        id: 'approve',
        label: `Approve deployment review for run #${number}`,
        icon: <IconCheck size={16} aria-hidden />,
        color: 'green',
        disabled: reviewBusy,
        pending: value.reviewingRun === number,
        swipe: { label: 'Approve', tone: 'green' },
        execute: () => value.review(run, 'approve'),
      },
      {
        id: 'reject',
        label: `Reject deployment review for run #${number}`,
        icon: <IconX size={16} aria-hidden />,
        color: 'red',
        disabled: reviewBusy,
        pending: value.reviewingRun === number,
        execute: () => value.review(run, 'reject'),
      }
    )
  }

  if (state.cancel.allowed) {
    const cancelling = value.cancellingRun === number

    actions.push({
      id: 'cancel',
      label: `Cancel run #${number}`,
      icon: <IconSquare size={16} aria-hidden />,
      color: 'red',
      disabled: cancelling,
      pending: cancelling,
      swipe: { label: 'Cancel', tone: 'red', primary: true },
      execute: () => value.cancel(run),
    })
  } else if (state.rerun.allowed) {
    const busy = value.rerunningRuns.has(number) || value.rerunningFailedRun === number

    actions.push({
      id: 'rerun',
      label: `${presentation.rerun.label} #${number}`,
      icon: <IconPlayerPlay size={16} aria-hidden />,
      disabled: busy,
      pending: busy,
      swipe: { label: presentation.rerun.compact_label, tone: 'blue', primary: true },
      execute: () => value.requestRerun(run),
    })
  }

  if (state.force_cancel.allowed) {
    actions.push({
      id: 'force-cancel',
      label: `Force cancel run #${number}`,
      icon: <IconSquare size={16} aria-hidden />,
      color: 'red',
      disabled: value.forceCancellingRun === number || value.cancellingRun === number,
      pending: value.forceCancellingRun === number,
      execute: () => value.forceCancel(run),
    })
  }

  actions.push({
    id: 'details',
    label: `View details for ${value.labels.run} #${number}`,
    icon: <IconFileText size={16} aria-hidden />,
    disabled: false,
    pending: false,
    execute: () => value.openRun(run),
  })

  return actions
}

interface WatchSource {
  data: BuildWatchStatus | undefined
  busy: boolean
  isFetching: boolean
  isError: boolean
}

export function watchStateOf(buildWatch: WatchSource): RunActionsValue['watch'] {
  const watching = buildWatch.data?.watch?.monitoring === 'watching'

  return {
    supported: !!buildWatch.data?.supported,
    available: !!buildWatch.data?.available,
    reason: buildWatch.data?.reason ?? undefined,
    busy: buildWatch.busy,
    disabled: buildWatch.busy || buildWatch.isFetching || buildWatch.isError || !buildWatch.data?.available || watching,
  }
}

export function swipeActionsOf(actions: RunAction[]): SwipeAction[] {
  return actions.flatMap((action) =>
    action.swipe && !action.disabled
      ? [
          {
            key: action.id,
            label: action.swipe.label,
            tone: action.swipe.tone,
            ...(action.swipe.primary ? { primary: true } : {}),
            onSelect: () => void action.execute(),
          },
        ]
      : []
  )
}
