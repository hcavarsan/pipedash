import { Button, Checkbox, Portal } from '@mantine/core'
import { IconChevronRight } from '@tabler/icons-react'
import type { CSSProperties, ReactNode } from 'react'

import { STATUS_META } from '../../theme/status'
import type { PipelineRun } from '../../types'
import { formatDuration } from '../../utils/formatDuration'
import { formatRelativeTime } from '../../utils/formatRelativeTime'
import { getStatusLabel } from '../../utils/pipelinePresentation'
import { IosGroup } from '../ios/IosList'

import classes from './IosRunList.module.css'

interface IosRunListProps {
  runs: PipelineRun[]
  isFlux: boolean
  noun: string
  label: string
  onSelect?: () => void
  selecting: boolean
  selectedNumbers: number[]
  isSelectable: (run: PipelineRun) => boolean
  onToggle: (run: PipelineRun) => void
  onOpen: (run: PipelineRun) => void
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
  renderMenu,
}: IosRunListProps) => {
  const now = Date.now()

  return (
    <IosGroup
      header={
        <div className={classes.header}>
          <span>{label}</span>
          {onSelect && !selecting && (
            <button type="button" className={classes.headerAction} onClick={onSelect}>
              Select
            </button>
          )}
        </div>
      }
    >
      {runs.map((run) => {
        const selectable = selecting && isSelectable(run)
        const selected = selectable && selectedNumbers.includes(run.run_number)
        const name = isFlux ? String(run.metadata?.run_label ?? 'Reconciliation') : `#${run.run_number}`
        const message = isFlux
          ? String(run.metadata?.message ?? run.commit_message ?? '')
          : (run.commit_message?.split('\n')[0]?.trim() ?? '')
        const subtitle = isFlux ? '' : [run.branch, run.actor].filter(Boolean).join(' · ')
        const timing = [run.started_at && formatRelativeTime(run.started_at, now), formatDuration(run.duration_seconds)]
          .filter((part) => part && part !== '-')
          .join(' · ')
        const statusLabel = getStatusLabel(run) ?? STATUS_META[run.status].label

        return (
          <div
            key={run.run_number}
            className={classes.row}
            data-selecting={selecting ? '' : undefined}
            data-dimmed={selecting && !selectable ? '' : undefined}
          >
            <button
              type="button"
              className={classes.main}
              disabled={selecting && !selectable}
              aria-pressed={selecting ? selected : undefined}
              aria-label={selecting ? `Select ${noun} ${name}` : `View details for ${noun} ${name}, ${statusLabel}`}
              onClick={() => (selecting ? onToggle(run) : onOpen(run))}
            >
              {selecting && (
                <span className={classes.check} aria-hidden>
                  {selectable && <Checkbox checked={selected} readOnly tabIndex={-1} />}
                </span>
              )}
              <span
                className={classes.dot}
                style={{ '--dot-color': `var(--status-${STATUS_META[run.status].tone})` } as CSSProperties}
                role="img"
                aria-label={statusLabel}
              />
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
                {(isFlux ? message : subtitle) && (
                  <span className={classes.subtitle}>{isFlux ? message : subtitle}</span>
                )}
                {timing && <span className={classes.subtitle}>{timing}</span>}
              </span>
              {!selecting && (
                <span className={classes.chevron} aria-hidden>
                  <IconChevronRight size={18} />
                </span>
              )}
            </button>
            {!selecting && <div className={classes.more}>{renderMenu(run)}</div>}
          </div>
        )
      })}
    </IosGroup>
  )
}

interface IosRunToolbarProps {
  count: number
  cancelling: boolean
  onDone: () => void
  onCancelRuns: () => void
}

export const IosRunToolbar = ({ count, cancelling, onDone, onCancelRuns }: IosRunToolbarProps) => (
  <Portal>
    <div className={classes.toolbar} role="toolbar" aria-label="Selected runs">
      <Button variant="subtle" onClick={onDone}>
        Done
      </Button>
      <span className={classes.count}>{count === 0 ? 'Select runs' : `${count} selected`}</span>
      {count > 0 && (
        <Button variant="subtle" color="red" loading={cancelling} onClick={onCancelRuns}>
          {count > 1 ? `Cancel ${count} runs` : 'Cancel run'}
        </Button>
      )}
    </div>
  </Portal>
)
