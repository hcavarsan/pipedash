import type { KeyboardEvent, ReactNode } from 'react'

import type { PipelineRun } from '../../types'
import { formatCount } from '../../utils/formatCount'
import { useContextMenu } from '../common/ContextMenu'
import { StandardTable } from '../common/StandardTable'

import { useRunActions } from './RunActionsContext'
import { buildRunActions } from './runActions'
import type { RunPageSelection } from './runHistoryTypes'
import type { RunHistoryColumns } from './useRunHistoryColumns'
import type { RunHistoryPageData } from './useRunHistoryPageData'

interface RunHistoryTableProps {
  data: RunHistoryPageData
  columns: RunHistoryColumns
  selection: RunPageSelection
}

export function RunHistoryTable({ data, columns, selection }: RunHistoryTableProps): ReactNode {
  const actions = useRunActions()
  const contextMenu = useContextMenu()
  const { labels, openRun, canCancel } = actions
  const { page } = data.filters

  const runMenu = (run: PipelineRun) =>
    contextMenu(() =>
      buildRunActions(actions, run).map(({ label, icon, color, disabled, execute }) => ({
        label,
        icon,
        disabled,
        ...(color === 'red' ? { color: 'red' as const } : {}),
        onClick: () => void execute(),
      }))
    )

  return (
    <StandardTable<PipelineRun>
      records={data.runs}
      onRowClick={({ record }) => openRun(record)}
      customRowAttributes={(record) => {
        const menu = runMenu(record)

        return {
          tabIndex: 0,
          'aria-label': `Open ${labels.run} #${record.run_number}`,
          onContextMenu: menu.onContextMenu,
          onKeyDown: (event: KeyboardEvent<HTMLTableRowElement>) => {
            menu.onKeyDown(event)
            if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) {
              event.preventDefault()
              openRun(record)
            }
          },
        }
      }}
      columns={columns.columns}
      {...(canCancel && {
        selectedRecords: selection.selectedRuns,
        onSelectedRecordsChange: selection.selectRuns,
        isRecordSelectable: selection.isRunCancellable,
      })}
      sortStatus={data.sortStatus}
      onSortStatusChange={data.setSortOverride}
      fetching={data.isFetching}
      totalRecords={data.effectiveTotalRecords}
      recordsPerPage={data.pageSize}
      page={page}
      onPageChange={(next) => data.setFilter('page', next)}
      paginationText={({ from, to }) =>
        data.isFetching
          ? 'Loading...'
          : data.isComplete
            ? `Showing ${from}-${to} of ${formatCount(data.totalCount, labels.run)}`
            : `Showing ${from}-${to} ${labels.runs}`
      }
    />
  )
}
