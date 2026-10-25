import { Button, Group, Stack, UnstyledButton } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconChevronDown,
  IconChevronRight,
  IconFilterOff,
  IconFolder,
  IconGitBranch,
  IconPin,
  IconPinFilled,
  IconPlugConnected,
} from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { useCallback, useMemo, useRef, useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { usePipelineFilters } from '../../hooks/useUrlState'
import { logger } from '../../lib/logger'
import { pinKey, usePinnedStore } from '../../stores/pinnedStore'
import { useProviderStore } from '../../stores/providerStore'
import { mostSevereStatus } from '../../theme/status'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'
import { getDateRangeStart } from '../../utils/dateRange'
import { getPipelineLabels, getStatusLabel, getStatusOptions } from '../../utils/pipelinePresentation'
import { fetchHint, providerAttention } from '../../utils/providerAttention'
import { TableCells } from '../../utils/tableCells'
import { useContextMenu } from '../common/ContextMenu'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { PageHeader } from '../common/PageHeader'
import { StandardTable } from '../common/StandardTable'
import { ProviderRecoveryActions } from '../provider/ProviderRecoveryActions'
import { ProviderStatusButton } from '../provider/ProviderStatusButton'

import { IosPipelineList } from './IosPipelineList'
import { MobilePipelineCards } from './MobilePipelineCards'
import { PinnedWorkflows } from './PinnedWorkflows'
import {
  RepositoryFetchIndicator,
  RowStatus,
  SectionHeading,
  type TableRow,
  WorkflowActions,
  workflowMenuItems,
} from './PipelineRowParts'

const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true })

interface RepoGroup {
  fullName: string
  repository: string
  organization: string
  pipelines: Pipeline[]
  providerIds: Set<number>
}

interface RowFilters {
  query: string
  organization: string | null
  providerId?: number
  status: string | null
  cutoff: Date | null
}

interface Section {
  row: TableRow
  workflows: TableRow[]
}

const compareStatus = (a: PipelineStatus | null | undefined, b: PipelineStatus | null | undefined) => {
  if (a === b) {
    return 0
  }

  if (!a) {
    return 1
  }

  if (!b) {
    return -1
  }

  return mostSevereStatus([a, b]) === a ? -1 : 1
}

const lastRunAt = (row: TableRow) => (row.lastRun ? Date.parse(row.lastRun) : null)

const compareRows = (a: TableRow, b: TableRow, sort: DataTableSortStatus<TableRow>) => {
  let primary = 0

  if (sort.columnAccessor === 'lastRun') {
    const left = lastRunAt(a)
    const right = lastRunAt(b)

    if (left === null || right === null) {
      return left === right ? collator.compare(a.name, b.name) : left === null ? 1 : -1
    }

    primary = left - right
  } else if (sort.columnAccessor === 'status') {
    primary = compareStatus(a.status, b.status)
  } else if (sort.columnAccessor === 'organization') {
    primary = collator.compare(a.organization ?? '', b.organization ?? '')
  } else if (sort.columnAccessor === 'provider') {
    primary = collator.compare(a.providerLabel ?? '', b.providerLabel ?? '')
  } else {
    primary = collator.compare(a.name, b.name)
  }

  if (primary !== 0) {
    return sort.direction === 'asc' ? primary : -primary
  }

  return collator.compare(a.name, b.name)
}

const pipelineMatches = (pipeline: Pipeline, repository: string, organization: string, filters: RowFilters) => {
  if (filters.organization && organization !== filters.organization) {
    return false
  }

  if (filters.providerId !== undefined && pipeline.provider_id !== filters.providerId) {
    return false
  }

  if (filters.status && pipeline.status !== filters.status) {
    return false
  }

  if (filters.cutoff && !(pipeline.last_run && new Date(pipeline.last_run) >= filters.cutoff)) {
    return false
  }

  return (
    !filters.query ||
    [pipeline.name, pipeline.branch, repository, organization].some((value) =>
      value?.toLowerCase().includes(filters.query)
    )
  )
}

interface EmptyTableIssue {
  id: number
  name: string
  kind: ProviderSummary['last_fetch_error_kind']
  headline: string
  detail: string | null
  hint: string | null
}

const toEmptyTableIssue = (provider: ProviderSummary): EmptyTableIssue | null => {
  const attention = providerAttention(provider)
  const base = { id: provider.id, name: provider.name, kind: attention.kind }

  if (attention.headline) {
    return { ...base, headline: attention.headline, detail: attention.detail, hint: attention.hint }
  }

  if (provider.last_fetch_status === 'error' && provider.last_fetch_error) {
    return { ...base, headline: 'Fetch failed', detail: provider.last_fetch_error, hint: fetchHint(provider) }
  }

  return null
}

interface UnifiedPipelinesViewProps {
  pipelines: Pipeline[]
  providers: ProviderSummary[]
  loading?: boolean
  error?: unknown
  onRetry: () => void
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export const UnifiedPipelinesView = ({
  pipelines,
  providers,
  loading = false,
  error,
  onRetry,
  onViewHistory,
  onTrigger,
  onViewMetrics,
}: UnifiedPipelinesViewProps) => {
  const orphanedPipelines = useRef<Set<string>>(new Set())

  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()

  const { filters, setFilter, clearFilters } = usePipelineFilters()
  const {
    search,
    status: statusFilter,
    provider: providerFilter,
    organization: organizationFilter,
    dateRange,
  } = filters
  const [searchParams] = useSearchParams()
  const pinnedView = searchParams.get('view') === 'pinned'
  const activeProviders = providerFilter
    ? providers.filter((provider) => String(provider.id) === providerFilter)
    : providers
  const isFluxView =
    activeProviders.length > 0 && activeProviders.every((provider) => provider.provider_type === 'fluxcd')
  const labels = getPipelineLabels(isFluxView ? 'fluxcd' : undefined)
  const title = pinnedView ? 'Pinned' : isFluxView ? 'FluxCD resources' : 'Pipelines'

  const pinnedKeys = usePinnedStore((s) => s.pinnedKeys)
  const togglePin = usePinnedStore((s) => s.togglePin)
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const contextMenu = useContextMenu()
  const rowMenu = ({ pipeline, pinKey }: TableRow) => {
    if (!pipeline || !pinKey) {
      return {}
    }

    return contextMenu(() =>
      workflowMenuItems({
        pipeline,
        isPinned: pinnedKeys.includes(pinKey),
        onTogglePin: () => togglePin(pinKey),
        onTrigger,
        onViewMetrics,
        onViewHistory,
      })
    )
  }

  const [expansionOverrides, setExpansionOverrides] = useState<Record<string, boolean>>({})
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<TableRow>>({
    columnAccessor: 'name',
    direction: 'asc',
  })
  const getProvider = useCallback(
    (providerId: number): ProviderSummary | undefined => {
      return providers.find((p) => p.id === providerId)
    },
    [providers]
  )

  const parseRepositoryName = useCallback(
    (fullName: string, providerId: number): { organization: string; repository: string } => {
      const parts = fullName.split('/')

      if (parts.length >= 2) {
        const org = parts[0]

        if (org === '(root)' || org === 'Unknown' || !org) {
          const provider = getProvider(providerId)

          return {
            organization: provider?.name || 'Unknown',
            repository: parts.slice(1).join('/'),
          }
        }

        return {
          organization: org,
          repository: parts.slice(1).join('/'),
        }
      }
      const provider = getProvider(providerId)

      return {
        organization: provider?.name || 'Unknown',
        repository: fullName,
      }
    },
    [getProvider]
  )

  const { repoGroups, validPipelines, pinKeys } = useMemo(() => {
    const valid = new Map<string, Pipeline>()
    const keysById = new Map<string, string>()
    const repoMap = new Map<string, { pipelines: Pipeline[]; providerIds: Set<number> }>()

    pipelines.forEach((pipeline) => {
      if (!pipeline.repository) {
        return
      }

      if (pipeline.provider_id === 0 || pipeline.provider_id === null || pipeline.provider_id === undefined) {
        if (!orphanedPipelines.current.has(pipeline.id)) {
          orphanedPipelines.current.add(pipeline.id)

          logger.error('UnifiedPipelinesView', 'Orphaned pipeline detected', {
            pipeline_id: pipeline.id,
            pipeline_name: pipeline.name,
            provider_id: pipeline.provider_id,
          })

          if (import.meta.env.DEV) {
            notifications.show({
              title: 'Data inconsistency detected',
              message: `Pipeline "${pipeline.name}" has invalid provider reference`,
              color: 'red',
              autoClose: false,
            })
          }
        }

        return
      }

      const provider = getProvider(pipeline.provider_id)

      if (!provider) {
        if (!orphanedPipelines.current.has(pipeline.id)) {
          orphanedPipelines.current.add(pipeline.id)

          logger.warn('UnifiedPipelinesView', 'Pipeline references non-existent provider', {
            pipeline_id: pipeline.id,
            pipeline_name: pipeline.name,
            provider_id: pipeline.provider_id,
          })
        }

        return
      }

      orphanedPipelines.current.delete(pipeline.id)

      const key = pinKey(pipeline, provider.name)

      valid.set(key, pipeline)
      keysById.set(pipeline.id, key)

      const existing = repoMap.get(pipeline.repository)

      if (existing) {
        existing.pipelines.push(pipeline)
        existing.providerIds.add(pipeline.provider_id)
      } else {
        repoMap.set(pipeline.repository, {
          pipelines: [pipeline],
          providerIds: new Set([pipeline.provider_id]),
        })
      }
    })

    providers.forEach((provider) => {
      provider.configured_repositories?.forEach((repoName) => {
        const existing = repoMap.get(repoName)

        if (existing) {
          existing.providerIds.add(provider.id)
        } else {
          repoMap.set(repoName, { pipelines: [], providerIds: new Set([provider.id]) })
        }
      })
    })

    const groups: RepoGroup[] = Array.from(repoMap.entries()).map(([fullName, data]) => {
      const providerId = data.pipelines[0]?.provider_id || Array.from(data.providerIds)[0] || 0

      return { fullName, ...parseRepositoryName(fullName, providerId), ...data }
    })

    return { repoGroups: groups, validPipelines: valid, pinKeys: keysById }
  }, [pipelines, providers, getProvider, parseRepositoryName])

  const rowFilters = useMemo<RowFilters>(
    () => ({
      query: search.toLowerCase(),
      organization: organizationFilter,
      providerId: providers.find((p) => String(p.id) === providerFilter)?.id,
      status: statusFilter,
      cutoff: getDateRangeStart(dateRange),
    }),
    [search, organizationFilter, providerFilter, statusFilter, dateRange, providers]
  )

  const filtersActive = Boolean(
    rowFilters.query ||
      rowFilters.organization ||
      rowFilters.providerId !== undefined ||
      rowFilters.status ||
      rowFilters.cutoff
  )

  const { pinnedPipelines, sections } = useMemo(() => {
    const buildWorkflowRow = (pipeline: Pipeline): TableRow => ({
      id: `workflow-${pipeline.id}`,
      type: 'workflow',
      name: pipeline.name,
      pinKey: pinKeys.get(pipeline.id),
      organization: parseRepositoryName(pipeline.repository, pipeline.provider_id).organization,
      providerIds: new Set([pipeline.provider_id]),
      providerLabel: getProvider(pipeline.provider_id)?.name ?? 'Unknown',
      status: pipeline.status,
      lastRun: pipeline.last_run,
      pipeline,
    })

    const matches = (pipeline: Pipeline) => {
      const { repository, organization } = parseRepositoryName(pipeline.repository, pipeline.provider_id)

      return pipelineMatches(pipeline, repository, organization, rowFilters)
    }

    const pinned = pinnedKeys
      .map((key) => validPipelines.get(key))
      .filter((pipeline): pipeline is Pipeline => !!pipeline && matches(pipeline))

    const built = repoGroups.flatMap((group): Section[] => {
      const workflows = group.pipelines.filter(matches)
      const matchesAsEmptyRepo =
        group.pipelines.length === 0 &&
        !rowFilters.status &&
        !rowFilters.cutoff &&
        (!rowFilters.organization || group.organization === rowFilters.organization) &&
        (rowFilters.providerId === undefined || group.providerIds.has(rowFilters.providerId)) &&
        (!rowFilters.query ||
          [group.repository, group.organization].some((value) => value.toLowerCase().includes(rowFilters.query)))

      if (workflows.length === 0 && !matchesAsEmptyRepo) {
        return []
      }

      const providerNames: string[] = []
      let hasPendingFetch = false

      group.providerIds.forEach((id) => {
        const provider = getProvider(id)

        providerNames.push(provider?.name ?? 'Unknown')

        if (provider?.last_fetch_status === 'never') {
          hasPendingFetch = true
        }
      })

      const lastRun = workflows.reduce<string | null>(
        (latest, p) => (p.last_run && (!latest || p.last_run > latest) ? p.last_run : latest),
        null
      )
      const status = mostSevereStatus(workflows.map((pipeline) => pipeline.status))
      const statusPipeline = workflows.find((pipeline) => pipeline.status === status)

      return [
        {
          row: {
            id: `repo-${group.fullName}`,
            type: 'repository',
            name: group.repository,
            organization: group.organization,
            pipelineCount: workflows.length,
            providerIds: group.providerIds,
            providerLabel: providerNames.sort(collator.compare).join(', '),
            hasPendingFetch,
            status,
            statusLabel: statusPipeline ? getStatusLabel(statusPipeline) : undefined,
            lastRun,
          },
          workflows: workflows.map(buildWorkflowRow).sort((a, b) => compareRows(a, b, sortStatus)),
        },
      ]
    })

    return { pinnedPipelines: pinned, sections: built.sort((a, b) => compareRows(a.row, b.row, sortStatus)) }
  }, [repoGroups, validPipelines, pinKeys, pinnedKeys, rowFilters, sortStatus, getProvider, parseRepositoryName])

  const rows = useMemo(() => {
    const result: TableRow[] = []

    sections.forEach(({ row, workflows }) => {
      const expanded = expansionOverrides[row.id] ?? filtersActive

      result.push({ ...row, expanded })

      if (expanded) {
        result.push(...workflows)
      }
    })

    return result
  }, [sections, expansionOverrides, filtersActive])

  const toggleRepo = useCallback(
    (repoId: string) => {
      setExpansionOverrides((prev) => ({ ...prev, [repoId]: !(prev[repoId] ?? filtersActive) }))
    },
    [filtersActive]
  )

  const uniqueOrganizations = useMemo(
    () => Array.from(new Set(repoGroups.map((g) => g.organization).filter((org) => org !== 'Unknown'))).sort(),
    [repoGroups]
  )

  const providerOptions = useMemo(() => {
    const providerIds = new Set(repoGroups.flatMap((group) => Array.from(group.providerIds)))

    return providers
      .filter((provider) => providerIds.has(provider.id))
      .map((provider) => ({ value: String(provider.id), label: provider.name }))
      .sort((a, b) => collator.compare(a.label, b.label))
  }, [repoGroups, providers])

  const emptyTableIssues = providers
    .filter((provider) => rowFilters.providerId === undefined || provider.id === rowFilters.providerId)
    .map(toEmptyTableIssue)
    .filter((issue): issue is EmptyTableIssue => issue !== null)
  const firstIssue = emptyTableIssues[0]

  const pageHeading = mobileShell && !isFluxView ? null : <PageHeader title={title} />

  if (error) {
    return (
      <PageContainer>
        {pageHeading}
        <ErrorState title="Couldn't load pipelines" error={error} onRetry={onRetry} retrying={loading} />
      </PageContainer>
    )
  }

  if (!loading && providers.length === 0) {
    return (
      <PageContainer>
        {pageHeading}
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

  if (loading) {
    return (
      <PageContainer>
        {pageHeading}
        <LoadingState variant="page" message="Loading pipelines…" />
      </PageContainer>
    )
  }

  const showPinnedSection = !pinnedView && providerFilter === null && pinnedPipelines.length > 0

  const pinnedGrid = (header?: string) => (
    <PinnedWorkflows
      header={header}
      pipelines={pinnedPipelines}
      getProvider={getProvider}
      onViewHistory={onViewHistory}
      onTrigger={onTrigger}
      onUnpin={(pipeline) => {
        const key = pinKeys.get(pipeline.id)

        if (key) {
          togglePin(key)
        }
      }}
      onViewMetrics={onViewMetrics}
    />
  )

  const noMatches = (
    <EmptyState
      icon={<IconFilterOff size={24} />}
      title={isFluxView ? 'No matching resources' : 'No matching workflows'}
      description={
        isFluxView
          ? 'No resources match the current filters.'
          : pinnedView
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

  return (
    <PageContainer>
      {pageHeading}

      <FilterBar
        filters={{
          search: {
            value: search,
            onChange: (value) => setFilter('search', value),
            placeholder: isFluxView ? 'Search resources, namespaces, sources…' : 'Search repositories, workflows…',
          },
          status: {
            value: statusFilter,
            onChange: (value) => setFilter('status', value),
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
            options: uniqueOrganizations,
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

      {pinnedView ? (
        pinnedPipelines.length > 0 ? (
          pinnedGrid()
        ) : filtersActive ? (
          noMatches
        ) : (
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
        )
      ) : (
        <>
          {showPinnedSection &&
            (mobileShell ? (
              pinnedGrid('Pinned')
            ) : (
              <Stack gap="sm" component="section" aria-labelledby="pinned-workflows-title">
                <SectionHeading
                  id="pinned-workflows-title"
                  icon={<IconPinFilled size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
                  label="Pinned"
                  count={pinnedPipelines.length}
                />
                {pinnedGrid()}
              </Stack>
            ))}

          <Stack gap="sm" flex={1} mih={0}>
            {!mobileShell && showPinnedSection && rows.length > 0 && (
              <SectionHeading
                icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
                label={isFluxView ? 'Sources' : 'Repositories'}
                count={sections.length}
              />
            )}

            {rows.length === 0 ? (
              filtersActive ? (
                noMatches
              ) : firstIssue ? (
                <ErrorState
                  title={`Couldn't fetch ${labels.items} from ${emptyTableIssues.map((issue) => issue.name).join(', ')}`}
                  error={[firstIssue.headline, firstIssue.detail].filter(Boolean).join(': ')}
                  hint={firstIssue.hint}
                  actions={<ProviderRecoveryActions providerId={firstIssue.id} kind={firstIssue.kind} />}
                />
              ) : (
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
              )
            ) : mobileShell ? (
              <IosPipelineList
                header={isFluxView ? 'Sources' : 'Repositories'}
                groupTitle={labels.groupTitle}
                sortStatus={sortStatus}
                onSortStatusChange={setSortStatus}
                rows={rows}
                getProvider={getProvider}
                onToggleRepo={toggleRepo}
                onViewHistory={onViewHistory}
                onTrigger={onTrigger}
                pinnedKeys={pinnedKeys}
                onTogglePin={togglePin}
                onViewMetrics={onViewMetrics}
              />
            ) : isMobile ? (
              <MobilePipelineCards
                rows={rows}
                getProvider={getProvider}
                onToggleRepo={toggleRepo}
                onViewHistory={onViewHistory}
                onTrigger={onTrigger}
                pinnedKeys={pinnedKeys}
                onTogglePin={togglePin}
                onViewMetrics={onViewMetrics}
              />
            ) : (
              <StandardTable<TableRow>
                records={rows}
                customRowAttributes={rowMenu}
                columns={[
                  {
                    accessor: 'name',
                    title: 'Name',
                    sortable: true,
                    width: '35%',
                    textAlign: 'left' as const,
                    render: (row) => {
                      if (row.type === 'repository') {
                        return (
                          <UnstyledButton
                            className="interactive-surface"
                            w="100%"
                            onClick={() => toggleRepo(row.id)}
                            aria-expanded={row.expanded}
                          >
                            <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                              {row.expanded ? (
                                <IconChevronDown size={16} style={{ flexShrink: 0 }} />
                              ) : (
                                <IconChevronRight size={16} style={{ flexShrink: 0 }} />
                              )}
                              <IconFolder size={16} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
                              {TableCells.truncated(row.name)}
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
                          className="interactive-surface"
                          w="100%"
                          pl="xl"
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
                    sortable: true,
                    width: 160,
                    textAlign: 'left' as const,
                    render: (row) => TableCells.truncatedDimmed(row.organization),
                  },
                  {
                    accessor: 'status',
                    title: 'Status',
                    sortable: true,
                    width: 110,
                    textAlign: 'left' as const,
                    render: (row) => <RowStatus row={row} />,
                  },
                  {
                    accessor: 'provider',
                    title: 'Provider',
                    sortable: true,
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
                    width: onViewMetrics ? 160 : 140,
                    textAlign: 'right' as const,
                    render: (row) => {
                      if (row.type === 'repository') {
                        return (
                          <Group justify="flex-end">
                            <RepositoryFetchIndicator row={row} />
                          </Group>
                        )
                      }

                      const { pipeline, pinKey: rowPinKey } = row

                      if (row.type === 'workflow' && pipeline && rowPinKey) {
                        return (
                          <Group justify="flex-end">
                            <WorkflowActions
                              pipeline={pipeline}
                              isPinned={pinnedKeys.includes(rowPinKey)}
                              onTogglePin={() => togglePin(rowPinKey)}
                              onTrigger={onTrigger}
                              onViewMetrics={onViewMetrics}
                            />
                          </Group>
                        )
                      }

                      return null
                    },
                  },
                ]}
                sortStatus={sortStatus}
                onSortStatusChange={setSortStatus}
              />
            )}
          </Stack>
        </>
      )}
    </PageContainer>
  )
}
