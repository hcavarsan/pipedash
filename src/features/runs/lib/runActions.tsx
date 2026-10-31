import { IconCheck, IconEye, IconFileText, IconPlayerPlay, IconSquare, IconX } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import type { SwipeAction } from '../../../components/ios/swipeAction'
import type { Pipeline, PipelineRun, ReviewDecision } from '../../../types/domain'
import type { PipelineLabels } from '../../../utils/pipelinePresentation'
import { isDesktopTauri } from '../../../utils/platform'
import type { RerunMode } from '../../../utils/rerunSummary'

type RunActionId = 'watch' | 'approve' | 'reject' | 'cancel' | 'rerun' | 'details'

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
  pipeline: Pipeline | null
  labels: PipelineLabels
  isFlux: boolean
  canRerun: boolean
  canCancel: boolean
  cancellingRun: number | null
  reviewingRun: number | null
  rerunningFailedRun: number | null
  rerunningRuns: ReadonlySet<number>
  watch: { supported: boolean; available: boolean; reason: string | undefined; busy: boolean; disabled: boolean }
  startWatch: (runNumber: number) => void
  openRun: (run: PipelineRun) => void
  openTrigger: () => void
  cancel: (run: PipelineRun) => Promise<void>
  review: (run: PipelineRun, decision: ReviewDecision) => Promise<void>
  requestRerun: (run: PipelineRun) => void
  rerunTarget: PipelineRun | null
  closeRerun: () => void
  confirmRerun: (mode: RerunMode) => Promise<void>
  triggerBlockedReason: string | undefined
}

export function buildRunActions(value: RunActionsValue, run: PipelineRun): RunAction[] {
  const { pipeline } = value

  if (!pipeline) {
    return []
  }

  const number = run.run_number
  const active = run.status === 'running' || run.status === 'pending'
  const awaitingReview = run.metadata?.awaiting_review === true
  const reviewBusy = value.reviewingRun !== null
  const actions: RunAction[] = []

  if (active && value.watch.supported) {
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

  if (awaitingReview) {
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

  if (active && value.canCancel && !awaitingReview) {
    const cancelling = value.cancellingRun === number

    actions.push({
      id: 'cancel',
      label: run.status === 'pending' ? `Cancel unavailable for pending run #${number}` : `Cancel run #${number}`,
      icon: <IconSquare size={16} aria-hidden />,
      color: 'red',
      disabled: cancelling || run.status === 'pending',
      pending: cancelling,
      swipe: { label: 'Cancel', tone: 'red', primary: true },
      execute: () => value.cancel(run),
    })
  } else if (!active && value.canRerun) {
    const busy = value.rerunningRuns.has(number) || value.rerunningFailedRun === number

    actions.push({
      id: 'rerun',
      label: value.isFlux ? `Reconcile ${pipeline.name}` : `Rerun run #${number}`,
      icon: <IconPlayerPlay size={16} aria-hidden />,
      disabled: busy,
      pending: busy,
      swipe: { label: value.isFlux ? 'Reconcile' : 'Rerun', tone: 'blue', primary: true },
      execute: () => value.requestRerun(run),
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
