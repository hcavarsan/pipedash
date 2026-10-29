import { Loader } from '@mantine/core'
import { IconCheck, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'

import { useRowMenuLongPress } from '../../hooks/useLongPress'
import { useNow } from '../../hooks/useNow'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { STATUS_META } from '../../theme/status'
import type { PipelineStatus, ProviderSummary } from '../../types/domain'
import { staleNote } from '../../utils/fetchInfo'
import { getPipelineLabels, getStatusLabel } from '../../utils/pipelinePresentation'
import { StatusGlyph } from '../atoms/StatusBadge'
import groupClasses from '../ios/IosList.module.css'
import { SwipeActions } from '../ios/SwipeActions'
import type { SwipeAction } from '../ios/swipeAction'
import classes from './IosPipelineList.module.css'
import {
  type PipelineActions,
  type PipelineSelection,
  RowActionMenu,
  refreshItem,
  rowMenuItems,
  type TableRow,
  WorkflowActionMenu,
  workflowSwipeActions,
} from './PipelineRowParts'
import { VirtualList } from './VirtualList'

interface IosPipelineRowProps {
  status?: PipelineStatus | null | undefined
  statusText?: string | undefined
  title: string
  subtitle: string
  hint?: string | undefined
  onClick: () => void
  menu?: ReactNode
  swipeActions?: SwipeAction[] | undefined
  longPressMenu?: boolean | undefined
  selected?: boolean | undefined
}

export function IosPipelineRow({
  status,
  statusText,
  title,
  subtitle,
  hint,
  onClick,
  menu,
  swipeActions,
  longPressMenu,
  selected,
}: IosPipelineRowProps): ReactNode {
  const hintId = useId()
  const selecting = selected !== undefined
  const pressRef = useRowMenuLongPress(Boolean(longPressMenu) && !selecting)
  const swipeable = Boolean(swipeActions?.length) && !selecting
  const body = (
    <>
      <button
        type="button"
        className={classes.main}
        onClick={onClick}
        aria-pressed={selected}
        aria-describedby={hint ? hintId : undefined}
      >
        {selecting && (
          <span className={classes.selectCheck} data-selected={selected || undefined} aria-hidden>
            {selected && <IconCheck size={16} stroke={3} />}
          </span>
        )}
        {status && <StatusGlyph status={status} />}
        <span className={classes.text}>
          <span className={classes.title}>{title}</span>
          {status && <span className={classes.srOnly}>, {statusText ?? STATUS_META[status].label}</span>}
          {subtitle && <span className={classes.subtitle}>{subtitle}</span>}
        </span>
        {!selecting && (
          <span className={classes.chevron} aria-hidden>
            <IconChevronRight size={18} />
          </span>
        )}
      </button>
      {hint && (
        <span id={hintId} hidden>
          {hint}
        </span>
      )}
      {!selecting && menu}
    </>
  )

  return (
    <div
      ref={pressRef}
      className={classes.row}
      data-glyph={status ? '' : undefined}
      data-selecting={selecting ? '' : undefined}
      data-swipeable={swipeable ? '' : undefined}
    >
      {swipeable && swipeActions ? <SwipeActions actions={swipeActions}>{body}</SwipeActions> : body}
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

export function staleFooter(
  providerIds: Iterable<number>,
  getProvider: (providerId: number) => ProviderSummary | undefined,
  now: number
): string | undefined {
  const notes = Array.from(new Set(providerIds)).flatMap((id) => {
    const provider = getProvider(id)
    const note = provider && staleNote(provider, now)

    return provider && note ? [`${provider.name}: ${note}`] : []
  })

  return notes.length > 0 ? notes.join('\n') : undefined
}

const ROW_ESTIMATE = 58

interface IosPipelineListProps {
  header?: ReactNode
  rows: TableRow[]
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

type SectionTableRow = Extract<TableRow, { type: 'section' }>
type WorkflowTableRow = Extract<TableRow, { type: 'workflow' }>

type ListItem =
  | { type: 'header'; id: string; row: SectionTableRow; first: boolean }
  | { type: 'cell'; id: string; row: WorkflowTableRow; first: boolean; last: boolean }
  | { type: 'footer'; id: string; text: string }

function listItems(
  rows: TableRow[],
  getProvider: (providerId: number) => ProviderSummary | undefined,
  now: number
): ListItem[] {
  const items: ListItem[] = []
  let group: number[] = []
  let groupId = 'list'

  rows.forEach((row, index) => {
    if (row.type === 'section') {
      items.push({ type: 'header', id: row.id, row, first: index === 0 })
      group = []
      groupId = row.id

      return
    }

    const previous = rows[index - 1]
    const next = rows[index + 1]
    const last = next === undefined || next.type === 'section'

    group.push(...row.providerIds)
    items.push({
      type: 'cell',
      id: row.id,
      row,
      first: previous === undefined || previous.type === 'section',
      last,
    })

    const text = last ? staleFooter(group, getProvider, now) : undefined

    if (text) {
      items.push({ type: 'footer', id: `footer:${groupId}`, text })
    }
  })

  return items
}

interface SectionHeaderProps {
  row: SectionTableRow
  first: boolean
  getProvider: (providerId: number) => ProviderSummary | undefined
  onToggleRepo: (repoId: string) => void
  actions: PipelineActions
}

function SectionHeader({ row, first, getProvider, onToggleRepo, actions }: SectionHeaderProps): ReactNode {
  const pressRef = useRowMenuLongPress()
  const openNotifications = useNotificationCenterStore((state) => state.open)
  const providerIds = Array.from(row.providerIds)
  const meta = Array.from(new Set([row.organization, row.providerLabel].filter(Boolean))).join(' · ')
  const statusItems = providerIds.flatMap((id) => {
    const provider = getProvider(id)

    return provider
      ? [
          {
            label: `${provider.name} service status`,
            onClick: () => openNotifications({ type: 'provider', providerId: provider.id }),
          },
        ]
      : []
  })

  return (
    <div ref={pressRef} className={classes.sectionHeader} data-first={first || undefined}>
      <h3 className={classes.sectionHeading}>
        <button
          type="button"
          className={classes.sectionToggle}
          aria-expanded={row.expanded}
          onClick={() => onToggleRepo(row.id)}
        >
          <span className={classes.sectionTitle}>{row.name}</span>
          {meta && <span className={classes.sectionMeta}>{meta}</span>}
          {row.hasPendingFetch ? (
            <Loader size={14} aria-label="Fetching pipelines" />
          ) : (
            <span className={classes.sectionCount}>{row.pipelineCount}</span>
          )}
          <IconChevronRight className={classes.sectionChevron} data-open={row.expanded || undefined} aria-hidden />
        </button>
      </h3>
      <RowActionMenu
        name={row.name}
        items={[refreshItem(providerIds, actions), ...rowMenuItems(row, actions), ...statusItems]}
      />
    </div>
  )
}

interface WorkflowRowViewProps {
  row: WorkflowTableRow
  now: number
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

function WorkflowRowView({ row, now, getProvider, actions, selection }: WorkflowRowViewProps): ReactNode {
  const { pipeline } = row
  const message =
    pipeline.provider_type === 'fluxcd' && pipeline.metadata?.message ? String(pipeline.metadata.message) : ''
  const nested = Boolean(row.depth)
  const subtitle = [nested ? '' : row.organization, pipeline.branch, relativeOrNever(row.lastRun, now), message]
    .filter(Boolean)
    .join(' · ')

  return (
    <IosPipelineRow
      status={row.status}
      statusText={getStatusLabel(pipeline)}
      title={row.name}
      subtitle={subtitle}
      hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
      longPressMenu
      selected={selection ? selection.selected.has(pipeline.id) : undefined}
      onClick={() => (selection ? selection.onToggle(pipeline) : actions.onViewHistory(pipeline))}
      swipeActions={workflowSwipeActions(pipeline, actions)}
      menu={<WorkflowActionMenu pipeline={pipeline} provider={getProvider(pipeline.provider_id)} actions={actions} />}
    />
  )
}

export function IosPipelineList({
  header,
  rows,
  getProvider,
  onToggleRepo,
  actions,
  selection,
}: IosPipelineListProps): ReactNode {
  const now = useNow(30_000)
  const items = listItems(rows, getProvider, now)
  const sectioned = rows[0]?.type === 'section'

  return (
    <section className={groupClasses.section}>
      {header && !sectioned && (
        <div className={groupClasses.headerRow}>
          <h2 className={groupClasses.header}>{header}</h2>
        </div>
      )}
      <VirtualList
        items={items}
        getKey={(item) => item.id}
        estimateSize={ROW_ESTIMATE}
        renderItem={(item) => {
          if (item.type === 'header') {
            return (
              <SectionHeader
                row={item.row}
                first={item.first}
                getProvider={getProvider}
                onToggleRepo={onToggleRepo}
                actions={actions}
              />
            )
          }

          if (item.type === 'footer') {
            return <p className={classes.sectionFooter}>{item.text}</p>
          }

          return (
            <div className={classes.cell} data-first={item.first || undefined} data-last={item.last || undefined}>
              <WorkflowRowView
                row={item.row}
                now={now}
                getProvider={getProvider}
                actions={actions}
                selection={selection}
              />
            </div>
          )
        }}
      />
    </section>
  )
}
