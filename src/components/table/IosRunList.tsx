import { Button, Portal } from '@mantine/core'
import { IconCheck, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useEffect, useId, useState } from 'react'

import { prefersReducedMotion } from '../../lib/motion'
import { animateSpring } from '../../lib/spring'
import { STATUS_META } from '../../theme/status'
import type { PipelineRun } from '../../types'
import { formatDuration } from '../../utils/formatDuration'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusGlyph } from '../atoms/StatusBadge'
import { IosGroup } from '../ios/IosList'
import { type SwipeAction, SwipeActions } from '../ios/SwipeActions'
import { useRowMenuLongPress } from '../ios/useLongPress'

import classes from './IosRunList.module.css'

interface IosRunRowProps {
  run: PipelineRun
  isFlux: boolean
  noun: string
  now: number
  selecting: boolean
  selectable: boolean
  selected: boolean
  onToggle: () => void
  onOpen: () => void
  swipeActions: SwipeAction[]
  menu: ReactNode
}

const IosRunRow = ({
  run,
  isFlux,
  noun,
  now,
  selecting,
  selectable,
  selected,
  onToggle,
  onOpen,
  swipeActions,
  menu,
}: IosRunRowProps) => {
  const hintId = useId()
  const pressRef = useRowMenuLongPress(!selecting)
  const name = isFlux ? String(run.metadata?.run_label ?? 'Reconciliation') : `#${run.run_number}`
  const message = isFlux
    ? String(run.metadata?.message ?? run.commit_message ?? '')
    : (run.commit_message?.split('\n')[0]?.trim() ?? '')
  const subtitle = isFlux ? '' : [run.branch, run.actor].filter(Boolean).join(' · ')
  const timing = [run.started_at && formatRelativeTime(run.started_at, now), formatDuration(run.duration_seconds)]
    .filter((part) => part && part !== '-')
    .join(' · ')
  const statusLabel = getStatusLabel(run) ?? STATUS_META[run.status].label
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
          {(isFlux ? message : subtitle) && <span className={classes.subtitle}>{isFlux ? message : subtitle}</span>}
          <span className={classes.meta}>
            <span className={classes.status} style={{ color: `var(--status-${STATUS_META[run.status].tone})` }}>
              {statusLabel}
            </span>
            {timing && ` · ${timing}`}
          </span>
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
      {!selecting && <div className={classes.more}>{menu}</div>}
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
  isFlux: boolean
  noun: string
  label: string
  onSelect?: (() => void) | undefined
  selecting: boolean
  selectedNumbers: number[]
  isSelectable: (run: PipelineRun) => boolean
  onToggle: (run: PipelineRun) => void
  onOpen: (run: PipelineRun) => void
  getSwipeActions: (run: PipelineRun) => SwipeAction[]
  renderMenu: (run: PipelineRun) => ReactNode
}

export const IosRunList = ({
  runs,
  isFlux,
  noun,
  label,
  onSelect,
  selecting,
  selectedNumbers,
  isSelectable,
  onToggle,
  onOpen,
  getSwipeActions,
  renderMenu,
}: IosRunListProps) => {
  const now = Date.now()

  return (
    <IosGroup
      header={label}
      headerAction={
        onSelect &&
        !selecting && (
          <button type="button" className={classes.headerAction} onClick={onSelect}>
            Select
          </button>
        )
      }
    >
      {runs.map((run) => {
        const selectable = selecting && isSelectable(run)

        return (
          <IosRunRow
            key={run.run_number}
            run={run}
            isFlux={isFlux}
            noun={noun}
            now={now}
            selecting={selecting}
            selectable={selectable}
            selected={selectable && selectedNumbers.includes(run.run_number)}
            onToggle={() => onToggle(run)}
            onOpen={() => onOpen(run)}
            swipeActions={getSwipeActions(run)}
            menu={renderMenu(run)}
          />
        )
      })}
    </IosGroup>
  )
}

export const IosToolbar = ({ label, children }: { label: string; children: ReactNode }) => {
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

export const IosRunToolbar = ({ count, cancelling, onDone, onCancelRuns }: IosRunToolbarProps) => (
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
