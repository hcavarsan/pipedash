import { useEffect, useMemo, useState } from 'react'
import { DataTableSortStatus } from 'mantine-datatable'

import { ActionIcon, Box, Card, Center, Container, Group, Loader, Stack, Tabs, Text } from '@mantine/core'
import { IconCalendar, IconChartLine, IconClock, IconFileText, IconGitBranch, IconHistory, IconRefresh, IconSquare, IconUser } from '@tabler/icons-react'
import { listen } from '@tauri-apps/api/event'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import { tauriService } from '../../services/tauri'
import type { Pipeline, PipelineRun } from '../../types'
import { formatDuration } from '../../utils/formatDuration'
import { TableCells } from '../../utils/tableCells'
import { COLUMN_PRESETS } from '../../utils/tableColumns'
import { FilterBar } from '../common/FilterBar'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { PipelineMetricsView } from '../pipeline/PipelineMetricsView'

interface RunHistoryPageProps {
  pipeline: Pipeline | null;
  onBack: () => void;
  onViewRun: (pipelineId: string, runNumber: number) => void;
  onRerun?: (pipeline: Pipeline, run: PipelineRun) => void;
  onCancel?: (pipeline: Pipeline, run: PipelineRun) => void;
  refreshTrigger?: number;
  onLoadingChange?: (loading: boolean) => void;
  initialTab?: 'history' | 'metrics';
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
}: RunHistoryPageProps) => {
  const isMobile = useIsMobile()
  const [runs, setRuns] = useState<PipelineRun[]>([])
  const [totalCount, setTotalCount] = useState(0)
  const [totalPages, setTotalPages] = useState(0)
  const [_hasMore, setHasMore] = useState(false)
  const [isComplete, setIsComplete] = useState(false)
  const [loading, setLoading] = useState(false)
  const [pageLoading, setPageLoading] = useState(false)
  const [initialLoad, setInitialLoad] = useState(true)
  const [cancellingRunNumber, setCancellingRunNumber] = useState<number | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [branchFilter, setBranchFilter] = useState<string | null>(null)
  const [actorFilter, setActorFilter] = useState<string | null>(null)
  const [dateRange, setDateRange] = useState<string | null>(null)
  const [page, setPage] = useState(1)
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<PipelineRun>>({
    columnAccessor: 'run_number',
    direction: 'desc',
  })
  const [activeTab, setActiveTab] = useState<string>(initialTab)

  const PAGE_SIZE = 20

  // Extract unique values for filters
  const branches = useMemo(() => {
    return Array.from(new Set(runs.map((r) => r.branch).filter((b): b is string => !!b)))
  }, [runs])

  const actors = useMemo(() => {
    return Array.from(new Set(runs.map((r) => r.actor).filter((a): a is string => !!a)))
  }, [runs])

  const loadRuns = async (showLoading = true, targetPage = page, clearCache = false) => {
    if (!pipeline) {
      setRuns([])
      setTotalCount(0)
      setTotalPages(0)
      setHasMore(false)
      setIsComplete(false)

return
    }

    try {
      // Clear cache if requested (for manual refresh)
      if (clearCache) {
        await tauriService.clearRunHistoryCache(pipeline.id)
      }

      // Show full loading only on initial load
      if (showLoading && initialLoad) {
        setLoading(true)
        if (onLoadingChange) {
onLoadingChange(true)
}
      } else if (showLoading) {
        // Show small page loading indicator for page changes
        setPageLoading(true)
        if (onLoadingChange) {
onLoadingChange(true)
}
      }

      const data = await tauriService.fetchRunHistory(pipeline.id, targetPage, PAGE_SIZE)

      setRuns(data.runs)
      setTotalCount(data.total_count)
      setTotalPages(data.total_pages)
      setHasMore(data.has_more)
      setIsComplete(data.is_complete)
    } catch (error: any) {
      const errorMsg = error?.error || error?.message || 'Failed to load run history'

      console.error('Failed to load run history:', errorMsg)
      setRuns([])
      setTotalCount(0)
      setTotalPages(0)
      setHasMore(false)
      setIsComplete(false)
    } finally {
      setLoading(false)
      setPageLoading(false)
      if (onLoadingChange) {
onLoadingChange(false)
}
      if (initialLoad) {
        setInitialLoad(false)
      }
    }
  }

  useEffect(() => {
    setInitialLoad(true)
    setPage(1)
    loadRuns(true, 1)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pipeline])

  useEffect(() => {
    if (refreshTrigger !== undefined && refreshTrigger > 0) {
      loadRuns(true, page, true)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshTrigger])

  // Fetch data when page changes
  useEffect(() => {
    if (!initialLoad && pipeline) {
      loadRuns(false)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page])

  useEffect(() => {
    if (!pipeline) {
return
}

    const unlistenPromises: Promise<() => void>[] = []

    unlistenPromises.push(
      listen<string>('run-triggered', (event) => {
        if (event.payload === pipeline.id) {
          console.log('[RunHistory] Detected new run for pipeline:', pipeline.id)
          setPage(1)
          loadRuns(false, 1)
        }
      })
    )

    unlistenPromises.push(
      listen<string>('run-cancelled', (event) => {
        if (event.payload === pipeline.id) {
          console.log('[RunHistory] Detected cancelled run for pipeline:', pipeline.id)
          loadRuns(false, page)
        }
      })
    )

    return () => {
      Promise.all(unlistenPromises).then((unlisteners) => {
        unlisteners.forEach((unlisten) => unlisten())
      })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pipeline?.id])

  const handleCancelRun = async (pipeline: Pipeline, run: PipelineRun) => {
    setCancellingRunNumber(run.run_number)
    try {
      if (onCancel) {
        await onCancel(pipeline, run)
      }
    } finally {
      setCancellingRunNumber(null)
    }
  }

  const dateFilteredRuns = useMemo(() => {
    if (!dateRange || !dateRange.trim()) {
return runs
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
        return runs
    }

    return runs.filter((run) =>
      run.started_at ? new Date(run.started_at) >= cutoffDate : false
    )
  }, [runs, dateRange])

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

  // Note: Server already sorts by run_number desc, client-side sorting/filtering works on current page only
  const sortedRuns = useMemo(() => {
    const sorted = [...filteredRuns]
    const { columnAccessor, direction } = sortStatus

    sorted.sort((a, b) => {
      let aVal: any = a[columnAccessor as keyof PipelineRun]
      let bVal: any = b[columnAccessor as keyof PipelineRun]

      if (columnAccessor === 'started_at' || columnAccessor === 'concluded_at') {
        aVal = aVal ? new Date(aVal).getTime() : 0
        bVal = bVal ? new Date(bVal).getTime() : 0
      }

      if (aVal === null || aVal === undefined) {
aVal = ''
}
      if (bVal === null || bVal === undefined) {
bVal = ''
}

      if (typeof aVal === 'string') {
        return direction === 'asc'
          ? aVal.localeCompare(bVal)
          : bVal.localeCompare(aVal)
      }

      return direction === 'asc' ? aVal - bVal : bVal - aVal
    })

    return sorted
  }, [filteredRuns, sortStatus])

  const renderMobileCards = () => {
    return (
      <Stack gap="sm">
        {sortedRuns.map((run) => {
          const isRunning = run.status === 'running' || run.status === 'pending'
          const isCancelling = cancellingRunNumber === run.run_number

          return (
            <Card
              key={run.run_number}
              padding="md"
              withBorder
              style={{ cursor: 'pointer' }}
              onClick={() => onViewRun(pipeline!.id, run.run_number)}
            >
              <Stack gap="xs">
                <Group justify="space-between" wrap="nowrap">
                  <Group gap={8} wrap="nowrap">
                    <Text size="sm" fw={600} c="blue">
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
                    >
                      <IconFileText size={18} />
                    </ActionIcon>
                  </Group>
                </Group>

                <Group gap="md" wrap="nowrap" align="flex-start">
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconGitBranch size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size="xs" c="dimmed">Branch</Text>
                    </Group>
                    <Text size="sm" truncate>{run.branch || '—'}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconUser size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size="xs" c="dimmed">Actor</Text>
                    </Group>
                    <Text size="sm" truncate>{run.actor || '—'}</Text>
                  </Box>
                </Group>

                <Group gap="md" wrap="nowrap" align="flex-start">
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconClock size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size="xs" c="dimmed">Duration</Text>
                    </Group>
                    <Text size="sm">{formatDuration(run.duration_seconds)}</Text>
                  </Box>
                  <Box style={{ flex: 1, minWidth: 0 }}>
                    <Group gap={4} wrap="nowrap">
                      <IconCalendar size={14} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                      <Text size="xs" c="dimmed">Started</Text>
                    </Group>
                    <Text size="sm" truncate>
                      {run.started_at ? new Date(run.started_at).toLocaleString() : '—'}
                    </Text>
                  </Box>
                </Group>

                {run.commit_sha && (
                  <Box>
                    <Text size="xs" c="dimmed">Commit</Text>
                    {TableCells.commit(run.commit_sha)}
                  </Box>
                )}
              </Stack>
            </Card>
          )
        })}
        {/* Mobile pagination info */}
        <Text size="sm" c="dimmed" ta="center" py="sm">
          Page {page} of {totalPages} ({isComplete ? `${totalCount}` : `${totalCount}+`} runs)
        </Text>
        {totalPages > 1 && (
          <Group justify="center" gap="sm">
            <ActionIcon
              variant="light"
              onClick={() => setPage(p => Math.max(1, p - 1))}
              disabled={page === 1 || pageLoading}
            >
              ←
            </ActionIcon>
            <Text size="sm">{page}</Text>
            <ActionIcon
              variant="light"
              onClick={() => setPage(p => Math.min(totalPages, p + 1))}
              disabled={page >= totalPages || pageLoading}
            >
              →
            </ActionIcon>
          </Group>
        )}
      </Stack>
    )
  }

  if (!pipeline) {
    return (
      <Container size="100%" pt={{ base: 'xs', sm: 'sm' }} pb={{ base: 'xs', sm: '2xl' }} px={{ base: 'xs', sm: 'xl' }}>
        <Center py="xl">
          <Loader size="lg" />
        </Center>
      </Container>
    )
  }

  return (
    <Container size="100%" pt={{ base: 'xs', sm: 'sm' }} pb={{ base: 'xs', sm: '2xl' }} px={{ base: 'xs', sm: 'xl' }} style={{ maxWidth: '100%' }}>
      <PageHeader
        title={pipeline.name}
        onBack={onBack}
      />

      <Tabs value={activeTab} onChange={(value) => setActiveTab(value || 'history')}>
        <Tabs.List mb="md">
          <Tabs.Tab value="history" leftSection={<IconHistory size={16} />}>
            Run History
          </Tabs.Tab>
          <Tabs.Tab value="metrics" leftSection={<IconChartLine size={16} />}>
            Metrics
          </Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="history">
          {loading ? (
            <Center py="xl">
              <Loader size="lg" />
            </Center>
          ) : (
            <>
              <FilterBar
            filters={{
              search: {
                value: search,
                onChange: setSearch,
                placeholder: 'Search runs...',
              },
              status: {
                value: statusFilter,
                onChange: setStatusFilter,
              },
              branch: {
                value: branchFilter,
                onChange: setBranchFilter,
                options: branches,
              },
              actor: {
                value: actorFilter,
                onChange: setActorFilter,
                options: actors,
              },
              dateRange: {
                value: dateRange,
                onChange: setDateRange,
              },
            }}
          />

          {isMobile ? (
            renderMobileCards()
          ) : (
            <StandardTable<PipelineRun>
            records={sortedRuns}
            onRowClick={({ record }) => onViewRun(pipeline!.id, record.run_number)}
            rowStyle={() => ({ cursor: 'pointer' })}
            columns={[
              {
                accessor: 'run_number',
                title: 'Run',
                sortable: true,
                ...COLUMN_PRESETS.identifier,
                render: (run) => (
                  <div
                    style={{ cursor: 'pointer' }}
                    onClick={(e) => {
                      e.stopPropagation()
                      tauriService.openUrl(run.logs_url)
                    }}
                  >
                    {TableCells.textBold(`#${run.run_number}`)}
                  </div>
                ),
              },
              {
                accessor: 'status',
                title: 'Status',
                sortable: true,
                ...COLUMN_PRESETS.status,
                render: (run) => TableCells.status(run.status),
              },
              {
                accessor: 'branch',
                title: 'Branch',
                sortable: true,
                ...COLUMN_PRESETS.branch,
                render: (run) => TableCells.truncatedDimmed(run.branch),
              },
              {
                accessor: 'started_at',
                title: 'Started',
                sortable: true,
                ...COLUMN_PRESETS.timestamp,
                render: (run) => TableCells.timestamp(run.started_at),
              },
              {
                accessor: 'duration_seconds',
                title: 'Duration',
                sortable: true,
                ...COLUMN_PRESETS.duration,
                render: (run) => TableCells.textBold(formatDuration(run.duration_seconds)),
              },
              {
                accessor: 'commit_sha',
                title: 'Commit',
                ...COLUMN_PRESETS.commit,
                render: (run) => TableCells.commit(run.commit_sha),
              },
              {
                accessor: 'actions',
                title: 'Actions',
                width: 150,
                textAlign: 'center' as const,
                render: (run) => {
                  const isRunning = run.status === 'running' || run.status === 'pending'
                  const isCancelling = cancellingRunNumber === run.run_number

                  return (
                    <div onClick={(e) => e.stopPropagation()}>
                      <Group gap={4} wrap="nowrap" justify="center">
                        {isRunning ? (
                          onCancel && (
                            <ActionIcon
                              variant="subtle"
                              color="red"
                              size="md"
                              onClick={(e) => {
                                e.stopPropagation()
                                handleCancelRun(pipeline, run)
                              }}
                              title={run.status === 'pending' ? 'Cannot cancel pending workflow' : 'Stop run'}
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
                                onRerun(pipeline, run)
                              }}
                              title="Rerun workflow"
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
                            onViewRun(pipeline.id, run.run_number)
                          }}
                          title="View details"
                        >
                          <IconFileText size={18} />
                        </ActionIcon>
                      </Group>
                    </div>
                  )
                },
              },
            ]}
            sortStatus={sortStatus}
            onSortStatusChange={setSortStatus}
            noRecordsText={runs.length === 0 ? 'No runs found' : 'No matching runs'}
            totalRecords={totalPages * PAGE_SIZE}
            recordsPerPage={PAGE_SIZE}
            page={page}
            onPageChange={setPage}
            paginationText={({ from, to }) =>
              pageLoading
                ? 'Loading...'
                : `Showing ${from}-${to} of ${isComplete ? totalCount : `${totalCount}+`} runs`
            }
          />
          )}
            </>
          )}
        </Tabs.Panel>

        <Tabs.Panel value="metrics">
          <PipelineMetricsView
            pipelineId={pipeline.id}
            pipelineName={pipeline.name}
            repository={pipeline.repository}
            refreshTrigger={refreshTrigger}
          />
        </Tabs.Panel>
      </Tabs>

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
    </Container>
  )
}
