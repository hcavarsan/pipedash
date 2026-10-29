import { Button, Portal } from '@mantine/core'
import { IconCheck, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useEffect, useId, useState } from 'react'
import { RunActionMenu } from '../../features/runs/components/RunActionControls'
import { useRunActions } from '../../features/runs/components/RunActionsContext'
import { useRunActionItems } from '../../features/runs/hooks/useRunActionItems'
import { swipeActionsOf } from '../../features/runs/lib/runActions'
import { useRowMenuLongPress } from '../../hooks/useLongPress'
import { useNow } from '../../hooks/useNow'
import { prefersReducedMotion } from '../../lib/motion'
import { animateSpring } from '../../lib/spring'
import { STATUS_META } from '../../theme/status'
import type { PipelineRun } from '../../types/domain'
import { formatDuration } from '../../utils/formatDuration'
import { getRunStatusLabel } from '../../utils/pipelinePresentation'
import { formatLiveRunDuration, isLiveRun } from '../../utils/runTiming'
import { StatusGlyph } from '../atoms/StatusBadge'
import { IosGroup } from '../ios/IosList'
import { SwipeActions } from '../ios/SwipeActions'
import { relativeOrNever } from './IosPipelineList'
import classes from './IosRunList.module.css'
import { VirtualList } from './VirtualList'

interface IosRunRowProps {
  run: PipelineRun
  isFlux: boolean
  noun: string
  now: number
  selecting: boolean
  selectable: boolean
  selected: boolean
  onToggle: () => void
}

const IosRunRow = ({ run, isFlux, noun, now, selecting, selectable, selected, onToggle }: IosRunRowProps) => {
  const { openRun, pipeline } = useRunActions()
  const swipeActions = swipeActionsOf(useRunActionItems(run))
  const onOpen = () => openRun(run)
  const hintId = useId()
  const pressRef = useRowMenuLongPress(!selecting)
  const name = isFlux ? String(run.metadata?.run_label ?? 'Reconciliation') : `#${run.run_number}`
  const message = isFlux
    ? String(run.metadata?.message ?? run.commit_message ?? '')
    : (run.commit_message?.split('\n')[0]?.trim() ?? '')
  const timing = [
    run.started_at && relativeOrNever(run.started_at, now),
    isLiveRun(run)
      ? formatLiveRunDuration(run, now, pipeline?.typical_duration_seconds)
      : run.duration_seconds !== null && formatDuration(run.duration_seconds),
  ]
  const subtitle = [...timing, ...(isFlux ? [message] : [run.branch, run.actor])].filter(Boolean).join(' · ')
  const statusLabel = getRunStatusLabel(run) ?? STATUS_META[run.status].label
  const swipeable = !selecting && swipeActions.length > 0

  const body = (
    <>
      <button
        type="button"
        className={classes.main}
        disabled={selecting && !selectable}
        aria-pressed={selecting ? selected : undefined}
        aria-describedby={hintId}
        onClick={selecting ? onToggle : onOpen}
      >
        {selecting && (
          <span
            className={classes.check}
            data-selected={selected || undefined}
            data-disabled={!selectable || undefined}
            aria-hidden
          >
            {selected && <IconCheck size={16} stroke={3} />}
          </span>
        )}
        <StatusGlyph status={run.status} />
        <span className={classes.text}>
          <span className={classes.title}>
            {isFlux ? (
              name
            ) : (
              <>
                <span className={classes.number}>{name}</span>
                {message && ` ${message}`}
              </>
            )}
          </span>
          <span className={classes.srOnly}>, {statusLabel}</span>
          {subtitle && <span className={classes.subtitle}>{subtitle}</span>}
        </span>
        {!selecting && (
          <span className={classes.chevron} aria-hidden>
            <IconChevronRight size={18} />
          </span>
        )}
      </button>
      <span id={hintId} hidden>
        {selecting ? `Select ${noun}` : `View details for ${noun}`}
      </span>
      {!selecting && <RunActionMenu run={run} />}
    </>
  )

  return (
    <div
      ref={pressRef}
      className={classes.row}
      data-selecting={selecting ? '' : undefined}
      data-dimmed={selecting && !selectable ? '' : undefined}
      data-swipeable={swipeable ? '' : undefined}
    >
      {swipeable ? <SwipeActions actions={swipeActions}>{body}</SwipeActions> : body}
    </div>
  )
}

interface IosRunListProps {
  runs: PipelineRun[]
  label: string
  selecting: boolean
  selectedNumbers: number[]
  isSelectable: (run: PipelineRun) => boolean
  onToggle: (run: PipelineRun) => void
  onEndReached?: (() => void) | undefined
}

export const IosRunList = ({
  runs,
  label,
  selecting,
  selectedNumbers,
  isSelectable,
  onToggle,
  onEndReached,
}: IosRunListProps): ReactNode => {
  const now = useNow(runs.some(isLiveRun) ? 1000 : 30_000)
  const { isFlux, labels } = useRunActions()

  return (
    <IosGroup header={label}>
      <VirtualList
        items={runs}
        getKey={(run) => String(run.run_number)}
        estimateSize={58}
        onEndReached={onEndReached}
        renderItem={(run) => {
          const selectable = selecting && isSelectable(run)

          return (
            <IosRunRow
              run={run}
              isFlux={isFlux}
              noun={labels.run}
              now={now}
              selecting={selecting}
              selectable={selectable}
              selected={selectable && selectedNumbers.includes(run.run_number)}
              onToggle={() => onToggle(run)}
            />
          )
        }}
      />
    </IosGroup>
  )
}

export const IosToolbar = ({ label, children }: { label: string; children: ReactNode }): ReactNode => {
  const [toolbar, ref] = useState<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!toolbar) {
      return
    }
    const tabBar = document.querySelector<HTMLElement>('nav[aria-label="Main"]')

    tabBar?.setAttribute('data-editing', '')
    if (tabBar) {
      tabBar.inert = true
      tabBar.setAttribute('aria-hidden', 'true')
    }

    const animation = animateSpring({
      from: 0,
      to: 1,
      onUpdate: (value) => {
        toolbar.style.opacity = String(value)
        toolbar.style.transform = prefersReducedMotion() ? '' : `translateY(${(1 - value) * 24}px)`
      },
    })

    return () => {
      animation.stop()
      tabBar?.removeAttribute('data-editing')
      if (tabBar && !tabBar.hasAttribute('data-hidden')) {
        tabBar.inert = false
        tabBar.removeAttribute('aria-hidden')
      }
    }
  }, [toolbar])

  return (
    <Portal>
      <div ref={ref} className={classes.toolbar} role="toolbar" aria-label={label}>
        {children}
      </div>
    </Portal>
  )
}

interface IosRunToolbarProps {
  count: number
  cancelling: boolean
  onDone: () => void
  onCancelRuns: () => void
}

export const IosRunToolbar = ({ count, cancelling, onDone, onCancelRuns }: IosRunToolbarProps): ReactNode => (
  <IosToolbar label="Selected runs">
    <Button variant="subtle" onClick={onDone}>
      Done
    </Button>
    <span className={classes.count} role="status">
      {count === 0 ? 'Select runs' : `${count} selected`}
    </span>
    {count > 0 && (
      <Button variant="subtle" color="red" loading={cancelling} onClick={onCancelRuns}>
        {count > 1 ? `Cancel ${count} runs` : 'Cancel run'}
      </Button>
    )}
  </IosToolbar>
)
