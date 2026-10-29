import { ActionIcon, Button, Group, Menu, Paper, Stack, Text, Tooltip, UnstyledButton } from '@mantine/core'
import {
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconEye,
  IconEyeOff,
  IconFilterOff,
  IconFolder,
  IconFolderMinus,
  IconFolderOff,
  IconFolderPlus,
  IconFolders,
  IconGitBranch,
  IconListCheck,
  IconPin,
  IconPinFilled,
  IconPinnedOff,
  IconPlugConnected,
  IconRefresh,
} from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { Fragment, type ReactNode, useCallback, useId, useMemo } from 'react'
import { Link, useSearchParams } from 'react-router-dom'

import { useCancelActiveRuns } from '../../hooks/useCancelActiveRuns'
import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { usePinning } from '../../hooks/usePinning'
import { usePipelineExpansion } from '../../hooks/usePipelineExpansion'
import { usePipelineSelection } from '../../hooks/usePipelineSelection'
import { usePipelineFilters, usePipelineViewQuery } from '../../hooks/useUrlState'
import {
  usePipelineViewPage,
  usePipelineViews,
  useSetViewMembership,
  useToggleHidden,
  useTogglePin,
  useUpdatePipelineView,
} from '../../queries/usePipelineViewQueries'
import { useRefreshProviders } from '../../queries/useRefreshQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import { useProviderStore } from '../../stores/providerStore'
import { STATUS_META } from '../../theme/status'
import type { Pipeline, ProviderRecovery, ProviderSummary, SectionMode, ViewDisplay, ViewSortKey } from '../../types'
import { getErrorCode } from '../../types/errors'
import { getPipelineLabels, getStatusOptions } from '../../utils/pipelinePresentation'
import { fetchHint, providerAttention } from '../../utils/providerAttention'
import { TableCells } from '../../utils/tableCells'
import { typedKeys } from '../../utils/typedKeys'
import { canAddSubView } from '../../utils/viewTree'
import { useContextMenu } from '../common/ContextMenu'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'
import { BuildWatchSection } from '../pipeline/BuildWatchControls'
import { ProviderRecoveryActions } from '../provider/ProviderRecoveryActions'
import { ProviderStatusButton } from '../provider/ProviderStatusButton'

import { IosPipelineList } from './IosPipelineList'
import { IosToolbar } from './IosRunList'
import toolbarClasses from './IosRunList.module.css'
import { MobilePipelineCards } from './MobilePipelineCards'
import { PipelineCardGrid } from './PipelineCardGrid'
import {
  FetchInfo,
  type PipelineActions,
  PipelineStatusBadge,
  RepositoryFetchIndicator,
  RowFetchInfo,
  rowMenuItems,
  SectionActions,
  SectionHeading,
  type TableRow,
  WorkflowActions,
  workflowMenuItems,
} from './PipelineRowParts'
import { PipelineViewStrip } from './PipelineViewStrip'
import { PipelineViewTabs } from './PipelineViewTabs'
import { SubViewBar } from './SubViewBar'
import { ViewOptionsMenu } from './ViewOptionsMenu'

const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true })

const SORT_ACCESSORS: Record<ViewSortKey, string | undefined> = {
  manual: undefined,
  name: 'name',
  last_run: 'lastRun',
  status: 'status',
  organization: 'organization',
  provider: 'provider',
}

const accessorSort = (accessor: string): ViewSortKey | undefined =>
  typedKeys(SORT_ACCESSORS).find((key) => SORT_ACCESSORS[key] === accessor)

interface EmptyTableIssue {
  id: number
  name: string
  recovery: ProviderRecovery | null
  headline: string
  detail: string | null
  hint: string | null
}

const toEmptyTableIssue = (provider: ProviderSummary): EmptyTableIssue | null => {
  const attention = providerAttention(provider)
  const base = { id: provider.id, name: provider.name, recovery: attention.recovery }

  if (attention.headline) {
    return { ...base, headline: attention.headline, detail: attention.detail, hint: attention.hint }
  }

  if (provider.last_fetch_status === 'error' && provider.last_fetch_error) {
    return { ...base, headline: 'Fetch failed', detail: provider.last_fetch_error, hint: fetchHint(provider) }
  }

  return null
}

const getSectionsLabel = (sections: SectionMode, labels: ReturnType<typeof getPipelineLabels>, isFluxView: boolean) =>
  ({
    repository: isFluxView ? 'Sources' : 'Repositories',
    organization: labels.groupTitle,
    provider: 'Providers',
    status: 'Statuses',
    none: labels.items.charAt(0).toUpperCase() + labels.items.slice(1),
  })[sections]

interface UnifiedPipelinesViewProps {
  providers: ProviderSummary[]
  loading?: boolean
  error?: unknown
  onRetry: () => void
  onViewHistory: (pipeline: Pipeline) => void
  onViewRun: (pipelineId: string, runNumber: number) => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function UnifiedPipelinesView({
  providers,
  loading: loadingProp = false,
  error: errorProp,
  onRetry,
  onViewHistory,
  onViewRun,
  onTrigger,
  onViewMetrics,
}: UnifiedPipelinesViewProps): ReactNode {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const pinnedTitleId = useId()

  const { filters, setFilter, selectStatus, clearFilters } = usePipelineFilters()
  const {
    search,
    status: statusFilter,
    provider: providerFilter,
    organization: organizationFilter,
    dateRange,
    hidden,
  } = filters
  const showHidden = hidden === 'show'
  const query = usePipelineViewQuery()
  const {
    data: page,
    isLoading: pageLoading,
    error: pageError,
  } = usePipelineViewPage(query, providers.length > 0 && !loadingProp)
  const loading = loadingProp || pageLoading
  const error = errorProp ?? pageError
  const view = page?.view
  const kind = view?.kind ?? (query.view === 'all' ? 'all' : query.view === 'pinned' ? 'pinned' : 'custom')
  const display = view?.display
  const activeProviders = providerFilter
    ? providers.filter((provider) => String(provider.id) === providerFilter)
    : providers
  const isFluxView =
    activeProviders.length > 0 && activeProviders.every((provider) => provider.provider_type === 'fluxcd')
  const labels = getPipelineLabels(isFluxView ? 'fluxcd' : undefined)
  const title = kind === 'all' ? (isFluxView ? 'FluxCD resources' : 'Pipelines') : (view?.name ?? 'Pipelines')

  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const { mutate: togglePin } = useTogglePin()
  const { mutate: toggleHidden } = useToggleHidden()
  const { mutate: setMembership } = useSetViewMembership()
  const { refresh: refreshProviders, refreshing } = useRefreshProviders()
  const pinItem = usePinning()
  const { mutate: updateView } = useUpdatePipelineView()
  const contextMenu = useContextMenu()
  const { cancelItem, dialog: cancelDialog } = useCancelActiveRuns()

  const canRemove = view?.kind === 'custom' && view.capabilities.edit_members
  const { data: allViews = [] } = usePipelineViews()
  const [, setSearchParams] = useSearchParams()
  const openView = (id: string) =>
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev)

        next.set('view', id)

        return next
      },
      { replace: true }
    )
  const actions: PipelineActions = {
    onTogglePin: (p) => togglePin(p),
    onToggleHidden: (p) => toggleHidden(p),
    onAddToView: (p) => usePipelineViewStore.getState().openAddToView([p]),
    onRemoveFromView:
      view && canRemove ? (p) => setMembership({ id: view.id, pipelineIds: [p.id], member: false }) : undefined,
    removeLabel: view && canRemove ? `Remove from ${view.name}` : undefined,
    onTrigger,
    onViewMetrics,
    onViewHistory,
    onOpenView: openView,
    onRefreshProviders: (ids) => void refreshProviders(ids),
    isRefreshing: (ids) => ids.some((id) => refreshing.has(id)),
    pinItem,
    cancelItem,
  }
  const rowMenu = (row: TableRow) => {
    const { pipeline } = row

    return contextMenu(() => (pipeline ? workflowMenuItems(pipeline, actions) : rowMenuItems(row, actions)))
  }
  const changeDisplay = (next: ViewDisplay) => {
    if (view) {
      updateView({ id: view.id, patch: { display: next } })
    }
  }

  const filtersActive = page?.filters_active ?? false
  const { isOpen, toggle: toggleOpen, setAll } = usePipelineExpansion(query.view, JSON.stringify(query), filtersActive)
  const getProvider = useCallback(
    (providerId: number): ProviderSummary | undefined => {
      return providers.find((p) => p.id === providerId)
    },
    [providers]
  )

  const rows = useMemo(() => {
    const result: TableRow[] = []
    const providerLabel = (ids: number[]) =>
      ids
        .map((id) => getProvider(id)?.name ?? 'Unknown')
        .sort(collator.compare)
        .join(', ')

    page?.sections.forEach((section) => {
      const flat = section.kind === 'none'
      const expanded = isOpen(section.id, false)
      const depth = flat ? 0 : 1

      if (!flat) {
        result.push({
          id: section.id,
          type: 'section',
          kind: section.kind,
          name: section.kind === 'status' && section.status ? STATUS_META[section.status].label : section.title,
          organization: section.organization ?? undefined,
          pipelineCount: section.pipelines.length,
          providerIds: new Set(section.provider_ids),
          providerLabel: providerLabel(section.provider_ids),
          hasPendingFetch: section.pending_fetch,
          lastRun: section.last_run,
          expanded,
          pipelines: section.pipelines,
        })
      }

      if (!flat && !expanded) {
        return
      }

      section.pipelines.forEach((pipeline) => {
        result.push({
          id: `workflow-${section.id}-${pipeline.id}`,
          type: 'workflow',
          name: pipeline.name,
          depth,
          organization: pipeline.organization,
          providerIds: new Set([pipeline.provider_id]),
          providerLabel: getProvider(pipeline.provider_id)?.name ?? 'Unknown',
          status: pipeline.status,
          lastRun: pipeline.last_run,
          pipeline,
        })
      })
    })

    return result
  }, [page, isOpen, getProvider])

  const toggleSection = useCallback((rowId: string) => toggleOpen(rowId, false), [toggleOpen])
  const collapsibleIds = page?.sections.filter((section) => section.kind !== 'none').map((section) => section.id) ?? []

  const {
    selecting,
    selectedPipelines,
    toggle: toggleSelected,
    select,
    start: startSelection,
    finish: finishSelection,
  } = usePipelineSelection(query.view, page)
  const selectedSet = new Set(selectedPipelines.map((p) => p.id))
  const pipelineIds = (records: TableRow[]) => records.flatMap((row) => (row.pipeline ? [row.pipeline.id] : []))
  const selectRows = (records: TableRow[]) => {
    const kept = new Set(records)
    const dropped = new Set(
      pipelineIds(rows.filter((row) => row.pipeline && selectedSet.has(row.pipeline.id) && !kept.has(row)))
    )

    select(
      pipelineIds(rows),
      pipelineIds(records).filter((id) => !dropped.has(id))
    )
  }

  const providerOptions = useMemo(
    () =>
      (page?.facets.provider_ids ?? [])
        .map((id) => getProvider(id))
        .filter((provider): provider is ProviderSummary => provider !== undefined)
        .map((provider) => ({ value: String(provider.id), label: provider.name }))
        .sort((a, b) => collator.compare(a.label, b.label)),
    [page, getProvider]
  )

  const sortStatus: DataTableSortStatus<TableRow> = {
    columnAccessor: (display && SORT_ACCESSORS[display.sort]) ?? 'manual',
    direction: display?.direction ?? 'asc',
  }
  const onSortStatusChange = ({ columnAccessor, direction }: DataTableSortStatus<TableRow>) => {
    const sort = accessorSort(String(columnAccessor))

    if (display && sort) {
      changeDisplay({ ...display, sort, direction })
    }
  }
  const sortable = view?.capabilities.edit_display ?? false
  const hiddenOption =
    page && kind !== 'pinned' && (page.hidden_count > 0 || showHidden)
      ? {
          value: showHidden,
          count: page.hidden_count,
          onChange: (value: boolean) => setFilter('hidden', value ? 'show' : null),
        }
      : undefined
  const viewOptions = view ? (
    <ViewOptionsMenu
      view={view}
      groupTitle={labels.groupTitle}
      onChange={changeDisplay}
      onExpandAll={collapsibleIds.length > 0 ? (open) => setAll(collapsibleIds, open) : undefined}
      hidden={hiddenOption}
    />
  ) : null

  const emptyTableIssues = providers
    .filter((provider) => query.provider_id === undefined || provider.id === query.provider_id)
    .map(toEmptyTableIssue)
    .filter((issue): issue is EmptyTableIssue => issue !== null)
  const firstIssue = emptyTableIssues[0]

  const selectButton = selecting ? (
    <Button size="sm" miw={100} variant="filled" leftSection={<IconCheck size={16} />} onClick={finishSelection}>
      Done
    </Button>
  ) : (
    page &&
    (page.total + page.pinned.length > 0 || page.pinned_blocks.some((block) => block.pipelines.length > 0)) && (
      <Button size="sm" miw={100} variant="default" leftSection={<IconListCheck size={16} />} onClick={startSelection}>
        Select
      </Button>
    )
  )
  const pageHeading =
    mobileShell && !isFluxView ? null : (
      <PageHeader
        title={title}
        actions={
          mobileShell ? undefined : (
            <Group gap="xs" wrap="nowrap">
              {selectButton}
              {viewOptions}
            </Group>
          )
        }
      />
    )
  const viewNav = (
    <>
      {mobileShell ? query.view !== 'pinned' && <PipelineViewStrip /> : <PipelineViewTabs />}
      <SubViewBar />
    </>
  )
  const watchSection =
    query.view === 'all' && providerFilter === null ? <BuildWatchSection onViewRun={onViewRun} /> : null

  if (getErrorCode(error) === 'NOT_FOUND') {
    return (
      <PageContainer>
        {viewNav}
        <EmptyState
          icon={<IconFolderOff size={24} />}
          title="View not found"
          description="It may have been deleted or renamed in config.toml."
          action={
            <Button component={Link} to="/pipelines" variant="default" size="sm">
              Show all pipelines
            </Button>
          }
        />
      </PageContainer>
    )
  }

  if (error) {
    return (
      <PageContainer>
        {pageHeading}
        {watchSection}
        <ErrorState title="Couldn't load pipelines" error={error} onRetry={onRetry} retrying={loading} />
      </PageContainer>
    )
  }

  if (!loading && providers.length === 0) {
    return (
      <PageContainer>
        {pageHeading}
        {watchSection}
        <EmptyState
          icon={<IconPlugConnected size={24} />}
          title="No providers configured"
          description="Add a CI/CD provider to start monitoring your pipelines and workflows."
          action={
            <Button size="sm" onClick={openAddProviderModal}>
              Add provider
            </Button>
          }
        />
      </PageContainer>
    )
  }

  if (loading || !page || !view || !display) {
    return (
      <PageContainer>
        {pageHeading}
        {watchSection}
        <LoadingState variant="page" message="Loading pipelines…" />
      </PageContainer>
    )
  }

  const pinnedPipelines = page.pinned
  const showPinnedSection = pinnedPipelines.length > 0
  const pinnedBlocks = page.pinned_blocks.filter((block) => block.kind === 'repository' && block.pipelines.length > 0)
  const hasPinnedArea = showPinnedSection || pinnedBlocks.length > 0
  const sectionsLabel = getSectionsLabel(display.sections, labels, isFluxView)
  const ownSections = page.sections.filter((section) => section.kind !== 'view')
  const sectionCount =
    display.sections === 'none'
      ? ownSections.reduce((sum, section) => sum + section.pipelines.length, 0)
      : ownSections.length

  const selection = selecting ? { selected: selectedSet, onToggle: toggleSelected } : undefined

  const pinnedGrid = (header?: string) => (
    <PipelineCardGrid
      header={header}
      pipelines={pinnedPipelines}
      getProvider={getProvider}
      actions={actions}
      selection={selection}
    />
  )

  const allPinned = selectedPipelines.length > 0 && selectedPipelines.every((p) => p.pinned)
  const cancelSelected = cancelItem(
    selectedPipelines.length === 1 ? '1 selected pipeline' : `${selectedPipelines.length} selected pipelines`,
    selectedPipelines
  )
  const allHidden = selectedPipelines.length > 0 && selectedPipelines.every((p) => p.hidden)
  const selectedIds = selectedPipelines.map((p) => p.id)

  const selectionActions = [
    {
      label: allPinned ? 'Unpin' : 'Pin',
      icon: allPinned ? <IconPinnedOff size={14} /> : <IconPin size={14} />,
      onClick: () => {
        for (const p of selectedPipelines.filter((p) => p.pinned === allPinned)) {
          togglePin(p)
        }
      },
    },
    {
      label: allHidden ? 'Unhide' : 'Hide',
      icon: allHidden ? <IconEye size={14} /> : <IconEyeOff size={14} />,
      onClick: () => {
        for (const p of selectedPipelines.filter((p) => p.hidden === allHidden)) {
          toggleHidden(p)
        }
        finishSelection()
      },
    },
    {
      label: 'Refresh',
      icon: <IconRefresh size={14} />,
      onClick: () => void refreshProviders([...new Set(selectedPipelines.map((p) => p.provider_id))]),
    },
    ...(cancelSelected ? [cancelSelected] : []),
    {
      label: 'New view',
      icon: <IconFolderPlus size={14} />,
      onClick: () =>
        usePipelineViewStore.getState().openEditor(null, selectedIds, canAddSubView(allViews, view) ? view.id : null),
    },
    {
      label: 'Add to view…',
      icon: <IconFolder size={14} />,
      onClick: () => usePipelineViewStore.getState().openAddToView(selectedPipelines),
    },
    ...(canRemove
      ? [
          {
            label: `Remove from ${view.name}`,
            icon: <IconFolderMinus size={14} />,
            color: 'red',
            onClick: () => {
              setMembership({ id: view.id, pipelineIds: selectedIds, member: false })
              finishSelection()
            },
          },
        ]
      : []),
  ]

  const selectionBar = !mobileShell && selecting && (
    <Paper withBorder radius="md" px="sm" py="xs">
      <Group justify="space-between" gap="sm" wrap="nowrap" align="flex-start">
        <Text size="sm" fw={600} py={4} style={{ whiteSpace: 'nowrap' }} role="status">
          {selectedPipelines.length === 0 ? `Select ${labels.items}` : `${selectedPipelines.length} selected`}
        </Text>
        <Group gap="xs" justify="flex-end">
          {selectionActions.map((action) => (
            <Button
              key={action.label}
              size="xs"
              variant="light"
              {...(action.color === undefined ? {} : { color: action.color })}
              leftSection={action.icon}
              disabled={selectedPipelines.length === 0}
              onClick={action.onClick}
            >
              {action.label}
            </Button>
          ))}
        </Group>
      </Group>
    </Paper>
  )

  const iosToolbar = mobileShell && selecting && (
    <IosToolbar label="Selected pipelines">
      <Button variant="subtle" onClick={finishSelection}>
        Done
      </Button>
      <span className={toolbarClasses.count} role="status">
        {selectedPipelines.length === 0 ? `Select ${labels.items}` : `${selectedPipelines.length} selected`}
      </span>
      <Menu position="top">
        <Menu.Target>
          <Button variant="subtle" disabled={selectedPipelines.length === 0}>
            Actions
          </Button>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Label>{`${selectedPipelines.length} selected`}</Menu.Label>
          {selectionActions.map((action) => (
            <Menu.Item
              key={action.label}
              {...(action.color === undefined ? {} : { color: action.color })}
              onClick={action.onClick}
            >
              {action.label}
            </Menu.Item>
          ))}
          <ActionSheetCancel />
        </Menu.Dropdown>
      </Menu>
    </IosToolbar>
  )

  const noMatches = (
    <EmptyState
      icon={<IconFilterOff size={24} />}
      title={isFluxView ? 'No matching resources' : 'No matching workflows'}
      description={
        isFluxView
          ? 'No resources match the current filters.'
          : kind === 'pinned'
            ? 'No pinned workflows match the current filters.'
            : 'No repositories or workflows match the current filters.'
      }
      action={
        <Button variant="default" size="sm" onClick={clearFilters}>
          Clear filters
        </Button>
      }
    />
  )

  const emptyState = filtersActive ? (
    noMatches
  ) : kind === 'all' && firstIssue ? (
    <ErrorState
      title={`Couldn't fetch ${labels.items} from ${emptyTableIssues.map((issue) => issue.name).join(', ')}`}
      error={[firstIssue.headline, firstIssue.detail].filter(Boolean).join(': ')}
      hint={firstIssue.hint}
      actions={<ProviderRecoveryActions providerId={firstIssue.id} recovery={firstIssue.recovery} />}
    />
  ) : page.hidden_count > 0 && !showHidden ? (
    <EmptyState
      icon={<IconEyeOff size={24} />}
      title={isFluxView ? 'All resources are hidden' : 'All workflows are hidden'}
      description="Turn on Show hidden pipelines in View options to list them and unhide them."
      action={
        <Button variant="default" size="sm" onClick={() => setFilter('hidden', 'show')}>
          Show hidden
        </Button>
      }
    />
  ) : kind === 'all' ? (
    <EmptyState
      icon={<IconFolder size={24} />}
      title={isFluxView ? 'No FluxCD resources found' : 'No workflows yet'}
      description={
        isFluxView
          ? 'Check the configured namespaces and resource selection in Settings → Providers.'
          : 'Workflows appear here after they run on your configured providers.'
      }
      action={
        !mobileShell &&
        (onRetry ? (
          <Button variant="default" size="sm" onClick={onRetry}>
            Refresh
          </Button>
        ) : (
          <Button component={Link} to="/settings/providers" variant="default" size="sm">
            Provider settings
          </Button>
        ))
      }
    />
  ) : kind === 'pinned' ? (
    <EmptyState
      icon={<IconPin size={24} />}
      title={isFluxView ? 'No pinned resources' : 'No pinned workflows'}
      description={
        mobileShell
          ? `Swipe a ${labels.item} left or open its Actions menu and choose Pin to keep it here.`
          : `Use the pin icon on a ${labels.item} to keep it here.`
      }
      action={
        <Button component={Link} to="/pipelines" variant="default" size="sm">
          Browse {labels.items}
        </Button>
      }
    />
  ) : (
    <EmptyState
      icon={<IconFolder size={24} />}
      title="No pipelines in this view"
      description={
        view.capabilities.edit_rules
          ? "Use Add to view on any pipeline, or edit the view's rules."
          : "No pipelines match this view's rules in config.toml."
      }
      action={
        view.capabilities.edit_rules ? (
          <Button variant="default" size="sm" onClick={() => usePipelineViewStore.getState().openEditor(view.id)}>
            Edit view
          </Button>
        ) : undefined
      }
    />
  )

  return (
    <PageContainer>
      {pageHeading}
      {viewNav}

      <FilterBar
        filters={{
          search: {
            value: search,
            onChange: (value) => setFilter('search', value),
            placeholder: isFluxView ? 'Search resources, namespaces, sources…' : 'Search repositories, workflows…',
          },
          status: {
            value: statusFilter,
            onSelect: selectStatus,
            onClear: () => setFilter('status', []),
            options: getStatusOptions(isFluxView ? 'fluxcd' : undefined),
          },
          provider: {
            value: providerFilter,
            onChange: (value) => setFilter('provider', value),
            options: providerOptions,
          },
          organization: {
            value: organizationFilter,
            onChange: (value) => setFilter('organization', value),
            options: page.facets.organizations,
            label: labels.groupTitle,
            allLabel: `All ${labels.groups}`,
          },
          dateRange: {
            value: dateRange,
            onChange: (value) => setFilter('dateRange', value),
          },
        }}
        onClearAll={clearFilters}
      />

      {watchSection}

      {selectionBar}
      {iosToolbar}
      {cancelDialog}

      {showPinnedSection &&
        (mobileShell ? (
          pinnedGrid('Pinned')
        ) : (
          <Stack gap="sm" component="section" aria-labelledby={pinnedTitleId}>
            <SectionHeading
              id={pinnedTitleId}
              icon={<IconPinFilled size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
              label="Pinned"
              count={pinnedPipelines.length}
              expanded={isOpen('__pinned', true)}
              onToggle={() => toggleOpen('__pinned', true)}
            />
            {isOpen('__pinned', true) && pinnedGrid()}
          </Stack>
        ))}

      {pinnedBlocks.map((block) => {
        const blockId = `pin:${block.kind}:${block.ref}`
        const unpin = () => actions.pinItem(block.kind, block.ref).onClick()
        const grid = (header?: string, headerAction?: ReactNode) => (
          <PipelineCardGrid
            header={header}
            headerAction={headerAction}
            pipelines={block.pipelines}
            getProvider={getProvider}
            actions={actions}
            selection={selection}
          />
        )

        return mobileShell ? (
          <Fragment key={blockId}>
            {grid(
              block.name,
              <button type="button" className={toolbarClasses.headerAction} onClick={unpin}>
                Unpin
              </button>
            )}
          </Fragment>
        ) : (
          <Stack key={blockId} gap="sm" component="section">
            <SectionHeading
              icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
              label={block.name}
              count={block.pipelines.length}
              expanded={isOpen(blockId, true)}
              onToggle={() => toggleOpen(blockId, true)}
              actions={
                <Tooltip label={`Unpin ${block.name}`}>
                  <ActionIcon aria-label={`Unpin ${block.name}`} onClick={unpin}>
                    <IconPinnedOff size={16} />
                  </ActionIcon>
                </Tooltip>
              }
            />
            {isOpen(blockId, true) && grid()}
          </Stack>
        )
      })}

      <Stack gap="sm" flex={1} mih={0}>
        {!mobileShell && hasPinnedArea && rows.length > 0 && (
          <SectionHeading
            icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
            label={sectionsLabel}
            count={sectionCount}
            expanded={isOpen('__sections', true)}
            onToggle={() => toggleOpen('__sections', true)}
          />
        )}

        {rows.length === 0 ? (
          emptyState
        ) : !mobileShell && hasPinnedArea && !isOpen('__sections', true) ? null : mobileShell ? (
          <IosPipelineList
            header={sectionsLabel}
            headerAction={viewOptions}
            rows={rows}
            getProvider={getProvider}
            onToggleRepo={toggleSection}
            actions={actions}
            selection={selection}
            onSelect={selecting ? undefined : startSelection}
          />
        ) : isMobile ? (
          <MobilePipelineCards
            rows={rows}
            getProvider={getProvider}
            onToggleRepo={toggleSection}
            actions={actions}
            selection={selection}
          />
        ) : display.layout === 'cards' ? (
          <Stack gap="lg">
            {page.sections.map((section) => {
              const open = section.kind === 'none' || isOpen(section.id, true)
              const sectionName =
                section.kind === 'status' && section.status ? STATUS_META[section.status].label : section.title

              return (
                <Stack key={section.id} gap="sm">
                  {section.kind !== 'none' && (
                    <SectionHeading
                      icon={
                        section.kind === 'view' ? (
                          <IconFolders size={16} color="var(--mantine-color-dimmed)" aria-hidden />
                        ) : (
                          <IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />
                        )
                      }
                      label={sectionName}
                      info={<FetchInfo providers={section.provider_ids.flatMap((id) => getProvider(id) ?? [])} />}
                      actions={
                        <SectionActions
                          name={sectionName}
                          providerIds={section.provider_ids}
                          items={rowMenuItems(
                            {
                              id: section.id,
                              type: 'section',
                              name: sectionName,
                              kind: section.kind,
                              pipelines: section.pipelines,
                            },
                            actions
                          )}
                          actions={actions}
                        />
                      }
                      count={section.pipelines.length}
                      expanded={open}
                      onToggle={() => toggleOpen(section.id, true)}
                    />
                  )}
                  {open && (
                    <PipelineCardGrid
                      pipelines={section.pipelines}
                      getProvider={getProvider}
                      actions={actions}
                      selection={selection}
                    />
                  )}
                </Stack>
              )
            })}
          </Stack>
        ) : (
          <StandardTable<TableRow>
            records={rows}
            customRowAttributes={rowMenu}
            onRowClick={({ record }) => {
              if (record.pipeline) {
                onViewHistory(record.pipeline)
              } else {
                toggleSection(record.id)
              }
            }}
            columns={[
              {
                accessor: 'name',
                title: 'Name',
                sortable,
                width: '35%',
                textAlign: 'left' as const,
                render: (row) => {
                  const indent = row.depth ? `calc(var(--mantine-spacing-xl) * ${row.depth})` : undefined

                  if (row.type !== 'workflow') {
                    return (
                      <UnstyledButton
                        className="item-primary"
                        w="100%"
                        {...(indent === undefined ? {} : { pl: indent })}
                        onClick={() => toggleSection(row.id)}
                        {...(row.expanded === undefined ? {} : { 'aria-expanded': row.expanded })}
                      >
                        <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                          {row.expanded ? (
                            <IconChevronDown size={16} style={{ flexShrink: 0 }} />
                          ) : (
                            <IconChevronRight size={16} style={{ flexShrink: 0 }} />
                          )}
                          {row.kind === 'view' ? (
                            <IconFolders size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                          ) : (
                            <IconFolder size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                          )}
                          {TableCells.truncated(row.name)}
                          {row.kind === 'view' && (
                            <Text size="sm" c="dimmed">
                              {row.pipelineCount}
                            </Text>
                          )}
                        </Group>
                      </UnstyledButton>
                    )
                  }

                  const { pipeline } = row

                  if (!pipeline) {
                    return null
                  }

                  return (
                    <UnstyledButton
                      className="item-primary"
                      w="100%"
                      {...(indent === undefined ? {} : { pl: indent })}
                      onClick={() => onViewHistory(pipeline)}
                      aria-label={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()} for ${row.name}`}
                    >
                      <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                        <IconGitBranch size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                        {TableCells.truncated(row.name)}
                      </Group>
                    </UnstyledButton>
                  )
                },
              },
              {
                accessor: 'organization',
                title: labels.groupTitle,
                sortable,
                width: 160,
                textAlign: 'left' as const,
                render: (row) => TableCells.truncatedDimmed(row.organization),
              },
              {
                accessor: 'status',
                title: 'Status',
                sortable,
                width: 120,
                textAlign: 'center' as const,
                render: (row) =>
                  row.type === 'workflow' && row.pipeline ? (
                    <PipelineStatusBadge pipeline={row.pipeline} provider={getProvider(row.pipeline.provider_id)} />
                  ) : (
                    <RowFetchInfo row={row} getProvider={getProvider} />
                  ),
              },
              {
                accessor: 'provider',
                title: 'Provider',
                sortable,
                width: 200,
                textAlign: 'left' as const,
                render: (row) => (
                  <Stack gap="3xs" align="flex-start">
                    {Array.from(row.providerIds ?? []).map((providerId) => (
                      <ProviderStatusButton key={providerId} provider={getProvider(providerId)} />
                    ))}
                  </Stack>
                ),
              },
              {
                accessor: 'actions',
                title: 'Actions',
                width: onViewMetrics ? 220 : 200,
                textAlign: 'right' as const,
                render: (row) => {
                  if (row.type === 'section') {
                    return (
                      <Group justify="flex-end" wrap="nowrap">
                        <RepositoryFetchIndicator row={row} />
                        <SectionActions
                          name={row.name}
                          providerIds={Array.from(row.providerIds ?? [])}
                          items={rowMenuItems(row, actions)}
                          actions={actions}
                        />
                      </Group>
                    )
                  }

                  const { pipeline } = row

                  if (pipeline) {
                    return (
                      <Group justify="flex-end">
                        <WorkflowActions pipeline={pipeline} actions={actions} />
                      </Group>
                    )
                  }

                  return null
                },
              },
            ]}
            {...(selecting && {
              selectedRecords: rows.filter((row) => row.pipeline && selectedSet.has(row.pipeline.id)),
              onSelectedRecordsChange: selectRows,
              isRecordSelectable: (row: TableRow) => row.type === 'workflow',
              getRecordSelectionCheckboxProps: (row: TableRow) =>
                row.type === 'workflow' ? {} : { style: { visibility: 'hidden' as const } },
            })}
            sortStatus={sortStatus}
            onSortStatusChange={onSortStatusChange}
          />
        )}
      </Stack>
    </PageContainer>
  )
}
