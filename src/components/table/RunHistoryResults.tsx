import { Button, Group } from '@mantine/core'
import { IconHistory, IconRefresh } from '@tabler/icons-react'
import { type ReactNode, useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'

import { IosRunToolbar } from './IosRunList'
import { useRunActions } from './RunActionsContext'
import { RunHistoryMobile } from './RunHistoryMobile'
import { RunHistoryTable } from './RunHistoryTable'
import type { RunPageSelection } from './runHistoryTypes'
import type { RunHistoryColumns } from './useRunHistoryColumns'
import type { RunHistoryPageData } from './useRunHistoryPageData'

interface RunHistoryResultsProps {
  data: RunHistoryPageData
  columns: RunHistoryColumns
  selection: RunPageSelection
}

function RunHistoryEmpty({ data }: { data: RunHistoryPageData }): ReactNode {
  const { isMobile } = useIsMobile()
  const { labels, isFlux } = useRunActions()

  if (data.sourceRuns.length === 0) {
    return (
      <EmptyState
        icon={<IconHistory size={24} aria-hidden />}
        title={`No ${labels.runs} yet`}
        description={
          isFlux
            ? 'Flux has not recorded a reconciliation for this resource yet.'
            : 'This workflow has no recorded runs.'
        }
        action={
          <Button
            type="button"
            variant="default"
            size="sm"
            leftSection={<IconRefresh size={16} aria-hidden />}
            onClick={() => {
              void data.refetch()
            }}
          >
            Refresh {labels.runs}
          </Button>
        }
      />
    )
  }

  return (
    <EmptyState
      icon={<IconHistory size={24} aria-hidden />}
      title={`No ${labels.runs} match your filters`}
      description={
        data.hasMore
          ? `None of the loaded ${labels.runs} match the current search or filters. More can still be loaded.`
          : `None of the ${labels.runs} match the current search or filters.`
      }
      action={
        <Group gap="sm">
          {data.hasMore && isMobile && (
            <Button type="button" variant="default" size="sm" onClick={data.fetchMore}>
              Load more {labels.runs}
            </Button>
          )}
          <Button type="button" variant="default" size="sm" onClick={data.clearFilters}>
            Clear filters
          </Button>
        </Group>
      }
    />
  )
}

export function RunHistoryResults({ data, columns, selection }: RunHistoryResultsProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const { labels } = useRunActions()
  const [selecting, setSelecting] = useState(false)

  if (data.isError) {
    return (
      <ErrorState
        title={`Couldn't load ${labels.history.toLowerCase()}`}
        error={data.error}
        onRetry={() => {
          void data.refetch()
        }}
        retrying={data.isFetching}
      />
    )
  }

  if (!isMobile && columns.error) {
    return (
      <ErrorState
        title="Couldn't load run table columns"
        error={columns.error}
        onRetry={() => {
          void columns.refetch()
        }}
        retrying={columns.loading}
      />
    )
  }

  const empty = !data.isFetching && data.runs.length === 0 ? <RunHistoryEmpty data={data} /> : null

  if (isMobile) {
    return (
      <>
        <RunHistoryMobile
          data={data}
          selection={selection}
          shell={mobileShell}
          selecting={selecting}
          onStartSelection={() => setSelecting(true)}
          emptyState={empty}
        />
        {mobileShell && selecting && (
          <IosRunToolbar
            count={selection.selectedRuns.length}
            cancelling={selection.isCancelling}
            onDone={() => {
              selection.selectRuns([])
              setSelecting(false)
            }}
            onCancelRuns={selection.cancelSelected}
          />
        )}
      </>
    )
  }

  return empty ?? <RunHistoryTable data={data} columns={columns} selection={selection} />
}
