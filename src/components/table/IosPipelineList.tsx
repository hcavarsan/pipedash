import { Loader } from '@mantine/core'
import { IconCheck, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'

import { useNow } from '../../hooks/useNow'
import { STATUS_META } from '../../theme/status'
import type { PipelineStatus, ProviderSummary } from '../../types'
import { staleNote } from '../../utils/fetchInfo'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusGlyph } from '../atoms/StatusBadge'
import { IosGroup } from '../ios/IosList'
import { type SwipeAction, SwipeActions } from '../ios/SwipeActions'
import { useRowMenuLongPress } from '../ios/useLongPress'

import classes from './IosPipelineList.module.css'
import runClasses from './IosRunList.module.css'
import {
  type PipelineActions,
  type PipelineSelection,
  RowActionMenu,
  RowFetchInfo,
  refreshItem,
  rowMenuItems,
  type TableRow,
  WorkflowActionMenu,
  workflowSwipeActions,
} from './PipelineRowParts'

interface IosPipelineRowProps {
  status?: PipelineStatus | null | undefined
  statusText?: string | undefined
  title: string
  subtitle: string
  detail?: string | undefined
  value?: string | undefined
  depth?: number | undefined
  expanded?: boolean | undefined
  hint?: string | undefined
  onClick: () => void
  trailing?: ReactNode
  swipeActions?: SwipeAction[] | undefined
  longPressMenu?: boolean | undefined
  selected?: boolean | undefined
}

export function IosPipelineRow({
  status,
  statusText,
  title,
  subtitle,
  detail,
  value,
  depth,
  expanded,
  hint,
  onClick,
  trailing,
  swipeActions,
  longPressMenu,
  selected,
}: IosPipelineRowProps): ReactNode {
  const hintId = useId()
  const selecting = selected !== undefined
  const pressRef = useRowMenuLongPress(Boolean(longPressMenu) && !selecting)
  const statusLabel = status ? (statusText ?? STATUS_META[status].label) : undefined
  const body = (
    <>
      <button
        type="button"
        className={classes.main}
        onClick={onClick}
        aria-expanded={expanded}
        aria-pressed={selected}
        aria-describedby={hint ? hintId : undefined}
      >
        {selecting && (
          <span className={runClasses.check} data-selected={selected || undefined} aria-hidden>
            {selected && <IconCheck size={16} stroke={3} />}
          </span>
        )}
        {status ? <StatusGlyph status={status} /> : <span className={classes.glyphEmpty} aria-hidden />}
        <span className={classes.text}>
          <span className={classes.title}>{title}</span>
          <span className={classes.subtitle}>
            {statusLabel && status && (
              <>
                <span className={classes.status} style={{ color: `var(--status-${STATUS_META[status].tone})` }}>
                  {statusLabel}
                </span>
                {subtitle && ' · '}
              </>
            )}
            {subtitle}
          </span>
          {detail && <span className={classes.detail}>{detail}</span>}
        </span>
        {value && <span className={classes.value}>{value}</span>}
        {!selecting && (
          <span className={classes.chevron} data-open={expanded ? '' : undefined} aria-hidden>
            <IconChevronRight size={18} />
          </span>
        )}
      </button>
      {hint && (
        <span id={hintId} hidden>
          {hint}
        </span>
      )}
      {trailing && !selecting && <div className={classes.trailing}>{trailing}</div>}
    </>
  )

  return (
    <div
      ref={pressRef}
      className={classes.row}
      data-repo={expanded === undefined ? undefined : ''}
      data-nested={depth ? '' : undefined}
      style={depth ? { '--level': depth } : undefined}
      data-selecting={selecting ? '' : undefined}
      data-trailing={trailing && !selecting ? '' : undefined}
      data-swipeable={swipeActions?.length && !selecting ? '' : undefined}
    >
      {swipeActions?.length && !selecting ? <SwipeActions actions={swipeActions}>{body}</SwipeActions> : body}
    </div>
  )
}

const NARROW_FORMAT = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto', style: 'narrow' })

const NARROW_UNITS = [
  ['day', 86_400_000],
  ['hour', 3_600_000],
  ['minute', 60_000],
] as const

const compactRelative = (value: string, now: number) => {
  const diff = new Date(value).getTime() - now
  const unit = NARROW_UNITS.find(([, size]) => Math.abs(diff) >= size)

  return unit ? NARROW_FORMAT.format(Math.round(diff / unit[1]), unit[0]) : NARROW_FORMAT.format(0, 'second')
}

export const relativeOrNever = (value: string | null | undefined, now: number): string =>
  value ? compactRelative(value, now) : 'Never'

interface IosPipelineListProps {
  header?: ReactNode
  headerAction?: ReactNode
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  actions: PipelineActions
  selection?: PipelineSelection | undefined
  onSelect?: (() => void) | undefined
}

export function IosPipelineList({
  header,
  headerAction,
  rows,
  getProvider,
  onToggleRepo,
  actions,
  selection,
  onSelect,
}: IosPipelineListProps): ReactNode {
  const now = useNow(30_000)

  return (
    <IosGroup
      header={header}
      headerAction={
        <>
          {headerAction}
          {onSelect && (
            <button type="button" className={runClasses.headerAction} onClick={onSelect}>
              Select
            </button>
          )}
        </>
      }
    >
      {rows.map((row) => {
        if (row.type === 'section') {
          const isFluxGroup =
            !!row.providerIds?.size &&
            Array.from(row.providerIds).every((id) => getProvider(id)?.provider_type === 'fluxcd')
          const noun = isFluxGroup ? 'resource' : 'workflow'
          const count = `${row.pipelineCount} ${noun}${row.pipelineCount === 1 ? '' : 's'}`

          return (
            <IosPipelineRow
              key={row.id}
              title={row.name}
              subtitle={Array.from(new Set([row.organization, row.providerLabel, count].filter(Boolean))).join(' · ')}
              value={relativeOrNever(row.lastRun, now)}
              expanded={row.expanded}
              onClick={() => onToggleRepo(row.id)}
              longPressMenu
              trailing={
                <span className={classes.spacer}>
                  <RowFetchInfo row={row} getProvider={getProvider} />
                  {row.hasPendingFetch && <Loader size="sm" aria-label="Fetching pipelines" />}
                  <RowActionMenu
                    name={row.name}
                    items={[refreshItem(Array.from(row.providerIds ?? []), actions), ...rowMenuItems(row, actions)]}
                  />
                </span>
              }
            />
          )
        }

        const { pipeline } = row

        if (!pipeline) {
          return null
        }

        const isFlux = pipeline.provider_type === 'fluxcd'
        const message = isFlux && pipeline.metadata?.message ? String(pipeline.metadata.message) : undefined
        const provider = getProvider(pipeline.provider_id)

        return (
          <IosPipelineRow
            key={row.id}
            depth={row.depth}
            status={row.status}
            statusText={getStatusLabel(pipeline)}
            title={row.name}
            subtitle={[row.organization, pipeline.branch].filter(Boolean).join(' · ')}
            detail={[provider && staleNote(provider, now), message].filter(Boolean).join(' · ') || undefined}
            value={relativeOrNever(row.lastRun, now)}
            hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
            longPressMenu
            selected={selection ? selection.selected.has(pipeline.id) : undefined}
            onClick={() => (selection ? selection.onToggle(pipeline) : actions.onViewHistory(pipeline))}
            swipeActions={workflowSwipeActions(pipeline, actions)}
            trailing={<WorkflowActionMenu pipeline={pipeline} provider={provider} actions={actions} />}
          />
        )
      })}
    </IosGroup>
  )
}
