import { useCallback, useMemo, useRef, useState } from 'react'
import { DataTableSortStatus } from 'mantine-datatable'

import { ActionIcon, Alert, Box, Card, Center, Group, Loader, Modal, Paper, Stack, Text, Tooltip } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import { IconAlertCircle, IconAlertTriangle, IconChartLine, IconChevronRight, IconFolder, IconGitBranch, IconPlayerPlayFilled, IconPlugConnected } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'
import { usePipelineFilters } from '../../hooks/useUrlState'
import { logger } from '../../lib/logger'
import type { Pipeline, ProviderSummary } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { TableCells } from '../../utils/tableCells'
import { TableHeader } from '../atoms'
import { FilterBar } from '../common/FilterBar'
import { LoadingState } from '../common/LoadingState'
import { StandardTable } from '../common/StandardTable'

import { MobilePipelineCards } from './MobilePipelineCards'

type TableRowType = 'repository' | 'workflow';

interface TableRow {
  id: string;
  type: TableRowType;
  parentId?: string;

  repository?: string;
  repositoryFullName?: string;
  organization?: string;
  pipelineCount?: number;
  providerIds?: Set<number>;
  lastUpdated?: string | null;

  pipeline?: Pipeline;
  branch?: string;
  status?: string;
}

interface UnifiedPipelinesViewProps {
  pipelines: Pipeline[];
  providers: ProviderSummary[];
  selectedProviderId?: number;
  loading?: boolean;
  onViewHistory: (pipeline: Pipeline) => void;
  onTrigger: (pipeline: Pipeline) => void;
  onViewMetrics?: (pipeline: Pipeline) => void;
}

export const UnifiedPipelinesView = ({
  pipelines,
  providers,
  selectedProviderId,
  loading = false,
  onViewHistory,
  onTrigger,
  onViewMetrics,
}: UnifiedPipelinesViewProps) => {
  const orphanedPipelines = useRef<Set<string>>(new Set())

  const { isMobile, isDesktop } = useIsMobile()

  const { filters, setFilter, clearFilters } = usePipelineFilters()
  const { search, status: statusFilter, provider: providerFilter, organization: organizationFilter, dateRange } = filters

  const [expandedRepos, setExpandedRepos] = useState<Set<string>>(new Set())
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<TableRow>>({
    columnAccessor: 'repository',
    direction: 'asc',
  })
  const [errorModalOpened, { open: openErrorModal, close: closeErrorModal }] = useDisclosure(false)
  const [selectedError, setSelectedError] = useState<{ providerName: string; error: string } | null>(null)

  const getProvider = useCallback((providerId: number): ProviderSummary | undefined => {
    return providers.find((p) => p.id === providerId)
  }, [providers])

  const getRepositoryProviderErrors = (providerIds: Set<number>) => {
    const errors: Array<{ id: number; name: string; error: string }> = []


    providerIds.forEach(id => {
      const provider = providers.find(p => p.id === id)


      if (provider?.last_fetch_status === 'error' && provider.last_fetch_error) {
        errors.push({
          id: provider.id,
          name: provider.name,
          error: provider.last_fetch_error
        })
      }
    })

return errors
  }

  const parseRepositoryName = useCallback((fullName: string, providerId: number): { organization: string; repository: string } => {
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
  }, [getProvider])

  const tableRows = useMemo(() => {
    const rows: TableRow[] = []
    const repoMap = new Map<string, {
      pipelines: Pipeline[];
      providerIds: Set<number>;
      lastUpdated: string | null;
    }>()

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
              title: 'Data Inconsistency Detected',
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

      if (selectedProviderId !== undefined && pipeline.provider_id !== selectedProviderId) {
        return
      }

      const existing = repoMap.get(pipeline.repository)


      if (existing) {
        existing.pipelines.push(pipeline)
        existing.providerIds.add(pipeline.provider_id)
        if (pipeline.last_updated && (!existing.lastUpdated || pipeline.last_updated > existing.lastUpdated)) {
          existing.lastUpdated = pipeline.last_updated
        }
      } else {
        const providerIds = new Set<number>()


        providerIds.add(pipeline.provider_id)
        repoMap.set(pipeline.repository, {
          pipelines: [pipeline],
          providerIds,
          lastUpdated: pipeline.last_updated,
        })
      }
    })

    providers.forEach((provider) => {
      if (selectedProviderId !== undefined && provider.id !== selectedProviderId) {
        return
      }

      if (provider.configured_repositories && provider.configured_repositories.length > 0) {
        provider.configured_repositories.forEach((repoName) => {
          if (!repoMap.has(repoName)) {
            const providerIds = new Set<number>()


            providerIds.add(provider.id)
            repoMap.set(repoName, {
              pipelines: [],
              providerIds,
              lastUpdated: provider.last_updated || null,
            })
          } else {
            const existing = repoMap.get(repoName)


            if (existing) {
              existing.providerIds.add(provider.id)
            }
          }
        })
      }
    })

    Array.from(repoMap.entries()).forEach(([repoName, repoData]) => {
      const providerId = repoData.pipelines[0]?.provider_id || Array.from(repoData.providerIds)[0] || 0
      const { organization, repository } = parseRepositoryName(repoName, providerId)
      const repoId = `repo-${repoName}`

      rows.push({
        id: repoId,
        type: 'repository',
        repository,
        repositoryFullName: repoName,
        organization,
        pipelineCount: repoData.pipelines.length,
        providerIds: repoData.providerIds,
        lastUpdated: repoData.lastUpdated,
      })

      if (expandedRepos.has(repoId)) {
        repoData.pipelines.forEach((pipeline) => {
          rows.push({
            id: `workflow-${pipeline.id}`,
            type: 'workflow',
            parentId: repoId,
            pipeline,
          })
        })
      }
    })

    return rows
  }, [pipelines, expandedRepos, providers, selectedProviderId, getProvider, parseRepositoryName])

  const filteredRows = useMemo(() => {
    let result = tableRows

    if (search) {
      const matchingRepoIds = new Set<string>()
      const matchingWorkflowParentIds = new Set<string>()

      result.forEach((row) => {
        if (row.type === 'repository') {
          const matches = (
            row.repository?.toLowerCase().includes(search.toLowerCase()) ||
            row.organization?.toLowerCase().includes(search.toLowerCase())
          )


          if (matches) {
            matchingRepoIds.add(row.id)
          }
        } else if (row.pipeline) {
          const workflowMatches = (
            row.pipeline.name.toLowerCase().includes(search.toLowerCase()) ||
            row.pipeline.branch?.toLowerCase().includes(search.toLowerCase())
          )


          if (workflowMatches && row.parentId) {
            matchingWorkflowParentIds.add(row.parentId)
          }
        }
      })

      result = result.filter((row) => {
        if (row.type === 'repository') {
          return matchingRepoIds.has(row.id) || matchingWorkflowParentIds.has(row.id)
        } else if (row.pipeline && row.parentId) {
          const workflowMatches = (
            row.pipeline.name.toLowerCase().includes(search.toLowerCase()) ||
            row.pipeline.branch?.toLowerCase().includes(search.toLowerCase())
          )
          const parentMatches = matchingRepoIds.has(row.parentId)



return workflowMatches || parentMatches
        }

return false
      })
    }

    if (organizationFilter) {
      result = result.filter((row) => {
        if (row.type === 'repository') {
          return row.organization === organizationFilter
        }

return true
      })
    }

    if (providerFilter) {
      const provider = providers.find((p) => p.name === providerFilter)


      if (provider) {
        result = result.filter((row) => {
          if (row.type === 'repository') {
            return row.providerIds?.has(provider.id)
          } else if (row.pipeline) {
            return row.pipeline.provider_id === provider.id
          }

return false
        })
      }
    }

    if (statusFilter) {
      // Build set of repo IDs with workflows matching status
      const reposWithMatchingStatus = new Set<string>()

      pipelines.forEach((pipeline) => {
        if (pipeline.status === statusFilter && pipeline.repository) {
          reposWithMatchingStatus.add(`repo-${pipeline.repository}`)
        }
      })

      result = result.filter((row) => {
        if (row.type === 'repository') {
          return reposWithMatchingStatus.has(row.id)
        } else if (row.type === 'workflow' && row.pipeline) {
          return row.pipeline.status === statusFilter
        }

        return false
      })
    }

    if (dateRange) {
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

      if (dateRange) {
        result = result.filter((row) => {
          if (row.type === 'repository' && row.lastUpdated) {
            return new Date(row.lastUpdated) >= cutoffDate
          } else if (row.type === 'workflow' && row.pipeline?.last_run) {
            return new Date(row.pipeline.last_run) >= cutoffDate
          }

return false
        })
      }
    }

    return result
  }, [tableRows, search, organizationFilter, providerFilter, statusFilter, dateRange, providers, pipelines])

  const uniqueOrganizations = useMemo(() => {
    return Array.from(new Set(
      tableRows
        .filter(r => r.type === 'repository')
        .map(r => r.organization)
        .filter((org): org is string => !!org && org !== 'Unknown')
    )).sort()
  }, [tableRows])

  const uniqueProviderNames = useMemo(() => {
    const providerIdSet = new Set<number>()


    tableRows.forEach((row) => {
      if (row.type === 'repository' && row.providerIds) {
        row.providerIds.forEach((id) => providerIdSet.add(id))
      } else if (row.type === 'workflow' && row.pipeline) {
        providerIdSet.add(row.pipeline.provider_id)
      }
    })

return Array.from(providerIdSet)
      .map((id) => getProvider(id)?.name)
      .filter((name): name is string => !!name)
      .sort()
  }, [tableRows, getProvider])

  const repositoryCount = tableRows.filter(r => r.type === 'repository').length

  if (!loading && providers.length === 0) {
    return (
      <Box
        pt={{ base: 0, sm: 'sm' }}
        pb={{ base: 0, sm: '2xl' }}
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
        <TableHeader title="Repositories & Workflows" count={0} />
        <Card padding="xl" withBorder mt="md">
          <Center>
            <Stack align="center" gap="md">
              <IconPlugConnected size={48} color="var(--mantine-color-dimmed)" />
              <Stack align="center" gap="xs">
                <Text size={THEME_TYPOGRAPHY.MODAL_TITLE.size} fw={THEME_TYPOGRAPHY.MODAL_TITLE.weight}>No providers configured</Text>
                <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED} ta="center">
                  Add a CI/CD provider to start monitoring your pipelines and workflows.
                </Text>
              </Stack>
            </Stack>
          </Center>
        </Card>
      </Box>
    )
  }

  if (loading) {
    return (
      <Box
        pt={{ base: 0, sm: 'sm' }}
        pb={{ base: 0, sm: '2xl' }}
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
        <TableHeader title="Repositories & Workflows" count={repositoryCount} />
        <LoadingState variant="page" message="Loading pipelines..." />
      </Box>
    )
  }

  return (
    <Box
      pt={{ base: 0, sm: 'sm' }}
      pb={{ base: 0, sm: '2xl' }}
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
      <TableHeader title="Repositories & Workflows" count={repositoryCount} />

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
            options: uniqueProviderNames,
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

      {isMobile ? (
        <MobilePipelineCards
          filteredRows={filteredRows}
          pipelines={pipelines}
          expandedRepos={expandedRepos}
          setExpandedRepos={setExpandedRepos}
          getProvider={getProvider}
          parseRepositoryName={parseRepositoryName}
          onViewHistory={onViewHistory}
          onTrigger={onTrigger}
          onViewMetrics={onViewMetrics}
        />
      ) : filteredRows.length === 0 ? (
        <Card padding="xl" withBorder mt="md">
          <Center>
            <Stack align="center" gap="md">
              <IconFolder size={48} color="var(--mantine-color-dimmed)" />
              <Stack align="center" gap="xs">
                <Text size={THEME_TYPOGRAPHY.MODAL_TITLE.size} fw={THEME_TYPOGRAPHY.MODAL_TITLE.weight}>No workflows found</Text>
                <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED} ta="center">
                  {pipelines.length === 0
                    ? 'No workflows found for this provider. Workflows will appear after they run.'
                    : 'No workflows match your current filters. Try adjusting your search or filters.'}
                </Text>
              </Stack>
            </Stack>
          </Center>
        </Card>
      ) : (
        <Box style={{
          height: 'calc(100vh - var(--app-shell-header-height, 70px) - 250px)',
          minHeight: 'clamp(300px, 40vh, 500px)',
          position: 'relative'
        }}>
          <StandardTable<TableRow>
        records={filteredRows}
        columns={[
          {
            accessor: 'repository',
            title: 'Name',
            sortable: true,
            width: '35%',
            textAlign: 'left' as const,
            render: (row) => {
              if (row.type === 'repository') {
                const isExpanded = expandedRepos.has(row.id)
                const providerErrors = row.providerIds ? getRepositoryProviderErrors(row.providerIds) : []
                const hasErrors = providerErrors.length > 0



return (
                  <Group gap={8} wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                    <IconChevronRight
                      size={16}
                      style={{
                        transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
                        transition: 'transform 200ms ease',
                        flexShrink: 0,
                        opacity: hasErrors ? 0.3 : 1,
                      }}
                    />
                    <IconFolder
                      size={18}
                      color={hasErrors
                        ? 'var(--mantine-color-gray-6)'
                        : 'var(--mantine-color-blue-5)'
                      }
                      style={{ flexShrink: 0 }}
                    />
                    {TableCells.truncated(row.repository || '')}
                  </Group>
                )
              } else if (row.pipeline) {
                return (
                  <Group gap={8} wrap="nowrap" pl="xl" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                    <IconGitBranch size={16} color="var(--mantine-color-gray-6)" style={{ flexShrink: 0 }} />
                    {TableCells.truncated(row.pipeline.name)}
                  </Group>
                )
              }

return null
            },
          },
          {
            accessor: 'organization',
            title: 'Organization',
            sortable: true,
            width: 160,
            textAlign: 'left' as const,
            render: (row) => {
              if (row.type === 'repository') {
                return TableCells.truncatedDimmed(row.organization || '')
              } else if (row.pipeline) {
                const { organization } = parseRepositoryName(row.pipeline.repository, row.pipeline.provider_id)



return TableCells.truncatedDimmed(organization)
              }

return null
            },
          },
          {
            accessor: 'status',
            title: 'Status',
            sortable: true,
            width: 110,
            textAlign: 'center' as const,
            render: (row) => {
              if (row.type === 'workflow' && row.pipeline) {
                const lastActivity = row.pipeline.last_run
                  ? new Date(row.pipeline.last_run).toLocaleString()
                  : 'Never'



return (
                  <Box style={{ display: 'flex', justifyContent: 'center' }}>
                    <Tooltip label={`Last activity: ${lastActivity}`} withArrow>
                      <div>{TableCells.status(row.pipeline.status)}</div>
                    </Tooltip>
                  </Box>
                )
              } else if (row.type === 'repository' && row.repositoryFullName) {
                const repoPipelines = pipelines.filter(p => p.repository === row.repositoryFullName)


                if (repoPipelines.length > 0) {
                  const latestPipeline = repoPipelines
                    .sort((a, b) => {
                      const aTime = a.last_run ? new Date(a.last_run).getTime() : 0
                      const bTime = b.last_run ? new Date(b.last_run).getTime() : 0



return bTime - aTime
                    })[0]

                  if (latestPipeline) {
                    const lastActivity = latestPipeline.last_run
                      ? new Date(latestPipeline.last_run).toLocaleString()
                      : 'Never'



return (
                      <Box style={{ display: 'flex', justifyContent: 'center' }}>
                        <Tooltip label={`Last activity: ${lastActivity}`} withArrow>
                          <div>{TableCells.status(latestPipeline.status)}</div>
                        </Tooltip>
                      </Box>
                    )
                  }
                }
              }

return null
            },
          },
          {
            accessor: 'providerIds',
            title: 'Provider',
            sortable: true,
            width: 200,
            textAlign: 'left' as const,
            render: (row) => {
              if (row.type === 'repository' && row.providerIds) {
                const providerCount = row.providerIds.size

return (
                  <Stack gap={providerCount > 1 ? 8 : 0}>
                    {Array.from(row.providerIds).map((providerId) => {
                      const provider = getProvider(providerId)

return (
                        <div key={providerId}>
                          {TableCells.avatarName(
                            provider?.icon || null,
                            provider?.name || 'Unknown',
                            <IconPlugConnected size={14} />
                          )}
                        </div>
                      )
                    })}
                  </Stack>
                )
              } else if (row.pipeline) {
                const provider = getProvider(row.pipeline.provider_id)



return TableCells.avatarName(
                  provider?.icon || null,
                  provider?.name || 'Unknown',
                  <IconPlugConnected size={14} />
                )
              }

return null
            },
          },
          {
            accessor: 'actions',
            title: 'Actions',
            width: onViewMetrics ? 120 : 100,
            textAlign: 'center' as const,
            render: (row) => {
              if (row.type === 'repository') {
                const hasPendingFetch = Array.from(row.providerIds || []).some((id) => {
                  const provider = getProvider(id)



return provider?.last_fetch_status === 'never'
                })

                if (hasPendingFetch) {
                  return (
                    <Box style={{ display: 'flex', justifyContent: 'center' }}>
                      <Tooltip label="Loading pipelines..." withArrow>
                        <Loader size="sm" />
                      </Tooltip>
                    </Box>
                  )
                }

                const providerErrors = row.providerIds ? getRepositoryProviderErrors(row.providerIds) : []

                if (providerErrors.length > 0) {
                  return (
                    <Box style={{ display: 'flex', justifyContent: 'center' }}>
                      <Tooltip label="Provider fetch error - Click to view details" withArrow>
                        <ActionIcon
                          size="sm"
                          color="red"
                          variant="subtle"
                          onClick={(e) => {
                            e.stopPropagation()
                            setSelectedError({
                              providerName: providerErrors[0].name,
                              error: providerErrors[0].error
                            })
                            openErrorModal()
                          }}
                          style={{ backgroundColor: 'transparent' }}
                          aria-label="View provider fetch error details"
                        >
                          <IconAlertTriangle size={18} color="var(--mantine-color-red-6)" />
                        </ActionIcon>
                      </Tooltip>
                    </Box>
                  )
                }

                return null
              } else if (row.type === 'workflow' && row.pipeline) {
                return (
                  <Box style={{ display: 'flex', justifyContent: 'center', gap: 8 }}>
                    {onViewMetrics && (
                      <Tooltip label="View metrics" withArrow>
                        <ActionIcon
                          variant="subtle"
                          color="violet"
                          size="md"
                          onClick={(e) => {
                            e.stopPropagation()
                            onViewMetrics(row.pipeline!)
                          }}
                          aria-label="View metrics"
                        >
                          <IconChartLine size={18} />
                        </ActionIcon>
                      </Tooltip>
                    )}
                    <Tooltip label="Trigger workflow" withArrow>
                      <ActionIcon
                        variant="subtle"
                        color="blue"
                        size="md"
                        onClick={(e) => {
                          e.stopPropagation()
                          onTrigger(row.pipeline!)
                        }}
                        aria-label="Trigger workflow"
                      >
                        <IconPlayerPlayFilled size={18} />
                      </ActionIcon>
                    </Tooltip>
                  </Box>
                )
              }

return null
            },
          },
        ]}
        sortStatus={sortStatus}
        onSortStatusChange={setSortStatus}
        onRowClick={({ record }) => {
          if (record.type === 'repository') {
            const providerErrors = record.providerIds
              ? getRepositoryProviderErrors(record.providerIds)
              : []

            if (providerErrors.length > 0) {
              setSelectedError({
                providerName: providerErrors[0].name,
                error: providerErrors[0].error
              })
              openErrorModal()
            } else {
              setExpandedRepos((prev) => {
                const newSet = new Set(prev)


                if (newSet.has(record.id)) {
                  newSet.delete(record.id)
                } else {
                  newSet.add(record.id)
                }

return newSet
              })
            }
          } else if (record.type === 'workflow' && record.pipeline) {
            onViewHistory(record.pipeline)
          }
        }}
        rowStyle={(row) => ({
          backgroundColor: row.type === 'workflow'
            ? 'var(--mantine-color-dark-8)'
            : undefined,
          cursor: 'pointer',
        })}
        noRecordsText="No repositories found"
      />
        </Box>
      )}

      <Modal
        opened={errorModalOpened}
        onClose={closeErrorModal}
        title="Provider Fetch Error"
        size={isDesktop ? 'min(80vh, 80vw)' : 'md'}
        centered
        padding="lg"
        zIndex={300}
        styles={{
          content: isDesktop ? {
            aspectRatio: '1 / 1',
            maxHeight: '80vh',
            maxWidth: '80vw',
          } : {},
        }}
      >
        {selectedError && (
          <Stack gap="md">
            <Alert
              icon={<IconAlertCircle size={16} />}
              color="red"
              variant="light"
              title={selectedError.providerName}
            >
              <Text size="sm" c="dimmed">
                Failed to fetch pipelines from this provider
              </Text>
            </Alert>

            <Box>
              <Text size="xs" fw={500} c="dimmed" mb="xs" tt="uppercase">
                Error Details
              </Text>
              <Paper
                p="md"
                withBorder
                radius="md"
                style={{
                  backgroundColor: 'var(--mantine-color-dark-8)',
                  borderColor: 'var(--mantine-color-dark-5)',
                }}
              >
                <Text
                  size="sm"
                  c="red"
                  style={{
                    fontFamily: 'monospace',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                  }}
                >
                  {selectedError.error}
                </Text>
              </Paper>
            </Box>
          </Stack>
        )}
      </Modal>
    </Box>
  )
}
