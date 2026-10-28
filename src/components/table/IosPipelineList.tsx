import { Loader, Menu, UnstyledButton } from '@mantine/core'
import { IconArrowDown, IconArrowUp, IconChevronRight } from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { type ReactNode, useId } from 'react'

import { useNow } from '../../hooks/useNow'
import { STATUS_META } from '../../theme/status'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusGlyph } from '../atoms/StatusBadge'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup } from '../ios/IosList'
import { type SwipeAction, SwipeActions } from '../ios/SwipeActions'
import { useRowMenuLongPress } from '../ios/useLongPress'

import classes from './IosPipelineList.module.css'
import { type TableRow, WorkflowActionMenu, workflowSwipeActions } from './PipelineRowParts'

interface IosPipelineRowProps {
  status?: PipelineStatus | null
  statusText?: string
  title: string
  subtitle: string
  detail?: string
  value?: string
  nested?: boolean
  expanded?: boolean
  hint?: string
  onClick: () => void
  trailing?: ReactNode
  swipeActions?: SwipeAction[]
  longPressMenu?: boolean
}

export function IosPipelineRow({
  status,
  statusText,
  title,
  subtitle,
  detail,
  value,
  nested,
  expanded,
  hint,
  onClick,
  trailing,
  swipeActions,
  longPressMenu,
}: IosPipelineRowProps) {
  const hintId = useId()
  const pressRef = useRowMenuLongPress(Boolean(longPressMenu))
  const statusLabel = status ? (statusText ?? STATUS_META[status].label) : undefined
  const body = (
    <>
      <button
        type="button"
        className={classes.main}
        onClick={onClick}
        aria-expanded={expanded}
        aria-describedby={hint ? hintId : undefined}
      >
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
        <span className={classes.chevron} data-open={expanded ? '' : undefined} aria-hidden>
          <IconChevronRight size={18} />
        </span>
      </button>
      {hint && (
        <span id={hintId} hidden>
          {hint}
        </span>
      )}
      {trailing && <div className={classes.trailing}>{trailing}</div>}
    </>
  )

  return (
    <div
      ref={pressRef}
      className={classes.row}
      data-repo={expanded === undefined ? undefined : ''}
      data-nested={nested ? '' : undefined}
      data-trailing={trailing ? '' : undefined}
      data-swipeable={swipeActions?.length ? '' : undefined}
    >
      {swipeActions?.length ? <SwipeActions actions={swipeActions}>{body}</SwipeActions> : body}
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

export const relativeOrNever = (value: string | null | undefined, now: number) =>
  value ? compactRelative(value, now) : 'Never'

interface SortMenuProps {
  sortStatus: DataTableSortStatus<TableRow>
  onSortStatusChange: (sortStatus: DataTableSortStatus<TableRow>) => void
  groupTitle: string
}

const DEFAULT_DIRECTION: Record<string, 'asc' | 'desc'> = { lastRun: 'desc' }

function SortMenu({ sortStatus, onSortStatusChange, groupTitle }: SortMenuProps) {
  const options = [
    { accessor: 'name', label: 'Name' },
    { accessor: 'lastRun', label: 'Last Run' },
    { accessor: 'status', label: 'Status' },
    { accessor: 'organization', label: groupTitle },
    { accessor: 'provider', label: 'Provider' },
  ]
  const current = options.find((option) => option.accessor === sortStatus.columnAccessor) ?? options[0]
  const ascending = sortStatus.direction === 'asc'
  const orders = [
    { direction: 'asc', label: current.accessor === 'lastRun' ? 'Oldest First' : 'Ascending' },
    { direction: 'desc', label: current.accessor === 'lastRun' ? 'Newest First' : 'Descending' },
  ] as const
  const DirectionIcon = ascending ? IconArrowUp : IconArrowDown

  return (
    <Menu position="bottom">
      <Menu.Target>
        <UnstyledButton
          className={classes.sort}
          aria-label={`Sort by ${current.label}, ${ascending ? 'ascending' : 'descending'}`}
        >
          {current.label}
          <DirectionIcon size={14} aria-hidden />
        </UnstyledButton>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Label>Sort by</Menu.Label>
        {options.map(({ accessor, label }) => {
          const active = accessor === current.accessor

          return (
            <Menu.Item
              key={accessor}
              role="menuitemradio"
              aria-checked={active}
              onClick={() =>
                onSortStatusChange({
                  columnAccessor: accessor,
                  direction: active ? sortStatus.direction : (DEFAULT_DIRECTION[accessor] ?? 'asc'),
                })
              }
            >
              {active ? `${label} ✓` : label}
            </Menu.Item>
          )
        })}
        <Menu.Divider />
        {orders.map(({ direction, label }) => {
          const active = sortStatus.direction === direction

          return (
            <Menu.Item
              key={direction}
              role="menuitemradio"
              aria-checked={active}
              onClick={() => onSortStatusChange({ columnAccessor: current.accessor, direction })}
            >
              {active ? `${label} ✓` : label}
            </Menu.Item>
          )
        })}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}

interface IosPipelineListProps {
  header?: ReactNode
  groupTitle: string
  sortStatus: DataTableSortStatus<TableRow>
  onSortStatusChange: (sortStatus: DataTableSortStatus<TableRow>) => void
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  pinnedKeys: string[]
  onTogglePin: (pinKey: string) => void
  hiddenKeys: string[]
  onToggleHidden: (pinKey: string) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function IosPipelineList({
  header,
  groupTitle,
  sortStatus,
  onSortStatusChange,
  rows,
  getProvider,
  onToggleRepo,
  onViewHistory,
  onTrigger,
  pinnedKeys,
  onTogglePin,
  hiddenKeys,
  onToggleHidden,
  onViewMetrics,
}: IosPipelineListProps) {
  const now = useNow(30_000)

  return (
    <IosGroup
      header={header}
      headerAction={
        <SortMenu sortStatus={sortStatus} onSortStatusChange={onSortStatusChange} groupTitle={groupTitle} />
      }
    >
      {rows.map((row) => {
        if (row.type === 'repository') {
          const isFluxGroup =
            !!row.providerIds?.size &&
            Array.from(row.providerIds).every((id) => getProvider(id)?.provider_type === 'fluxcd')
          const noun = isFluxGroup ? 'resource' : 'workflow'
          const count = `${row.pipelineCount} ${noun}${row.pipelineCount === 1 ? '' : 's'}`

          return (
            <IosPipelineRow
              key={row.id}
              status={row.status}
              statusText={row.statusLabel}
              title={row.name}
              subtitle={Array.from(new Set([row.organization, row.providerLabel, count].filter(Boolean))).join(' · ')}
              value={relativeOrNever(row.lastRun, now)}
              expanded={row.expanded}
              onClick={() => onToggleRepo(row.id)}
              trailing={
                <span className={classes.spacer}>
                  {row.hasPendingFetch && <Loader size="sm" aria-label="Fetching pipelines" />}
                </span>
              }
            />
          )
        }

        const { pipeline, pinKey } = row

        if (!pipeline || !pinKey) {
          return null
        }

        const isFlux = pipeline.provider_type === 'fluxcd'
        const message = isFlux && pipeline.metadata?.message ? String(pipeline.metadata.message) : undefined

        return (
          <IosPipelineRow
            key={row.id}
            nested
            status={row.status}
            statusText={getStatusLabel(pipeline)}
            title={row.name}
            subtitle={[row.organization, pipeline.branch].filter(Boolean).join(' · ')}
            detail={message}
            value={relativeOrNever(row.lastRun, now)}
            hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
            longPressMenu
            onClick={() => onViewHistory(pipeline)}
            swipeActions={workflowSwipeActions(
              pipeline,
              pinnedKeys.includes(pinKey),
              () => onTogglePin(pinKey),
              onTrigger
            )}
            trailing={
              <WorkflowActionMenu
                pipeline={pipeline}
                provider={getProvider(pipeline.provider_id)}
                isPinned={pinnedKeys.includes(pinKey)}
                onTogglePin={() => onTogglePin(pinKey)}
                isHidden={hiddenKeys.includes(pinKey)}
                onToggleHidden={() => onToggleHidden(pinKey)}
                onTrigger={onTrigger}
                onViewMetrics={onViewMetrics}
                onViewHistory={onViewHistory}
              />
            }
          />
        )
      })}
    </IosGroup>
  )
}
