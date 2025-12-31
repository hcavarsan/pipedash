import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { DataTableSortStatus } from 'mantine-datatable'

import { ActionIcon, Box, Button, Card, Center, Group, Loader, Skeleton, Stack, Tabs, Text } from '@mantine/core'
import { useDisclosure, useIntersection } from '@mantine/hooks'
import { modals } from '@mantine/modals'
import { IconAdjustments, IconCalendar, IconChartLine, IconClock, IconFileText, IconGitBranch, IconHistory, IconRefresh, IconSquare, IconUser } from '@tabler/icons-react'

import { PAGE_SIZES } from '../../constants/pagination'
import { useIsMobile } from '../../hooks/useIsMobile'
import { useTableColumns } from '../../hooks/useTableColumns'
import { useRunHistoryFilters } from '../../hooks/useUrlState'
import {
  useClearRunHistoryCache,
  useRunHistory,
  useSaveTablePreferences,
  useTablePreferences,
} from '../../queries/useRunHistoryQueries'
import { events } from '../../services'
import { useModalStore } from '../../stores/modalStore'
import type { Pipeline, PipelineRun } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { formatDuration } from '../../utils/formatDuration'
import { TableCells } from '../../utils/tableCells'
import { FilterBar } from '../common/FilterBar'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { PipelineMetricsView } from '../pipeline/PipelineMetricsView'

import { TableCustomizationModal } from './TableCustomizationModal'

interface RunHistoryPageProps {
  pipeline: Pipeline | null;
  onBack: () => void;
  onViewRun: (pipelineId: string, runNumber: number) => void;
  onRerun?: (pipeline: Pipeline, run: PipelineRun) => void;
  onCancel?: (pipeline: Pipeline, run: PipelineRun) => void;
  refreshTrigger?: number;
  onLoadingChange?: (loading: boolean) => void;
  initialTab?: 'history' | 'metrics';
  isLoadingPipeline?: boolean;
}

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

  const { filters, setFilter, clearFilters: _clearFilters } = useRunHistoryFilters()
  const { search, status: statusFilter, branch: branchFilter, actor: actorFilter, dateRange, page } = filters

  const [cancellingRunNumber, setCancellingRunNumber] = useState<number | null>(null)
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<PipelineRun>>({
    columnAccessor: 'run_number',
    direction: 'desc',
  })
  const [activeTab, setActiveTab] = useState<string>(initialTab)
  const [customizeModalOpened, { open: openCustomizeModal, close: closeCustomizeModal }] =
    useDisclosure(false)
  const [accumulatedRuns, setAccumulatedRuns] = useState<PipelineRun[]>([])
  const { ref: loadMoreRef, entry } = useIntersection({ threshold: 0.5 })

  const PAGE_SIZE = PAGE_SIZES.RUN_HISTORY

  const refreshCounterRef = useRef(0)
  const lastKnownPaginationRef = useRef<{ totalRecords: number; maxPage: number }>({ totalRecords: PAGE_SIZE, maxPage: 1 })

  const {
    data: runHistoryData,
    isLoading: loading,
    isFetching: pageLoading,
    isError,
    refetch: refetchRuns,
  } = useRunHistory(pipeline?.id || '', page, PAGE_SIZE)

  const {
    data: tablePreferences,
    isLoading: preferencesLoading,
  } = useTablePreferences(pipeline?.provider_id ?? 0, 'pipeline_runs')

  const saveTablePreferencesMutation = useSaveTablePreferences()
  const clearCacheMutation = useClearRunHistoryCache()

  const runs = useMemo(() => runHistoryData?.runs ?? [], [runHistoryData?.runs])
  const totalCount = runHistoryData?.total_count ?? 0
  const isComplete = runHistoryData?.is_complete ?? false
  const hasMore = runHistoryData?.has_more ?? false
  const preferencesLoaded = !preferencesLoading

  useEffect(() => {
    if (!isError && !pageLoading && totalCount > 0) {
      const calculatedTotal = hasMore
        ? Math.max((page + 4) * PAGE_SIZE, totalCount)
        : totalCount


      lastKnownPaginationRef.current = {
        totalRecords: Math.max(lastKnownPaginationRef.current.totalRecords, calculatedTotal),
        maxPage: Math.max(lastKnownPaginationRef.current.maxPage, page),
      }
    }
  }, [isError, pageLoading, totalCount, hasMore, page, PAGE_SIZE])

  const effectiveTotalRecords = useMemo(() => {
    if (isError || (pageLoading && totalCount === 0)) {
      const minFromCurrentPage = Math.max(page * PAGE_SIZE, PAGE_SIZE)


      
return Math.max(lastKnownPaginationRef.current.totalRecords, minFromCurrentPage)
    }

    const calculated = hasMore
      ? Math.max((page + 4) * PAGE_SIZE, totalCount)
      : Math.max(totalCount, PAGE_SIZE)

    return calculated
  }, [pageLoading, isError, totalCount, hasMore, page, PAGE_SIZE])

  useEffect(() => {
    if (!isMobile || runs.length === 0) {
return
}

    if (page === 1) {
      setAccumulatedRuns(runs)
    } else {
      setAccumulatedRuns(prev => {
        const existingIds = new Set(prev.map(r => r.run_number))
        const newRuns = runs.filter(r => !existingIds.has(r.run_number))


        
return [...prev, ...newRuns]
      })
    }
  }, [runs, page, isMobile])

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

  const handleApplyCustomization = async (
    newColumnOrder: string[],
    newColumnVisibility: Record<string, boolean>
  ) => {
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

  const handleCancelRun = useCallback((pipeline: Pipeline, run: PipelineRun) => {
    modals.openConfirmModal({
      title: 'Cancel Run',
      children: (
        <Text size="sm">
          Are you sure you want to cancel run #{run.run_number}? This action cannot be undone.
        </Text>
      ),
      labels: { confirm: 'Cancel Run', cancel: 'Keep Running' },
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
  }, [onCancel])

  const rerunLoading = useModalStore((s) => s.rerunLoading)

  const actionsColumn = useMemo(() => ({
    accessor: 'actions' as keyof PipelineRun,
    title: 'Actions',
    width: '120px',
    textAlign: 'center' as const,
    resizable: false,
    toggleable: false,
    draggable: false,
    render: (run: PipelineRun) => {
      const isRunning = run.status === 'running' || run.status === 'pending'
      const isCancelling = cancellingRunNumber === run.run_number
      const isRerunning = rerunLoading?.pipelineId === pipeline?.id && rerunLoading?.runNumber === run.run_number

      return (
        <Group gap={6} wrap="nowrap" justify="center" onClick={(e) => e.stopPropagation()}>
            {isRunning ? (
              onCancel && (
                <ActionIcon
                  variant="subtle"
                  color="red"
                  size="md"
                  onClick={(e) => {
                    e.stopPropagation()
                    if (pipeline) {
                      handleCancelRun(pipeline, run)
                    }
                  }}
                  title={run.status === 'pending' ? 'Cannot cancel pending workflow' : 'Stop run'}
                  aria-label={run.status === 'pending' ? 'Cannot cancel pending workflow' : 'Stop run'}
                  disabled={isCancelling || run.status === 'pending'}
                  style={{
                    backgroundColor: 'transparent',
                    cursor: (isCancelling || run.status === 'pending') ? 'not-allowed' : 'pointer',
                  }}
                >
                  <IconSquare
                    size={18}
                    style={{
                      animation: isCancelling ? 'spin 1s linear infinite' : 'none',
                    }}
                  />
                </ActionIcon>
              )
            ) : (
              onRerun && (
                <ActionIcon
                  variant="subtle"
                  color="blue"
                  size="md"
                  onClick={(e) => {
                    e.stopPropagation()
                    if (pipeline) {
                      onRerun(pipeline, run)
                    }
                  }}
                  title="Rerun workflow"
                  aria-label="Rerun workflow"
                  loading={isRerunning}
                  disabled={isRerunning}
                >
                  <IconRefresh size={18} />
                </ActionIcon>
              )
            )}
            <ActionIcon
              variant="subtle"
              color="blue"
              size="md"
              onClick={(e) => {
                e.stopPropagation()
                if (pipeline) {
                  onViewRun(pipeline.id, run.run_number)
                }
              }}
              title="View details"
              aria-label="View details"
            >
              <IconFileText size={18} />
            </ActionIcon>
          </Group>
      )
    },
  }), [cancellingRunNumber, handleCancelRun, onCancel, onRerun, onViewRun, pipeline, rerunLoading])

  const additionalColumns = useMemo(() => [actionsColumn], [actionsColumn])

  const headerActions = useMemo(() => {
    const shouldShow = !isMobile && activeTab === 'history'
    const isLoading = isLoadingPipeline || !pipeline

    if (isLoading && shouldShow) {
      return (
        <Box style={{ minWidth: 'fit-content' }}>
          <Skeleton height="1.75rem" width="144px" radius="sm" />
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
          variant="light"
          size="xs"
          leftSection={<IconAdjustments size={14} />}
          onClick={openCustomizeModal}
          disabled={!shouldShow}
          tabIndex={shouldShow ? 0 : -1}
        >
          Customize Columns
        </Button>
      </Box>
    )
  }, [isMobile, activeTab, openCustomizeModal, isLoadingPipeline, pipeline])

  const { columns, allColumns } = useTableColumns(
    pipeline?.provider_id,
    'pipeline_runs',
    additionalColumns,
    columnPreferences
  )

  const safeRuns = useMemo(() => {
    return runs && Array.isArray(runs) ? runs : []
  }, [runs])

  const branches = useMemo(() => {
    if (!safeRuns || !Array.isArray(safeRuns)) {
return []
}
    
return Array.from(new Set(safeRuns.map((r) => r.branch).filter((b): b is string => !!b)))
  }, [safeRuns])

  const actors = useMemo(() => {
    if (!safeRuns || !Array.isArray(safeRuns)) {
return []
}
    
return Array.from(new Set(safeRuns.map((r) => r.actor).filter((a): a is string => !!a)))
  }, [safeRuns])

  const previousPipelineIdRef = useRef<string | null>(null)


  useEffect(() => {
    const pipelineChanged = previousPipelineIdRef.current !== null &&
                           previousPipelineIdRef.current !== pipeline?.id

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
} catch { /* listener already removed */ }
        
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
} catch { /* listener already removed */ }
        
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
        } catch { /* listener may already be removed */ }
      })
    }
  }, [pipeline, refetchRuns, setFilter])

  const dateFilteredRuns = useMemo(() => {
    if (!safeRuns || !Array.isArray(safeRuns)) {
return []
}
    if (!dateRange || !dateRange.trim()) {
return safeRuns
}

    const now = new Date()
    const cutoffDate = new Date()

    switch (dateRange) {
      case 'today':
        cutoffDate.setHours(0, 0, 0, 0)
        break
      case '24h':
        cutoffDate.setHours(now.getHours() - 24)
        break
      case '7d':
        cutoffDate.setDate(now.getDate() - 7)
        break
      case '30d':
        cutoffDate.setDate(now.getDate() - 30)
        break
      case '90d':
        cutoffDate.setDate(now.getDate() - 90)
        break
      default:
        return safeRuns
    }

    return safeRuns.filter((run) =>
      run.started_at ? new Date(run.started_at) >= cutoffDate : false
    )
  }, [safeRuns, dateRange])

  const filteredRuns = useMemo(() => {
    let result = dateFilteredRuns

    if (search) {
      result = result.filter(
        (run) =>
          (run.branch && run.branch.toLowerCase().includes(search.toLowerCase())) ||
          (run.commit_sha && run.commit_sha.toLowerCase().includes(search.toLowerCase())) ||
          (run.commit_message && run.commit_message.toLowerCase().includes(search.toLowerCase())) ||
          (run.actor && run.actor.toLowerCase().includes(search.toLowerCase()))
      )
    }

    if (statusFilter) {
      result = result.filter((run) => run.status === statusFilter)
    }

    if (branchFilter) {
      result = result.filter((run) => run.branch === branchFilter)
    }

    if (actorFilter) {
      result = result.filter((run) => run.actor === actorFilter)
    }

    return result
  }, [dateFilteredRuns, search, statusFilter, branchFilter, actorFilter])

  const sortedRuns = useMemo(() => {
    if (!filteredRuns || !Array.isArray(filteredRuns)) {
return []
}
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
        return direction === 'asc'
          ? aVal.localeCompare(bVal)
          : bVal.localeCompare(aVal)
      }

      if (typeof aVal === 'number' && typeof bVal === 'number') {
        return direction === 'asc' ? aVal - bVal : bVal - aVal
      }

      return 0
    })

    return sorted
  }, [filteredRuns, sortStatus])

  const mobileRuns = useMemo(() => {
    if (!isMobile) {
return sortedRuns
}

    let result = [...accumulatedRuns]

    if (dateRange && dateRange.trim()) {
      const now = new Date()
      const cutoffDate = new Date()


      switch (dateRange) {
        case 'today':
          cutoffDate.setHours(0, 0, 0, 0)
          break
        case '24h':
          cutoffDate.setHours(now.getHours() - 24)
          break
        case '7d':
          cutoffDate.setDate(now.getDate() - 7)
          break
        case '30d':
          cutoffDate.setDate(now.getDate() - 30)
          break
        case '90d':
          cutoffDate.setDate(now.getDate() - 90)
          break
      }
      result = result.filter(run => run.started_at ? new Date(run.started_at) >= cutoffDate : false)
    }

    if (search) {
      result = result.filter(run =>
        run.branch?.toLowerCase().includes(search.toLowerCase()) ||
        run.commit_sha?.toLowerCase().includes(search.toLowerCase()) ||
        run.commit_message?.toLowerCase().includes(search.toLowerCase()) ||
        run.actor?.toLowerCase().includes(search.toLowerCase())
      )
    }

    if (statusFilter) {
result = result.filter(run => run.status === statusFilter)
}
    if (branchFilter) {
result = result.filter(run => run.branch === branchFilter)
}
    if (actorFilter) {
result = result.filter(run => run.actor === actorFilter)
}

    return result
  }, [isMobile, accumulatedRuns, sortedRuns, search, statusFilter, branchFilter, actorFilter, dateRange])

  const renderMobileCards = () => {
    return (
      <Stack gap="md">
        {mobileRuns.map((run) => {
          const isRunning = run.status === 'running' || run.status === 'pending'
          const isCancelling = cancellingRunNumber === run.run_number

          return (
            <Card
              key={run.run_number}
              padding="lg"
              withBorder
              style={{
                cursor: 'pointer',
                transition: 'all 150ms ease',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.boxShadow = '0 4px 12px rgba(0, 0, 0, 0.15)'
                e.currentTarget.style.transform = 'translateY(-2px)'
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.boxShadow = ''
                e.currentTarget.style.transform = ''
              }}
              onClick={() => onViewRun(pipeline!.id, run.run_number)}
            >
              <Stack gap="xs">
                <Group justify="space-between" wrap="nowrap">
                  <Group gap={8} wrap="nowrap">
                    <Text size={THEME_TYPOGRAPHY.ITEM_TITLE.size} fw={THEME_TYPOGRAPHY.ITEM_TITLE.weight}>
                      #{run.run_number}
                    </Text>
                    {TableCells.status(run.status)}
                  </Group>
                  <Group gap={4}>
                    {isRunning ? (
                      onCancel && (
                        <ActionIcon
                          variant="subtle"
                          color="red"
                          size="md"
                          onClick={(e) => {
                            e.stopPropagation()
                            handleCancelRun(pipeline!, run)
                          }}
                          disabled={isCancelling || run.status === 'pending'}
                          title={run.status === 'pending' ? 'Cannot cancel pending workflow' : 'Stop run'}
                          aria-label={run.status === 'pending' ? 'Cannot cancel pending workflow' : 'Stop run'}
                          style={{
                            backgroundColor: 'transparent',
                            cursor: (isCancelling || run.status === 'pending') ? 'not-allowed' : 'pointer',
                          }}
                        >
                          <IconSquare
                            size={18}
                            style={{
                              animation: isCancelling ? 'spin 1s linear infinite' : 'none',
                            }}
                          />
                        </ActionIcon>
                      )
                    ) : (
                      onRerun && (
                        <ActionIcon
                          variant="subtle"
                          color="blue"
                          size="md"
                          onClick={(e) => {
                            e.stopPropagation()
                            onRerun(pipeline!, run)
                          }}
                          title="Rerun workflow"
                          aria-label="Rerun workflow"
                        >
                          <IconRefresh size={18} />
                        </ActionIcon>
                      )
                    )}
                    <ActionIcon
                      variant="subtle"
                      color="blue"
                      size="md"
                      onClick={(e) => {
                        e.stopPropagation()
                        onViewRun(pipeline!.id, run.run_number)
                      }}
                      title="View details"
                      aria-label="View details"
                    >
                      <IconFileText size={18} />
                    </ActionIcon>
                  </Group>
                </Group>

                <Group gap="lg" wrap="nowrap" align="flex-start">
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconGitBranch size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Branch</Text>
                    </Group>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT} truncate>{run.branch || '—'}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconUser size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Actor</Text>
                    </Group>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT} truncate>{run.actor || '—'}</Text>
                  </Box>
                </Group>

                <Group gap="lg" wrap="nowrap" align="flex-start">
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconClock size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Duration</Text>
                    </Group>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{formatDuration(run.duration_seconds)}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconCalendar size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Started</Text>
                    </Group>
                    <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT} truncate>
                      {run.started_at ? new Date(run.started_at).toLocaleString() : '—'}
                    </Text>
                  </Box>
                </Group>

                {run.commit_sha && (
                  <Box>
                    <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Commit</Text>
                    {TableCells.commit(run.commit_sha)}
                  </Box>
                )}
              </Stack>
            </Card>
          )
        })}
        <Box ref={loadMoreRef} py="md">
          {pageLoading && hasMore && (
            <Center><Loader size="sm" /></Center>
          )}
          {!hasMore && mobileRuns.length > 0 && (
            <Text size="sm" c="dimmed" ta="center">
              {isComplete ? `All ${totalCount} runs` : `${totalCount}+ runs`}
            </Text>
          )}
        </Box>
      </Stack>
    )
  }

  return (
    <Box
      pt={{ base: 0, sm: 'sm' }}
      pb={{ base: 0, sm: 'md' }}
      px={{ base: 'xs', sm: 'xl' }}
      style={{
        width: '100%',
        maxWidth: '100%',
        display: 'flex',
        flexDirection: 'column',
        flex: 1,
        minHeight: 0,
        background: 'var(--mantine-color-body)',
      }}
    >
      <PageHeader
        title={
          !isLoadingPipeline && pipeline?.name ? (
            pipeline.name
          ) : (
            <Skeleton height="1.5rem" width="min(250px, 60vw)" />
          )
        }
        onBack={onBack}
        actions={headerActions}
      />

      <Tabs value={activeTab} onChange={(value) => setActiveTab(value || 'history')}>
        <Tabs.List mb="md" style={{ minHeight: '2.5rem' }}>
          <Tabs.Tab value="history" leftSection={<IconHistory size={16} />}>
            Run History
          </Tabs.Tab>
          <Tabs.Tab value="metrics" leftSection={<IconChartLine size={16} />}>
            Metrics
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="history" style={{ width: '100%', display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
          {isLoadingPipeline || loading || !preferencesLoaded || !pipeline?.id ? (
            <Center py="xl">
              <Loader size="lg" />
            </Center>
          ) : (
            <>
              <Group justify="space-between" align="flex-start" wrap="nowrap">
                <FilterBar
                  filters={{
                    search: {
                      value: search,
                      onChange: (value) => setFilter('search', value),
                      placeholder: 'Search runs...',
                    },
                    status: {
                      value: statusFilter,
                      onChange: (value) => setFilter('status', value),
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
                {isError && (
                  <Button
                    size="xs"
                    variant="light"
                    color="red"
                    leftSection={<IconRefresh size={14} />}
                    onClick={() => refetchRuns()}
                  >
                    Retry
                  </Button>
                )}
              </Group>

          {isMobile ? (
            renderMobileCards()
          ) : (
            <Box style={{
              height: 'calc(100vh - var(--app-shell-header-height, 70px) - 220px)',
              minHeight: '400px',
              position: 'relative'
            }}>
              <StandardTable<PipelineRun>
              records={sortedRuns}
              onRowClick={({ record }) => onViewRun(pipeline!.id, record.run_number)}
              rowStyle={() => ({ cursor: 'pointer' })}
              columns={columns}
              sortStatus={sortStatus}
              onSortStatusChange={setSortStatus}
              fetching={pageLoading}
              noRecordsText={
                pageLoading
                  ? ''
                  : isError
                    ? 'Failed to load runs. Click refresh to retry.'
                    : safeRuns.length === 0
                      ? 'No runs found'
                      : 'No matching runs'
              }
              totalRecords={effectiveTotalRecords}
              recordsPerPage={PAGE_SIZE}
              page={page}
              onPageChange={(p) => setFilter('page', p)}
              paginationText={({ from, to }) =>
                pageLoading
                  ? 'Loading...'
                  : isError
                    ? 'Error loading runs - use pagination to retry'
                    : isComplete
                      ? `Showing ${from}-${to} of ${totalCount} runs`
                      : `Showing ${from}-${to} runs`
              }
            />
            </Box>
          )}
            </>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="metrics" style={{ width: '100%' }}>
          {isLoadingPipeline || !pipeline ? (
            <Center py="xl">
              <Loader size="lg" />
            </Center>
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

      <style>{`
        @keyframes spin {
          from {
            transform: rotate(0deg);
          }
          to {
            transform: rotate(360deg);
          }
        }
      `}</style>
    </Box>
  )
}
