import {
  ActionIcon,
  Box,
  Button,
  Card,
  Checkbox,
  Group,
  Loader,
  Skeleton,
  Stack,
  Tabs,
  Text,
  UnstyledButton,
} from '@mantine/core'
import { useDisclosure, useIntersection } from '@mantine/hooks'
import { modals } from '@mantine/modals'
import {
  IconAdjustments,
  IconCalendar,
  IconChartLine,
  IconClock,
  IconFileText,
  IconGitBranch,
  IconHistory,
  IconRefresh,
  IconSquare,
  IconUser,
} from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { PAGE_SIZES } from '../../constants/pagination'
import { useBulkCancelRuns } from '../../hooks/useBulkCancelRuns'
import { useIsMobile } from '../../hooks/useIsMobile'
import { useTableColumns } from '../../hooks/useTableColumns'
import { useRunHistoryFilters } from '../../hooks/useUrlState'
import {
  useClearRunHistoryCache,
  useRunHistory,
  useSaveTablePreferences,
  useTablePreferences,
} from '../../queries/useRunHistoryQueries'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { events } from '../../services'
import { useModalStore } from '../../stores/modalStore'
import { TYPOGRAPHY } from '../../theme/typography'
import type { Pipeline, PipelineRun } from '../../types'
import { getDateRangeStart } from '../../utils/dateRange'
import { formatCount } from '../../utils/formatCount'
import { formatDateTime } from '../../utils/formatDateTime'
import { formatDuration } from '../../utils/formatDuration'
import { getPipelineLabels, getStatusLabel, getStatusOptions } from '../../utils/pipelinePresentation'
import { TableCells } from '../../utils/tableCells'
import { StatusBadge } from '../atoms/StatusBadge'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { PipelineMetricsView } from '../pipeline/PipelineMetricsView'

import { TableCustomizationModal } from './TableCustomizationModal'

interface RunHistoryPageProps {
  pipeline: Pipeline | null
  onBack: () => void
  onViewRun: (pipelineId: string, runNumber: number) => void
  onRerun?: (pipeline: Pipeline, run: PipelineRun) => void
  onCancel?: (pipeline: Pipeline, run: PipelineRun) => void
  refreshTrigger?: number
  onLoadingChange?: (loading: boolean) => void
  initialTab?: 'history' | 'metrics'
  isLoadingPipeline?: boolean
}

interface RunFieldProps {
  icon?: ReactNode
  label: string
  children: ReactNode
}

const RunField = ({ icon, label, children }: RunFieldProps) => (
  <Box flex={1} miw={0}>
    <Group gap="xs" wrap="nowrap" c="dimmed">
      {icon}
      <Text size={TYPOGRAPHY.FIELD_LABEL.size}>{label}</Text>
    </Group>
    {children}
  </Box>
)

export const RunHistoryPage = ({
  pipeline,
  onBack,
  onViewRun,
  onRerun,
  onCancel,
  refreshTrigger,
  onLoadingChange,
  initialTab = 'history',
  isLoadingPipeline = false,
}: RunHistoryPageProps) => {
  const { isMobile } = useIsMobile()
  const labels = getPipelineLabels(pipeline?.provider_type)
  const isFlux = pipeline?.provider_type === 'fluxcd'

  const { filters, setFilter, clearFilters } = useRunHistoryFilters()
  const { search, status: statusFilter, branch: branchFilter, actor: actorFilter, dateRange, page } = filters

  const [cancellingRunNumber, setCancellingRunNumber] = useState<number | null>(null)
  const { data: runsTable } = useTableDefinition(pipeline?.provider_id ?? 0, 'pipeline_runs')
  const defaultSortStatus = useMemo<DataTableSortStatus<PipelineRun>>(
    () => ({
      columnAccessor: runsTable?.default_sort_column ?? 'run_number',
      direction: runsTable?.default_sort_direction === 'asc' ? 'asc' : 'desc',
    }),
    [runsTable?.default_sort_column, runsTable?.default_sort_direction]
  )
  const [sortOverride, setSortOverride] = useState<DataTableSortStatus<PipelineRun> | null>(null)
  const sortStatus = sortOverride ?? defaultSortStatus
  const [activeTab, setActiveTab] = useState<string>(initialTab)
  const [customizeModalOpened, { open: openCustomizeModal, close: closeCustomizeModal }] = useDisclosure(false)
  const [accumulatedRuns, setAccumulatedRuns] = useState<PipelineRun[]>([])
  const { ref: loadMoreRef, entry } = useIntersection({ threshold: 0.5 })

  const PAGE_SIZE = PAGE_SIZES.RUN_HISTORY

  const refreshCounterRef = useRef(0)
  const lastKnownPaginationRef = useRef<{ totalRecords: number; maxPage: number }>({
    totalRecords: PAGE_SIZE,
    maxPage: 1,
  })

  const {
    data: runHistoryData,
    isLoading: loading,
    isFetching: pageLoading,
    isPlaceholderData,
    isError,
    error: runsError,
    refetch: refetchRuns,
  } = useRunHistory(pipeline?.id || '', page, PAGE_SIZE)

  const { data: tablePreferences, isLoading: preferencesLoading } = useTablePreferences(
    pipeline?.provider_id ?? 0,
    'pipeline_runs'
  )

  const saveTablePreferencesMutation = useSaveTablePreferences()
  const clearCacheMutation = useClearRunHistoryCache()

  const runs = useMemo(() => runHistoryData?.runs ?? [], [runHistoryData?.runs])
  const totalCount = runHistoryData?.total_count ?? 0
  const isComplete = runHistoryData?.is_complete ?? false
  const hasMore = runHistoryData?.has_more ?? false
  const preferencesLoaded = !preferencesLoading

  useEffect(() => {
    if (!isError && !pageLoading && totalCount > 0) {
      const calculatedTotal = hasMore ? Math.max((page + 4) * PAGE_SIZE, totalCount) : totalCount

      lastKnownPaginationRef.current = {
        totalRecords: Math.max(lastKnownPaginationRef.current.totalRecords, calculatedTotal),
        maxPage: Math.max(lastKnownPaginationRef.current.maxPage, page),
      }
    }
  }, [isError, pageLoading, totalCount, hasMore, page])

  const effectiveTotalRecords = useMemo(() => {
    if (isError || (pageLoading && totalCount === 0)) {
      const minFromCurrentPage = Math.max(page * PAGE_SIZE, PAGE_SIZE)

      return Math.max(lastKnownPaginationRef.current.totalRecords, minFromCurrentPage)
    }

    const calculated = hasMore ? Math.max((page + 4) * PAGE_SIZE, totalCount) : Math.max(totalCount, PAGE_SIZE)

    return calculated
  }, [pageLoading, isError, totalCount, hasMore, page])

  useEffect(() => {
    if (!isMobile || isPlaceholderData || !runHistoryData) {
      return
    }

    setAccumulatedRuns((prev) => {
      if (page === 1) {
        return runs
      }

      const incoming = new Map(runs.map((run) => [run.run_number, run]))
      const known = new Set(prev.map((run) => run.run_number))

      return [
        ...prev.map((run) => incoming.get(run.run_number) ?? run),
        ...runs.filter((run) => !known.has(run.run_number)),
      ]
    })
  }, [runs, runHistoryData, page, isMobile, isPlaceholderData])

  useEffect(() => {
    if (!isMobile || !entry?.isIntersecting || pageLoading || !hasMore) {
      return
    }
    setFilter('page', page + 1)
  }, [entry?.isIntersecting, isMobile, pageLoading, hasMore, page, setFilter])

  const columnPreferences = useMemo(
    () => ({
      columnOrder: tablePreferences?.columnOrder,
      columnVisibility: tablePreferences?.columnVisibility,
    }),
    [tablePreferences]
  )

  useEffect(() => {
    if (onLoadingChange) {
      onLoadingChange(loading || pageLoading)
    }
  }, [loading, pageLoading, onLoadingChange])

  const handleApplyCustomization = async (newColumnOrder: string[], newColumnVisibility: Record<string, boolean>) => {
    if (!pipeline?.provider_id) {
      return
    }

    await saveTablePreferencesMutation.mutateAsync({
      providerId: pipeline.provider_id,
      tableId: 'pipeline_runs',
      preferences: {
        columnOrder: newColumnOrder,
        columnVisibility: newColumnVisibility,
      },
    })
  }

  const handleCancelRun = useCallback(
    (pipeline: Pipeline, run: PipelineRun) => {
      modals.openConfirmModal({
        title: 'Cancel run',
        children: <Text size="sm">This cancels run #{run.run_number}. You can't undo this.</Text>,
        labels: { confirm: 'Cancel run', cancel: 'Keep running' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          setCancellingRunNumber(run.run_number)
          try {
            if (onCancel) {
              await onCancel(pipeline, run)
            }
          } finally {
            setCancellingRunNumber(null)
          }
        },
      })
    },
    [onCancel]
  )

  const rerunLoading = useModalStore((s) => s.rerunLoading)

  const renderRunActions = useCallback(
    (run: PipelineRun, withDetails: boolean) => {
      if (!pipeline) {
        return null
      }

      const isActive = run.status === 'running' || run.status === 'pending'
      const isPending = run.status === 'pending'
      const isCancelling = cancellingRunNumber === run.run_number
      const isRerunning = rerunLoading?.pipelineId === pipeline.id && rerunLoading.runNumber === run.run_number
      const cancelLabel = isPending
        ? `Cancel unavailable for pending run #${run.run_number}`
        : `Cancel run #${run.run_number}`
      const rerunLabel = isFlux ? `Reconcile ${pipeline.name}` : `Rerun run #${run.run_number}`
      const detailsLabel = `View details for ${labels.run} #${run.run_number}`

      return (
        <Group gap="xs" wrap="nowrap" justify="flex-end">
          {isActive
            ? onCancel && (
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  size="md"
                  title={cancelLabel}
                  aria-label={cancelLabel}
                  loading={isCancelling}
                  disabled={isCancelling || isPending}
                  onClick={(e) => {
                    e.stopPropagation()
                    handleCancelRun(pipeline, run)
                  }}
                >
                  <IconSquare size={16} />
                </ActionIcon>
              )
            : onRerun && (
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  size="md"
                  title={rerunLabel}
                  aria-label={rerunLabel}
                  loading={isRerunning}
                  disabled={isRerunning}
                  onClick={(e) => {
                    e.stopPropagation()
                    onRerun(pipeline, run)
                  }}
                >
                  <IconRefresh size={16} />
                </ActionIcon>
              )}
          {withDetails && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size="md"
              title={detailsLabel}
              aria-label={detailsLabel}
              onClick={(e) => {
                e.stopPropagation()
                onViewRun(pipeline.id, run.run_number)
              }}
            >
              <IconFileText size={16} />
            </ActionIcon>
          )}
        </Group>
      )
    },
    [cancellingRunNumber, handleCancelRun, onCancel, onRerun, onViewRun, pipeline, rerunLoading, isFlux, labels.run]
  )

  const actionsColumn = useMemo(
    () => ({
      accessor: 'actions' as keyof PipelineRun,
      title: 'Actions',
      width: '120px',
      textAlign: 'right' as const,
      resizable: false,
      toggleable: false,
      draggable: false,
      render: (run: PipelineRun) => renderRunActions(run, true),
    }),
    [renderRunActions]
  )

  const additionalColumns = useMemo(() => [actionsColumn], [actionsColumn])

  const {
    columns,
    allColumns,
    loading: schemaLoading,
    error: schemaError,
    refetch: refetchSchema,
  } = useTableColumns(pipeline?.provider_id, 'pipeline_runs', additionalColumns, columnPreferences)

  const headerActions = useMemo(() => {
    const shouldShow = !isMobile && activeTab === 'history'
    const isLoading = isLoadingPipeline || !pipeline

    if (isLoading && shouldShow) {
      return (
        <Box style={{ minWidth: 'fit-content' }}>
          <Skeleton height="2.25rem" width="10rem" />
        </Box>
      )
    }

    return (
      <Box
        style={{
          visibility: shouldShow ? 'visible' : 'hidden',
          minWidth: 'fit-content',
        }}
      >
        <Button
          variant="default"
          size="sm"
          leftSection={<IconAdjustments size={16} />}
          onClick={openCustomizeModal}
          disabled={!shouldShow || schemaLoading || !!schemaError}
          tabIndex={shouldShow ? 0 : -1}
        >
          Customize columns
        </Button>
      </Box>
    )
  }, [isMobile, activeTab, openCustomizeModal, isLoadingPipeline, pipeline, schemaLoading, schemaError])

  const sourceRuns = isMobile && accumulatedRuns.length > 0 ? accumulatedRuns : runs

  const branches = useMemo(
    () => Array.from(new Set(sourceRuns.map((r) => r.branch).filter((b): b is string => !!b))),
    [sourceRuns]
  )

  const actors = useMemo(
    () => Array.from(new Set(sourceRuns.map((r) => r.actor).filter((a): a is string => !!a))),
    [sourceRuns]
  )

  const previousPipelineIdRef = useRef<string | null>(null)

  useEffect(() => {
    const pipelineChanged = previousPipelineIdRef.current !== null && previousPipelineIdRef.current !== pipeline?.id

    if (pipelineChanged) {
      setAccumulatedRuns([])
      if (page !== 1) {
        setFilter('page', 1)
      }
    }

    previousPipelineIdRef.current = pipeline?.id || null
  }, [pipeline?.id, page, setFilter])

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0 && pipeline?.id) {
      const currentRefresh = refreshCounterRef.current

      if (refreshTrigger > currentRefresh) {
        refreshCounterRef.current = refreshTrigger
        clearCacheMutation.mutateAsync(pipeline.id).then(() => {
          refetchRuns()
        })
      }
    }
  }, [refreshTrigger, pipeline?.id, clearCacheMutation, refetchRuns])

  useEffect(() => {
    if (!pipeline) {
      return
    }

    let mounted = true
    const unlisteners: Array<() => void> = []

    const setupListeners = async () => {
      const unlisten1 = await events.listen<string>('run-triggered', (payload) => {
        if (mounted && payload === pipeline.id) {
          setFilter('page', 1)
          refetchRuns()
        }
      })

      if (!mounted) {
        try {
          unlisten1()
        } catch {
          /* listener already removed */
        }

        return
      }
      unlisteners.push(unlisten1)

      const unlisten2 = await events.listen<string>('run-cancelled', (payload) => {
        if (mounted && payload === pipeline.id) {
          refetchRuns()
        }
      })

      if (!mounted) {
        try {
          unlisten2()
        } catch {
          /* listener already removed */
        }

        return
      }
      unlisteners.push(unlisten2)
    }

    setupListeners()

    return () => {
      mounted = false
      unlisteners.forEach((unlisten) => {
        try {
          unlisten()
        } catch {
          /* listener may already be removed */
        }
      })
    }
  }, [pipeline, refetchRuns, setFilter])

  const filteredRuns = useMemo(() => {
    const rangeStart = getDateRangeStart(dateRange)
    const term = search.toLowerCase()

    return sourceRuns.filter((run) => {
      if (rangeStart && !(run.started_at && new Date(run.started_at) >= rangeStart)) {
        return false
      }

      if (
        term &&
        ![run.branch, run.commit_sha, run.commit_message, run.actor].some((field) =>
          field?.toLowerCase().includes(term)
        )
      ) {
        return false
      }

      return (
        (!statusFilter || run.status === statusFilter) &&
        (!branchFilter || run.branch === branchFilter) &&
        (!actorFilter || run.actor === actorFilter)
      )
    })
  }, [sourceRuns, dateRange, search, statusFilter, branchFilter, actorFilter])

  const sortedRuns = useMemo(() => {
    const sorted = [...filteredRuns]
    const { columnAccessor, direction } = sortStatus

    sorted.sort((a, b) => {
      let aVal: unknown = a[columnAccessor as keyof PipelineRun]
      let bVal: unknown = b[columnAccessor as keyof PipelineRun]

      if (columnAccessor === 'started_at' || columnAccessor === 'concluded_at') {
        aVal = aVal ? new Date(aVal as string | number | Date).getTime() : 0
        bVal = bVal ? new Date(bVal as string | number | Date).getTime() : 0
      }

      if (aVal === null || aVal === undefined) {
        aVal = ''
      }
      if (bVal === null || bVal === undefined) {
        bVal = ''
      }

      if (typeof aVal === 'string' && typeof bVal === 'string') {
        return direction === 'asc' ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal)
      }

      if (typeof aVal === 'number' && typeof bVal === 'number') {
        return direction === 'asc' ? aVal - bVal : bVal - aVal
      }

      return 0
    })

    return sorted
  }, [filteredRuns, sortStatus])

  const {
    selectedRuns,
    isRunCancellable,
    selectRuns,
    cancelSelected,
    isCancelling: isCancellingSelected,
  } = useBulkCancelRuns({
    pipelineId: pipeline?.id,
    page: isMobile ? 1 : page,
    runs: sortedRuns,
    enabled: !!onCancel,
    cancellingRunNumber,
  })

  const renderMobileCards = (currentPipeline: Pipeline, emptyState: ReactNode) => (
    <Stack gap="md">
      {sortedRuns.map((run) => {
        const selectable = isRunCancellable(run)
        const selected = selectedRuns.some((selectedRun) => selectedRun.run_number === run.run_number)

        return (
          <Card key={run.run_number} padding="lg">
            <Stack gap="xs">
              <Group justify="space-between" wrap="nowrap">
                <Group gap="xs" wrap="nowrap" flex={1} miw={0}>
                  {selectable && (
                    <Checkbox
                      aria-label={`Select run #${run.run_number}`}
                      checked={selected}
                      onChange={(e) =>
                        selectRuns(
                          e.currentTarget.checked
                            ? [...selectedRuns, run]
                            : selectedRuns.filter((selectedRun) => selectedRun.run_number !== run.run_number)
                        )
                      }
                    />
                  )}
                  <UnstyledButton
                    className="interactive-surface"
                    aria-label={`View details for ${labels.run} #${run.run_number}`}
                    onClick={() => onViewRun(currentPipeline.id, run.run_number)}
                  >
                    <Group gap="xs" wrap="nowrap">
                      <Text size={TYPOGRAPHY.ITEM_TITLE.size} fw={TYPOGRAPHY.ITEM_TITLE.weight}>
                        {isFlux ? String(run.metadata?.run_label ?? 'Reconciliation') : `#${run.run_number}`}
                      </Text>
                      <StatusBadge status={run.status} label={getStatusLabel(run)} />
                    </Group>
                  </UnstyledButton>
                </Group>
                {renderRunActions(run, false)}
              </Group>

              {isFlux ? (
                <Text size="sm" c="dimmed">
                  {String(run.metadata?.message ?? run.commit_message ?? '')}
                </Text>
              ) : (
                <Group gap="lg" wrap="nowrap" align="flex-start">
                  <RunField icon={<IconGitBranch size={14} />} label="Branch">
                    <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
                      {run.branch || '—'}
                    </Text>
                  </RunField>
                  <RunField icon={<IconUser size={14} />} label="Actor">
                    <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
                      {run.actor || '—'}
                    </Text>
                  </RunField>
                </Group>
              )}

              <Group gap="lg" wrap="nowrap" align="flex-start">
                <RunField icon={<IconClock size={14} />} label="Duration">
                  <Text size={TYPOGRAPHY.FIELD_VALUE.size}>{formatDuration(run.duration_seconds)}</Text>
                </RunField>
                <RunField icon={<IconCalendar size={14} />} label={isFlux ? 'Recorded' : 'Started'}>
                  <Text size={TYPOGRAPHY.FIELD_VALUE.size} truncate>
                    {run.started_at ? formatDateTime(run.started_at) : '—'}
                  </Text>
                </RunField>
              </Group>

              {run.commit_sha && <RunField label="Commit">{TableCells.commit(run.commit_sha)}</RunField>}
            </Stack>
          </Card>
        )
      })}
      {emptyState}
      <Box ref={loadMoreRef} py="md">
        {pageLoading && hasMore && (
          <Group justify="center" gap="xs" role="status">
            <Loader size="sm" />
            <Text size="sm" c="dimmed">
              Loading more {labels.runs}...
            </Text>
          </Group>
        )}
        {!hasMore && sortedRuns.length > 0 && (
          <Text size="sm" c="dimmed" ta="center">
            {isComplete ? `All available ${formatCount(totalCount, labels.run)}` : `${totalCount}+ ${labels.runs}`}
          </Text>
        )}
      </Box>
    </Stack>
  )

  const renderRuns = (currentPipeline: Pipeline) => {
    if (isError) {
      return (
        <ErrorState
          title={`Couldn't load ${labels.history.toLowerCase()}`}
          error={runsError}
          onRetry={() => refetchRuns()}
          retrying={pageLoading}
        />
      )
    }

    if (!isMobile && schemaError) {
      return (
        <ErrorState
          title="Couldn't load run table columns"
          error={schemaError}
          onRetry={() => refetchSchema()}
          retrying={schemaLoading}
        />
      )
    }

    const emptyState =
      !pageLoading && sortedRuns.length === 0 ? (
        sourceRuns.length === 0 ? (
          <EmptyState
            icon={<IconHistory size={24} />}
            title={`No ${labels.runs} yet`}
            description={
              isFlux
                ? 'Flux has not recorded a reconciliation for this resource yet.'
                : 'This workflow has no recorded runs.'
            }
            action={
              <Button variant="default" size="sm" leftSection={<IconRefresh size={16} />} onClick={() => refetchRuns()}>
                Refresh {labels.runs}
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<IconHistory size={24} />}
            title={`No ${labels.runs} match your filters`}
            description={
              hasMore
                ? `None of the loaded ${labels.runs} match the current search or filters. More can still be loaded.`
                : `None of the ${labels.runs} match the current search or filters.`
            }
            action={
              <Group gap="sm">
                {hasMore && isMobile && (
                  <Button variant="default" size="sm" onClick={() => setFilter('page', page + 1)}>
                    Load more {labels.runs}
                  </Button>
                )}
                <Button variant="default" size="sm" onClick={clearFilters}>
                  Clear filters
                </Button>
              </Group>
            }
          />
        )
      ) : null

    if (isMobile) {
      return renderMobileCards(currentPipeline, emptyState)
    }

    if (emptyState) {
      return emptyState
    }

    return (
      <Box style={{ flex: 1, minHeight: 360, position: 'relative' }}>
        <StandardTable<PipelineRun>
          records={sortedRuns}
          onRowClick={({ record }) => onViewRun(currentPipeline.id, record.run_number)}
          rowStyle={() => ({ cursor: 'pointer' })}
          columns={columns}
          {...(onCancel && {
            selectedRecords: selectedRuns,
            onSelectedRecordsChange: selectRuns,
            isRecordSelectable: isRunCancellable,
          })}
          sortStatus={sortStatus}
          onSortStatusChange={setSortOverride}
          fetching={pageLoading}
          totalRecords={effectiveTotalRecords}
          recordsPerPage={PAGE_SIZE}
          page={page}
          onPageChange={(p) => setFilter('page', p)}
          paginationText={({ from, to }) =>
            pageLoading
              ? 'Loading...'
              : isComplete
                ? `Showing ${from}-${to} of ${formatCount(totalCount, labels.run)}`
                : `Showing ${from}-${to} ${labels.runs}`
          }
        />
      </Box>
    )
  }

  const isHistoryLoading = isLoadingPipeline || loading || !preferencesLoaded || schemaLoading || !pipeline?.id

  return (
    <PageContainer>
      <PageHeader
        title={
          !isLoadingPipeline && pipeline?.name ? pipeline.name : <Skeleton height="1.5rem" width="min(250px, 60vw)" />
        }
        onBack={onBack}
        actions={headerActions}
      />
      {isFlux && pipeline && (
        <Group gap="sm" align="flex-start" mb="sm">
          <StatusBadge status={pipeline.status} label={getStatusLabel(pipeline)} size="md" />
          <Text size="sm" style={{ overflowWrap: 'anywhere', flex: 1 }}>
            {String(pipeline.metadata?.message ?? '')}
          </Text>
        </Group>
      )}
      {pipeline?.metadata?.history_scope === 'bounded_reconciliation' && (
        <Text size="sm" c="dimmed" mb="sm">
          FluxCD retains a limited reconciliation history. Repeated reconciliations may be grouped, and older entries
          may be unavailable.
        </Text>
      )}

      <Tabs
        value={activeTab}
        onChange={(value) => setActiveTab(value || 'history')}
        style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
      >
        <Tabs.List mb="sm">
          <Tabs.Tab value="history" leftSection={<IconHistory size={16} />}>
            {labels.history}
          </Tabs.Tab>
          <Tabs.Tab value="metrics" leftSection={<IconChartLine size={16} />}>
            Metrics
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel
          value="history"
          style={{
            width: '100%',
            display: 'flex',
            flexDirection: 'column',
            gap: 'var(--mantine-spacing-md)',
            flex: 1,
            minHeight: 0,
          }}
        >
          {isHistoryLoading || !pipeline ? (
            <LoadingState message={`Loading ${labels.history.toLowerCase()}...`} />
          ) : (
            <>
              <Group justify="space-between" align="flex-start" wrap="nowrap">
                <FilterBar
                  onClearAll={clearFilters}
                  filters={{
                    search: {
                      value: search,
                      onChange: (value) => setFilter('search', value),
                      placeholder: `Search ${labels.runs}...`,
                    },
                    status: {
                      value: statusFilter,
                      onChange: (value) => setFilter('status', value),
                      options: getStatusOptions(pipeline.provider_type),
                    },
                    branch: {
                      value: branchFilter,
                      onChange: (value) => setFilter('branch', value),
                      options: branches,
                    },
                    actor: {
                      value: actorFilter,
                      onChange: (value) => setFilter('actor', value),
                      options: actors,
                    },
                    dateRange: {
                      value: dateRange,
                      onChange: (value) => setFilter('dateRange', value),
                    },
                  }}
                />
                {selectedRuns.length > 0 && (
                  <Button
                    size="xs"
                    variant="light"
                    color="red"
                    leftSection={<IconSquare size={14} />}
                    loading={isCancellingSelected}
                    onClick={cancelSelected}
                  >
                    Cancel selected
                  </Button>
                )}
              </Group>

              {renderRuns(pipeline)}
            </>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="metrics" style={{ width: '100%' }}>
          {isLoadingPipeline || !pipeline ? (
            <LoadingState message="Loading pipeline..." />
          ) : (
            <PipelineMetricsView
              pipelineId={pipeline.id}
              pipelineName={pipeline.name}
              repository={pipeline.repository}
              refreshTrigger={refreshTrigger}
            />
          )}
        </Tabs.Panel>
      </Tabs>

      <TableCustomizationModal
        opened={customizeModalOpened}
        onClose={closeCustomizeModal}
        columns={allColumns}
        visibleColumns={columns}
        currentOrder={columnPreferences.columnOrder}
        currentVisibility={columnPreferences.columnVisibility}
        onApply={handleApplyCustomization}
      />
    </PageContainer>
  )
}
