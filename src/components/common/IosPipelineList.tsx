import { Loader } from '@mantine/core'
import { IconCheck, IconChevronRight } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'

import { useRowMenuLongPress } from '../../hooks/useLongPress'
import { useNow } from '../../hooks/useNow'
import { type StatusKey, statusMeta } from '../../theme/status'
import type { ProviderSummary } from '../../types/domain'
import { staleNote } from '../../utils/fetchInfo'
import { getPipelineLabels, getPipelineStatusLabel, pipelineStatusKey } from '../../utils/pipelinePresentation'
import { StatusGlyph } from '../atoms/StatusBadge'
import groupClasses from '../ios/IosList.module.css'
import { SwipeActions } from '../ios/SwipeActions'
import type { SwipeAction } from '../ios/swipeAction'
import classes from './IosPipelineList.module.css'
import {
  type PipelineActions,
  type PipelineSelection,
  type TableRow,
  WorkflowActionMenu,
  workflowSwipeActions,
} from './PipelineRowParts'
import { VirtualList } from './VirtualList'

interface IosPipelineRowProps {
  status?: StatusKey | null | undefined
  statusText?: string | undefined
  title: string
  subtitle: string
  hint?: string | undefined
  onClick: () => void
  menu?: ReactNode
  swipeActions?: SwipeAction[] | undefined
  longPressMenu?: boolean | undefined
  selected?: boolean | undefined
  reorderHandle?: ReactNode
  nested?: boolean | undefined
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
  reorderHandle,
  nested,
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
          {status && <span className={classes.srOnly}>, {statusText ?? statusMeta(status).label}</span>}
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
      {selecting && reorderHandle}
    </>
  )

  return (
    <div
      ref={pressRef}
      className={classes.row}
      data-glyph={status ? '' : undefined}
      data-nested={nested ? '' : undefined}
      data-selecting={selecting ? '' : undefined}
      data-swipeable={swipeable ? '' : undefined}
      data-trailing={reorderHandle ? '' : undefined}
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
  value ? compactRelative(value, now) : 'Never run'

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
  | { type: 'cell'; id: string; row: TableRow; first: boolean; last: boolean }
  | { type: 'footer'; id: string; text: string }

function listItems(
  rows: TableRow[],
  getProvider: (providerId: number) => ProviderSummary | undefined,
  now: number
): ListItem[] {
  const items: ListItem[] = rows.map((row, index) => ({
    type: 'cell',
    id: row.id,
    row,
    first: index === 0,
    last: index === rows.length - 1,
  }))
  const text = staleFooter(
    rows.flatMap((row) => [...row.providerIds]),
    getProvider,
    now
  )

  return text ? [...items, { type: 'footer', id: 'footer', text }] : items
}

function SectionRowView({
  row,
  onToggleRepo,
}: {
  row: SectionTableRow
  onToggleRepo: (repoId: string) => void
}): ReactNode {
  const summary = !row.expanded

  return (
    <div className={classes.row} data-glyph="">
      <h3 className={classes.sectionHeading}>
        <button
          type="button"
          className={classes.main}
          aria-expanded={row.expanded}
          onClick={() => onToggleRepo(row.id)}
        >
          {summary && row.status ? (
            <StatusGlyph status={row.status} />
          ) : (
            <span className={classes.glyphSlot} aria-hidden />
          )}
          <span className={classes.text}>
            <span className={classes.title}>{row.name}</span>
            {summary && row.status && <span className={classes.srOnly}>, {statusMeta(row.status).label}</span>}
          </span>
          {row.hasPendingFetch ? (
            <Loader size={14} aria-label="Fetching pipelines" />
          ) : (
            summary && <span className={classes.value}>{row.pipelineCount}</span>
          )}
          <IconChevronRight className={classes.sectionChevron} data-open={row.expanded || undefined} aria-hidden />
        </button>
      </h3>
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
  const message = pipeline.resource_state?.message ?? ''
  const nested = Boolean(row.depth)
  const subtitle = [nested ? '' : row.organization, pipeline.branch, relativeOrNever(row.lastRun, now), message]
    .filter(Boolean)
    .join(' · ')

  return (
    <IosPipelineRow
      status={pipelineStatusKey(pipeline)}
      statusText={getPipelineStatusLabel(pipeline)}
      title={row.name}
      subtitle={subtitle}
      hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
      longPressMenu
      selected={selection ? selection.selected.has(pipeline.id) : undefined}
      nested={nested}
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

  return (
    <section className={groupClasses.section}>
      {header && (
        <div className={groupClasses.headerRow}>
          <h2 className={groupClasses.header}>{header}</h2>
        </div>
      )}
      <VirtualList
        items={items}
        getKey={(item) => item.id}
        estimateSize={ROW_ESTIMATE}
        renderItem={(item) =>
          item.type === 'footer' ? (
            <p className={classes.sectionFooter}>{item.text}</p>
          ) : (
            <div className={classes.cell} data-first={item.first || undefined} data-last={item.last || undefined}>
              {item.row.type === 'section' ? (
                <SectionRowView row={item.row} onToggleRepo={onToggleRepo} />
              ) : (
                <WorkflowRowView
                  row={item.row}
                  now={now}
                  getProvider={getProvider}
                  actions={actions}
                  selection={selection}
                />
              )}
            </div>
          )
        }
      />
    </section>
  )
}
