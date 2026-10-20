import { Alert, Box, Button, Code, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import {
  IconAlertCircle,
  IconChevronDown,
  IconChevronRight,
  IconFilterOff,
  IconFolder,
  IconGitBranch,
  IconPlugConnected,
} from '@tabler/icons-react'
import type { DataTableSortStatus } from 'mantine-datatable'
import { useCallback, useMemo, useRef, useState } from 'react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { usePipelineFilters } from '../../hooks/useUrlState'
import { logger } from '../../lib/logger'
import { pinKey, usePinnedStore } from '../../stores/pinnedStore'
import { useProviderStore } from '../../stores/providerStore'
import { mostSevereStatus } from '../../theme/status'
import { TYPOGRAPHY } from '../../theme/typography'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'
import { getDateRangeStart } from '../../utils/dateRange'
import { formatCount } from '../../utils/formatCount'
import { TableCells } from '../../utils/tableCells'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { PageContainer } from '../common/PageContainer'
import { PageHeader } from '../common/PageHeader'
import { StandardModal } from '../common/StandardModal'
import { StandardTable } from '../common/StandardTable'

import { MobilePipelineCards } from './MobilePipelineCards'
import { PinnedWorkflows } from './PinnedWorkflows'
import {
  type ProviderFetchError,
  RepositoryFetchIndicator,
  RowStatus,
  SectionHeading,
  type TableRow,
  WorkflowActions,
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

const compareRows = (a: TableRow, b: TableRow, sort: DataTableSortStatus<TableRow>) => {
  let primary = 0

  if (sort.columnAccessor === 'status') {
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

  const { filters, setFilter, clearFilters } = usePipelineFilters()
  const {
    search,
    status: statusFilter,
    provider: providerFilter,
    organization: organizationFilter,
    dateRange,
  } = filters

  const pinnedKeys = usePinnedStore((s) => s.pinnedKeys)
  const togglePin = usePinnedStore((s) => s.togglePin)
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)

  const [expansionOverrides, setExpansionOverrides] = useState<Record<string, boolean>>({})
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<TableRow>>({
    columnAccessor: 'name',
    direction: 'asc',
  })
  const [errorModalOpened, { open: openErrorModal, close: closeErrorModal }] = useDisclosure(false)
  const [selectedError, setSelectedError] = useState<ProviderFetchError | null>(null)

  const getProvider = useCallback(
    (providerId: number): ProviderSummary | undefined => {
      return providers.find((p) => p.id === providerId)
    },
    [providers]
  )

  const showProviderError = useCallback(
    (error: ProviderFetchError) => {
      setSelectedError(error)
      openErrorModal()
    },
    [openErrorModal]
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

      const providerErrors: ProviderFetchError[] = []
      const providerNames: string[] = []
      let hasPendingFetch = false

      group.providerIds.forEach((id) => {
        const provider = getProvider(id)

        providerNames.push(provider?.name ?? 'Unknown')

        if (provider?.last_fetch_status === 'never') {
          hasPendingFetch = true
        }

        if (provider?.last_fetch_status === 'error' && provider.last_fetch_error) {
          providerErrors.push({ id: provider.id, name: provider.name, error: provider.last_fetch_error })
        }
      })

      const lastRun = workflows.reduce<string | null>(
        (latest, p) => (p.last_run && (!latest || p.last_run > latest) ? p.last_run : latest),
        null
      )

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
            providerErrors,
            hasPendingFetch,
            status: mostSevereStatus(workflows.map((p) => p.status)),
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

  const fetchErrors = providers.filter(
    (provider) =>
      provider.last_fetch_status === 'error' &&
      provider.last_fetch_error &&
      (rowFilters.providerId === undefined || provider.id === rowFilters.providerId)
  )

  if (error) {
    return (
      <PageContainer>
        <PageHeader title="Pipelines" />
        <ErrorState title="Couldn't load pipelines" error={error} onRetry={onRetry} retrying={loading} />
      </PageContainer>
    )
  }

  if (!loading && providers.length === 0) {
    return (
      <PageContainer>
        <PageHeader title="Pipelines" />
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
        <PageHeader title="Pipelines" />
        <LoadingState variant="page" message="Loading pipelines…" />
      </PageContainer>
    )
  }

  return (
    <PageContainer>
      <PageHeader title="Pipelines" badge={formatCount(repoGroups.length, 'repository', 'repositories')} />

      <FilterBar
        filters={{
          search: {
            value: search,
            onChange: (value) => setFilter('search', value),
            placeholder: 'Search repositories, workflows...',
          },
          status: {
            value: statusFilter,
            onChange: (value) => setFilter('status', value),
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
          },
          dateRange: {
            value: dateRange,
            onChange: (value) => setFilter('dateRange', value),
          },
        }}
        onClearAll={clearFilters}
      />

      {pinnedPipelines.length > 0 && (
        <PinnedWorkflows
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
      )}

      <Stack gap="sm" flex={1} mih={0}>
        {pinnedPipelines.length > 0 && rows.length > 0 && (
          <SectionHeading
            icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
            label="Repositories"
            count={sections.length}
          />
        )}

        {rows.length === 0 ? (
          filtersActive ? (
            <EmptyState
              icon={<IconFilterOff size={24} />}
              title="No matching workflows"
              description="No repositories or workflows match the current filters."
              action={
                <Button variant="default" size="sm" onClick={clearFilters}>
                  Clear filters
                </Button>
              }
            />
          ) : fetchErrors.length > 0 ? (
            <ErrorState
              title={`Couldn't fetch workflows from ${fetchErrors.map((p) => p.name).join(', ')}`}
              error={fetchErrors[0].last_fetch_error}
              hint="Check the provider credentials in Settings → Providers. Pipedash retries on the next refresh."
            />
          ) : (
            <EmptyState
              icon={<IconFolder size={24} />}
              title="No workflows yet"
              description="Workflows appear here after they run on your configured providers."
            />
          )
        ) : isMobile ? (
          <MobilePipelineCards
            rows={rows}
            getProvider={getProvider}
            onToggleRepo={toggleRepo}
            onShowProviderError={showProviderError}
            onViewHistory={onViewHistory}
            onTrigger={onTrigger}
            pinnedKeys={pinnedKeys}
            onTogglePin={togglePin}
            onViewMetrics={onViewMetrics}
          />
        ) : (
          <Box style={{ flex: 1, minHeight: 360, position: 'relative' }}>
            <StandardTable<TableRow>
              records={rows}
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
                        <UnstyledButton w="100%" onClick={() => toggleRepo(row.id)} aria-expanded={row.expanded}>
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
                        w="100%"
                        pl="xl"
                        onClick={() => onViewHistory(pipeline)}
                        aria-label={`View run history for ${row.name}`}
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
                  title: 'Organization',
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
                  textAlign: 'center' as const,
                  render: (row) => (
                    <Group justify="center">
                      <RowStatus row={row} />
                    </Group>
                  ),
                },
                {
                  accessor: 'provider',
                  title: 'Provider',
                  sortable: true,
                  width: 200,
                  textAlign: 'left' as const,
                  render: (row) => (
                    <Stack gap="xs">
                      {Array.from(row.providerIds ?? []).map((providerId) => (
                        <div key={providerId}>{TableCells.provider(getProvider(providerId))}</div>
                      ))}
                    </Stack>
                  ),
                },
                {
                  accessor: 'actions',
                  title: 'Actions',
                  width: onViewMetrics ? 160 : 140,
                  textAlign: 'center' as const,
                  render: (row) => {
                    if (row.type === 'repository') {
                      return (
                        <Group justify="center">
                          <RepositoryFetchIndicator row={row} onShowError={showProviderError} />
                        </Group>
                      )
                    }

                    const { pipeline, pinKey: rowPinKey } = row

                    if (row.type === 'workflow' && pipeline && rowPinKey) {
                      return (
                        <Group justify="center">
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
          </Box>
        )}
      </Stack>

      <StandardModal
        opened={errorModalOpened}
        onClose={closeErrorModal}
        title="Provider fetch error"
        size="md"
        footer={
          <Group justify="flex-end" gap="sm">
            <Button variant="default" size="sm" onClick={closeErrorModal}>
              Close
            </Button>
          </Group>
        }
      >
        {selectedError && (
          <Stack gap="md">
            <Alert icon={<IconAlertCircle size={16} />} color="red" variant="light" title={selectedError.name}>
              <Text size={TYPOGRAPHY.HELPER_TEXT.size}>Couldn't fetch pipelines from this provider.</Text>
            </Alert>

            <Box>
              <Text size={TYPOGRAPHY.FIELD_LABEL.size} fw={500} c="dimmed" mb="xs">
                Error details
              </Text>
              <Code block style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                {selectedError.error}
              </Code>
            </Box>
          </Stack>
        )}
      </StandardModal>
    </PageContainer>
  )
}
