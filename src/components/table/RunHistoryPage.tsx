import {
  ActionIcon,
  Box,
  Button,
  Card,
  Checkbox,
  Group,
  Loader,
  Menu,
  Skeleton,
  Stack,
  Tabs,
  Text,
  Tooltip,
  UnstyledButton,
} from '@mantine/core'
import { useDisclosure, useIntersection } from '@mantine/hooks'
import {
  IconAdjustments,
  IconCalendar,
  IconChartLine,
  IconCheck,
  IconClock,
  IconDots,
  IconEye,
  IconFileText,
  IconGitBranch,
  IconHistory,
  IconPlayerPlay,
  IconRefresh,
  IconSquare,
  IconUser,
  IconX,
} from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { PAGE_SIZES } from '../../constants/pagination'
import { usePlugins } from '../../contexts/PluginContext'
import { useBulkCancelRuns } from '../../hooks/useBulkCancelRuns'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useTableColumns } from '../../hooks/useTableColumns'
import { useRunHistoryFilters } from '../../hooks/useUrlState'
import { useBuildWatch } from '../../queries/useBuildWatchQueries'
import {
  useClearRunHistoryCache,
  useRunHistory,
  useSaveTablePreferences,
  useTablePreferences,
} from '../../queries/useRunHistoryQueries'
import { useTableDefinition } from '../../queries/useTableSchemaQueries'
import { events } from '../../services'
import { useModalStore } from '../../stores/modalStore'
import { usePullToRefresh } from '../../stores/pullRefreshStore'
import { STATUS_META } from '../../theme/status'
import { TYPOGRAPHY } from '../../theme/typography'
import type { Pipeline, PipelineRun, ReviewDecision } from '../../types'
import { getDateRangeStart } from '../../utils/dateRange'
import { formatCount } from '../../utils/formatCount'
import { formatDateTime } from '../../utils/formatDateTime'
import { formatDuration } from '../../utils/formatDuration'
import {
  canRerunInPlace,
  getPipelineLabels,
  getStatusLabel,
  getStatusOptions,
  getTriggerBlockReason,
} from '../../utils/pipelinePresentation'
import type { RerunMode } from '../../utils/rerunSummary'
import { TableCells } from '../../utils/tableCells'
import { StatusBadge } from '../atoms/StatusBadge'
import { useContextMenu } from '../common/ContextMenu'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { IosGroup, IosRow } from '../ios/IosList'
import type { SwipeAction } from '../ios/SwipeActions'
import { PipelineMetricsView } from '../pipeline/PipelineMetricsView'
import { RerunConfirmModal } from '../pipeline/RerunConfirmModal'

import { IosRunList, IosRunToolbar } from './IosRunList'
import { triggerIcon } from './PipelineRowParts'
import { TableCustomizationModal } from './TableCustomizationModal'

export type DetailTab = 'history' | 'metrics'

interface RunHistoryPageProps {
  pipeline: Pipeline | null
  onBack: () => void
  onViewRun: (pipelineId: string, runNumber: number) => void
  onRerun?: (pipeline: Pipeline, run: PipelineRun) => void
  onCancel?: (pipeline: Pipeline, run: PipelineRun) => Promise<void>
  onReview?: (pipeline: Pipeline, run: PipelineRun, decision: ReviewDecision) => Promise<void>
  onRerunFailedJobs?: (pipeline: Pipeline, run: PipelineRun) => Promise<void>
  refreshTrigger?: number
  onLoadingChange?: (loading: boolean) => void
  tab: DetailTab
  onTabChange: (tab: DetailTab) => void
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
  onReview,
  onRerunFailedJobs,
  refreshTrigger,
  onLoadingChange,
  tab,
  onTabChange,
  isLoadingPipeline = false,
}: RunHistoryPageProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const buildWatch = useBuildWatch()
  const { getPluginDisplayName } = usePlugins()
  const watching = buildWatch.data?.watch?.monitoring === 'watching'
  const watchDisabled =
    buildWatch.busy || buildWatch.isFetching || buildWatch.isError || !buildWatch.data?.available || watching
  const labels = getPipelineLabels(pipeline?.provider_type)
  const isFlux = pipeline?.provider_type === 'fluxcd'
  const triggerBlockedReason = pipeline ? getTriggerBlockReason(pipeline) : undefined
  const openTrigger = useCallback(() => {
    if (pipeline && !getTriggerBlockReason(pipeline)) {
      useModalStore.getState().openTriggerModal(pipeline.id)
    }
  }, [pipeline])

  const { filters, setFilter, clearFilters } = useRunHistoryFilters()
  const { search, status: statusFilter, branch: branchFilter, actor: actorFilter, dateRange, page } = filters

  const [cancellingRunNumber, setCancellingRunNumber] = useState<number | null>(null)
  const [reviewingRunNumber, setReviewingRunNumber] = useState<number | null>(null)
  const [rerunningFailedRunNumber, setRerunningFailedRunNumber] = useState<number | null>(null)
  const [rerunTarget, setRerunTarget] = useState<PipelineRun | null>(null)
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

  const [customizeModalOpened, { open: openCustomizeModal, close: closeCustomizeModal }] = useDisclosure(false)
  const [selecting, setSelecting] = useState(false)
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

  usePullToRefresh(mobileShell ? () => refetchRuns() : null)

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
    async (pipeline: Pipeline, run: PipelineRun) => {
      setCancellingRunNumber(run.run_number)
      try {
        await onCancel?.(pipeline, run)
      } finally {
        setCancellingRunNumber(null)
      }
    },
    [onCancel]
  )

  const handleReviewRun = useCallback(
    async (pipeline: Pipeline, run: PipelineRun, decision: ReviewDecision) => {
      setReviewingRunNumber(run.run_number)
      try {
        if (onReview) {
          await onReview(pipeline, run, decision)
        }
      } finally {
        setReviewingRunNumber(null)
      }
    },
    [onReview]
  )

  const handleRerunFailedJobs = useCallback(
    async (pipeline: Pipeline, run: PipelineRun) => {
      setRerunningFailedRunNumber(run.run_number)
      try {
        if (onRerunFailedJobs) {
          await onRerunFailedJobs(pipeline, run)
        }
      } finally {
        setRerunningFailedRunNumber(null)
      }
    },
    [onRerunFailedJobs]
  )

  const requestRerun = useCallback(
    (pipeline: Pipeline, run: PipelineRun) => {
      if (canRerunInPlace(pipeline)) {
        setRerunTarget(run)
      } else {
        onRerun?.(pipeline, run)
      }
    },
    [onRerun]
  )

  const confirmRerun = async (mode: RerunMode) => {
    if (pipeline && rerunTarget) {
      await (mode === 'failed' ? handleRerunFailedJobs(pipeline, rerunTarget) : onRerun?.(pipeline, rerunTarget))
    }
  }

  const rerunLoading = useModalStore((s) => s.rerunLoading)

  const getRunActions = useCallback(
    (run: PipelineRun, withDetails: boolean) => {
      if (!pipeline) {
        return []
      }
      const active = run.status === 'running' || run.status === 'pending'
      const awaitingReview = run.metadata?.awaiting_review === true
      const actions: {
        label: string
        icon: ReactNode
        onClick: () => void
        color?: 'red' | 'green'
        disabled?: boolean
        loading?: boolean
      }[] = []
      if (active && buildWatch.data?.supported) {
        actions.push({
          label: buildWatch.data.available
            ? `Watch run #${run.run_number}`
            : `Watch unavailable: ${buildWatch.data.reason ?? 'Allow Live Activities and background execution in iOS Settings.'}`,
          icon: <IconEye size={16} />,
          disabled: watchDisabled,
          loading: buildWatch.busy,
          onClick: () => buildWatch.start(pipeline.id, run.run_number),
        })
      }
      if (awaitingReview && onReview) {
        actions.push(
          {
            label: `Approve deployment review for run #${run.run_number}`,
            icon: <IconCheck size={16} />,
            color: 'green',
            disabled: reviewingRunNumber !== null,
            loading: reviewingRunNumber === run.run_number,
            onClick: () => {
              void handleReviewRun(pipeline, run, 'approve')
            },
          },
          {
            label: `Reject deployment review for run #${run.run_number}`,
            icon: <IconX size={16} />,
            color: 'red',
            disabled: reviewingRunNumber !== null,
            onClick: () => {
              void handleReviewRun(pipeline, run, 'reject')
            },
          }
        )
      }
      if (active && onCancel && !awaitingReview) {
        actions.push({
          label:
            run.status === 'pending'
              ? `Cancel unavailable for pending run #${run.run_number}`
              : `Cancel run #${run.run_number}`,
          icon: <IconSquare size={16} />,
          color: 'red',
          disabled: cancellingRunNumber === run.run_number || run.status === 'pending',
          loading: cancellingRunNumber === run.run_number,
          onClick: () => {
            void handleCancelRun(pipeline, run)
          },
        })
      } else if (!active && onRerun) {
        const busy =
          (rerunLoading?.pipelineId === pipeline.id && rerunLoading.runNumber === run.run_number) ||
          rerunningFailedRunNumber === run.run_number
        actions.push({
          label: isFlux ? `Reconcile ${pipeline.name}` : `Rerun run #${run.run_number}`,
          icon: <IconPlayerPlay size={16} />,
          disabled: busy,
          loading: busy,
          onClick: () => requestRerun(pipeline, run),
        })
      }
      if (withDetails) {
        actions.push({
          label: `View details for ${labels.run} #${run.run_number}`,
          icon: <IconFileText size={16} />,
          onClick: () => onViewRun(pipeline.id, run.run_number),
        })
      }
      return actions
    },
    [
      pipeline,
      onReview,
      reviewingRunNumber,
      handleReviewRun,
      onCancel,
      cancellingRunNumber,
      handleCancelRun,
      onRerun,
      requestRerun,
      rerunLoading,
      isFlux,
      rerunningFailedRunNumber,
      labels.run,
      onViewRun,
      buildWatch,
      watchDisabled,
    ]
  )

  const contextMenu = useContextMenu()
  const runMenu = (run: PipelineRun) =>
    contextMenu(() =>
      getRunActions(run, true).map(({ color, ...action }) => ({
        ...action,
        color: color === 'red' ? ('red' as const) : undefined,
      }))
    )

  const renderRunActions = useCallback(
    (run: PipelineRun, withDetails: boolean) => (
      <Group gap="xs" wrap="nowrap" justify="flex-end">
        {getRunActions(run, withDetails).map(({ label, icon, onClick, ...props }) => (
          <Tooltip key={label} label={label}>
            <ActionIcon
              variant="subtle"
              size="md"
              color="gray"
              {...props}
              aria-label={label}
              onClick={(event) => {
                event.stopPropagation()
                onClick()
              }}
            >
              {icon}
            </ActionIcon>
          </Tooltip>
        ))}
      </Group>
    ),
    [getRunActions]
  )

  const getRunSwipeActions = (run: PipelineRun): SwipeAction[] => {
    if (!pipeline) {
      return []
    }

    const isActive = run.status === 'running' || run.status === 'pending'
    const isAwaitingReview = run.metadata?.awaiting_review === true
    const actions: SwipeAction[] = []

    if (isAwaitingReview && onReview && reviewingRunNumber === null) {
      actions.push({
        key: 'approve',
        label: 'Approve',
        tone: 'green',
        onSelect: () => handleReviewRun(pipeline, run, 'approve'),
      })
    }

    if (isActive) {
      if (onCancel && !isAwaitingReview && run.status !== 'pending' && cancellingRunNumber !== run.run_number) {
        actions.push({
          key: 'cancel',
          label: 'Cancel',
          tone: 'red',
          primary: true,
          onSelect: () => handleCancelRun(pipeline, run),
        })
      }
    } else if (onRerun && !(rerunLoading?.pipelineId === pipeline.id && rerunLoading.runNumber === run.run_number)) {
      actions.push({
        key: 'rerun',
        label: isFlux ? 'Reconcile' : 'Rerun',
        tone: 'blue',
        primary: true,
        onSelect: () => requestRerun(pipeline, run),
      })
    }

    return actions
  }

  const renderRunMenu = (run: PipelineRun) => {
    if (!pipeline) {
      return null
    }

    const isActive = run.status === 'running' || run.status === 'pending'
    const isAwaitingReview = run.metadata?.awaiting_review === true
    const isReviewBusy = reviewingRunNumber !== null
    const busy =
      cancellingRunNumber === run.run_number ||
      reviewingRunNumber === run.run_number ||
      rerunningFailedRunNumber === run.run_number ||
      (rerunLoading?.pipelineId === pipeline.id && rerunLoading.runNumber === run.run_number)

    return (
      <Menu position="bottom-end">
        <Menu.Target>
          <ActionIcon
            variant="subtle"
            color="gray"
            loading={busy}
            data-row-menu
            aria-label={`Actions for ${labels.run} #${run.run_number}`}
          >
            <IconDots size={22} />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          {isActive && buildWatch.data?.supported && (
            <Menu.Item
              leftSection={buildWatch.busy ? <Loader size="xs" /> : <IconEye size={16} />}
              disabled={watchDisabled}
              onClick={() => buildWatch.start(pipeline.id, run.run_number)}
            >
              {buildWatch.data.available
                ? 'Watch run'
                : `Watch unavailable: ${buildWatch.data.reason ?? 'Check Live Activities and background execution in iOS Settings.'}`}
            </Menu.Item>
          )}
          {isAwaitingReview && onReview && (
            <>
              <Menu.Item disabled={isReviewBusy} onClick={() => handleReviewRun(pipeline, run, 'approve')}>
                Approve deployment
              </Menu.Item>
              <Menu.Item color="red" disabled={isReviewBusy} onClick={() => handleReviewRun(pipeline, run, 'reject')}>
                Reject deployment
              </Menu.Item>
            </>
          )}
          {isActive
            ? onCancel &&
              !isAwaitingReview && (
                <Menu.Item
                  color="red"
                  disabled={run.status === 'pending' || cancellingRunNumber === run.run_number}
                  onClick={() => handleCancelRun(pipeline, run)}
                >
                  {run.status === 'pending' ? 'Cancel unavailable while pending' : `Cancel ${labels.run}`}
                </Menu.Item>
              )
            : onRerun && (
                <Menu.Item onClick={() => requestRerun(pipeline, run)}>
                  {isFlux ? `Reconcile ${pipeline.name}` : `Rerun ${labels.run}`}
                </Menu.Item>
              )}
          <Menu.Item onClick={() => onViewRun(pipeline.id, run.run_number)}>View details</Menu.Item>
          <ActionSheetCancel />
        </Menu.Dropdown>
      </Menu>
    )
  }

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
    const shouldShow = !isMobile && tab === 'history'

    if (isLoadingPipeline || !pipeline) {
      return (
        <Box style={{ minWidth: 'fit-content' }}>
          <Skeleton height="2.25rem" width="10rem" />
        </Box>
      )
    }

    return (
      <Group gap="sm" wrap="nowrap" style={{ minWidth: 'fit-content' }}>
        {shouldShow && (
          <Button
            variant="default"
            size="sm"
            leftSection={<IconAdjustments size={16} />}
            onClick={openCustomizeModal}
            disabled={schemaLoading || !!schemaError}
          >
            Customize columns
          </Button>
        )}
        <Tooltip label={triggerBlockedReason} disabled={!triggerBlockedReason} multiline w={280}>
          <Button
            size="sm"
            leftSection={triggerIcon(pipeline)}
            onClick={openTrigger}
            aria-disabled={triggerBlockedReason ? true : undefined}
            data-disabled={triggerBlockedReason ? true : undefined}
          >
            {labels.trigger}
          </Button>
        </Tooltip>
      </Group>
    )
  }, [
    isMobile,
    tab,
    openCustomizeModal,
    isLoadingPipeline,
    pipeline,
    schemaLoading,
    schemaError,
    triggerBlockedReason,
    openTrigger,
    labels.trigger,
  ])

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
      {mobileShell && sortedRuns.length > 0 && (
        <IosRunList
          runs={sortedRuns}
          isFlux={isFlux}
          noun={labels.run}
          label={formatCount(sortedRuns.length, labels.run)}
          onSelect={sortedRuns.some(isRunCancellable) ? () => setSelecting(true) : undefined}
          selecting={selecting}
          selectedNumbers={selectedRuns.map((run) => run.run_number)}
          isSelectable={isRunCancellable}
          onToggle={(run) =>
            selectRuns(
              selectedRuns.some((selectedRun) => selectedRun.run_number === run.run_number)
                ? selectedRuns.filter((selectedRun) => selectedRun.run_number !== run.run_number)
                : [...selectedRuns, run]
            )
          }
          onOpen={(run) => onViewRun(currentPipeline.id, run.run_number)}
          getSwipeActions={getRunSwipeActions}
          renderMenu={renderRunMenu}
        />
      )}
      {(mobileShell ? [] : sortedRuns).map((run) => {
        const selectable = isRunCancellable(run)
        const selected = selectedRuns.some((selectedRun) => selectedRun.run_number === run.run_number)

        return (
          <Card key={run.run_number} padding="lg" className="interactive-surface stretched">
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
                    className="item-primary"
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
              Loading more {labels.runs}…
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
      <>
        <StandardTable<PipelineRun>
          records={sortedRuns}
          onRowClick={({ record }) => onViewRun(currentPipeline.id, record.run_number)}
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
                  onViewRun(currentPipeline.id, record.run_number)
                }
              },
            }
          }}
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
      </>
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
        actions={mobileShell ? undefined : headerActions}
      />
      {isFlux && pipeline && mobileShell && (
        <IosGroup>
          <IosRow
            title={
              <Text component="span" fw={600} c={`var(--status-${STATUS_META[pipeline.status].tone})`}>
                {getStatusLabel(pipeline) ?? STATUS_META[pipeline.status].label}
              </Text>
            }
            subtitle={String(pipeline.metadata?.message ?? '') || undefined}
          />
        </IosGroup>
      )}
      {isFlux && pipeline && !mobileShell && (
        <Group gap="sm" align="flex-start" mb="sm">
          <StatusBadge status={pipeline.status} label={getStatusLabel(pipeline)} size="md" />
          <Text size="sm" style={{ overflowWrap: 'anywhere', flex: 1 }}>
            {String(pipeline.metadata?.message ?? '')}
          </Text>
        </Group>
      )}
      {pipeline && mobileShell && (
        <IosGroup footer={triggerBlockedReason}>
          <IosRow
            title={
              <Text component="span" inherit c={triggerBlockedReason ? 'dimmed' : 'blue'}>
                {labels.trigger}
              </Text>
            }
            disabled={!!triggerBlockedReason}
            onClick={openTrigger}
          />
        </IosGroup>
      )}
      {pipeline?.metadata?.history_scope === 'bounded_reconciliation' && (
        <Text
          size={mobileShell ? 'xs' : 'sm'}
          c="dimmed"
          mb={mobileShell ? undefined : 'sm'}
          px={mobileShell ? 'md' : undefined}
        >
          FluxCD retains a limited reconciliation history. Repeated reconciliations may be grouped, and older entries
          may be unavailable.
        </Text>
      )}

      <Tabs
        value={tab}
        onChange={(value) => onTabChange(value === 'metrics' ? 'metrics' : 'history')}
        style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}
      >
        <Tabs.List mb="sm">
          <Tabs.Tab value="history" leftSection={mobileShell ? undefined : <IconHistory size={16} />}>
            {labels.history}
          </Tabs.Tab>
          <Tabs.Tab value="metrics" leftSection={mobileShell ? undefined : <IconChartLine size={16} />}>
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
            <LoadingState message={`Loading ${labels.history.toLowerCase()}…`} />
          ) : (
            <>
              <Group
                justify="space-between"
                align="flex-start"
                wrap="nowrap"
                style={mobileShell ? { display: 'contents' } : undefined}
              >
                <FilterBar
                  onClearAll={clearFilters}
                  filters={{
                    search: {
                      value: search,
                      onChange: (value) => setFilter('search', value),
                      placeholder: `Search ${labels.runs}…`,
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
                {!mobileShell && selectedRuns.length > 0 && (
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
            <LoadingState message="Loading pipeline…" />
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

      {mobileShell && selecting && tab === 'history' && (
        <IosRunToolbar
          count={selectedRuns.length}
          cancelling={isCancellingSelected}
          onDone={() => {
            selectRuns([])
            setSelecting(false)
          }}
          onCancelRuns={cancelSelected}
        />
      )}

      <TableCustomizationModal
        opened={customizeModalOpened}
        onClose={closeCustomizeModal}
        columns={allColumns}
        visibleColumns={columns}
        currentOrder={columnPreferences.columnOrder}
        currentVisibility={columnPreferences.columnVisibility}
        onApply={handleApplyCustomization}
      />

      <RerunConfirmModal
        run={rerunTarget}
        pipelineName={pipeline?.name ?? ''}
        providerName={pipeline ? getPluginDisplayName(pipeline.provider_type) : ''}
        onClose={() => setRerunTarget(null)}
        onConfirm={confirmRerun}
      />
    </PageContainer>
  )
}
