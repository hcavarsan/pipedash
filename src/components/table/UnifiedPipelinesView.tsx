import { useMemo, useState } from 'react'
import { DataTableSortStatus } from 'mantine-datatable'

import { ActionIcon, Box, Card, Center, Container, Group, Loader, Stack, Text, Tooltip } from '@mantine/core'
import { IconChartLine, IconChevronRight, IconFolder, IconGitBranch, IconPlayerPlayFilled, IconPlugConnected } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import type { Pipeline, ProviderSummary } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { TableCells } from '../../utils/tableCells'
import { TableHeader } from '../atoms'
import { FilterBar } from '../common/FilterBar'
import { StandardTable } from '../common/StandardTable'

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
  const isMobile = useIsMobile()
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [providerFilter, setProviderFilter] = useState<string | null>(null)
  const [organizationFilter, setOrganizationFilter] = useState<string | null>(null)
  const [dateRange, setDateRange] = useState<string | null>(null)
  const [expandedRepos, setExpandedRepos] = useState<Set<string>>(new Set())
  const [sortStatus, setSortStatus] = useState<DataTableSortStatus<TableRow>>({
    columnAccessor: 'repository',
    direction: 'asc',
  })

  const getProvider = (providerId: number): ProviderSummary | undefined => {
    return providers.find((p) => p.id === providerId)
  }

  const parseRepositoryName = (fullName: string, providerId: number): { organization: string; repository: string } => {
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
  }

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
      // Get provider ID from first pipeline, or from providerIds set if no pipelines
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pipelines, expandedRepos, providers, selectedProviderId])

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
      result = result.filter((row) => {
        if (row.type === 'workflow' && row.pipeline) {
          return row.pipeline.status === statusFilter
        }

return true
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
  }, [tableRows, search, organizationFilter, providerFilter, statusFilter, dateRange, providers])

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tableRows, providers])

  const repositoryCount = tableRows.filter(r => r.type === 'repository').length

  const renderMobileCards = () => {
    return (
      <Stack gap="sm">
        {filteredRows.length === 0 ? (
          <Card padding="xl" withBorder>
            <Center>
              <Stack align="center" gap="md">
                <IconFolder size={48} color="var(--mantine-color-dimmed)" />
                <Stack align="center" gap="xs">
                  <Text size={THEME_TYPOGRAPHY.MODAL_TITLE.size} fw={THEME_TYPOGRAPHY.MODAL_TITLE.weight}>No workflows found</Text>
                  <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED} ta="center">
                    {pipelines.length === 0
                      ? 'This provider doesn\'t have any workflows configured yet.'
                      : 'No workflows match your current filters'}
                  </Text>
                </Stack>
              </Stack>
            </Center>
          </Card>
        ) : (
          filteredRows.map((row) => {
            if (row.type === 'repository') {
              const isExpanded = expandedRepos.has(row.id)
              const repoPipelines = pipelines.filter(p => p.repository === row.repositoryFullName)
              const latestPipeline = repoPipelines.sort((a, b) => {
                const aTime = a.last_run ? new Date(a.last_run).getTime() : 0
                const bTime = b.last_run ? new Date(b.last_run).getTime() : 0



return bTime - aTime
              })[0]

              return (
                <Card
                  key={row.id}
                  padding="md"
                  withBorder
                  style={{ cursor: 'pointer' }}
                  onClick={() => {
                    setExpandedRepos((prev) => {
                      const newSet = new Set(prev)


                      if (newSet.has(row.id)) {
                        newSet.delete(row.id)
                      } else {
                        newSet.add(row.id)
                      }

return newSet
                    })
                  }}
                >
                  <Stack gap="xs">
                    <Group justify="space-between" wrap="nowrap">
                      <Group gap={8} wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
                        <IconChevronRight
                          size={16}
                          style={{
                            transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
                            transition: 'transform 200ms ease',
                            flexShrink: 0,
                          }}
                        />
                        <IconFolder size={18} color="var(--mantine-color-blue-5)" style={{ flexShrink: 0 }} />
                        <Text size={THEME_TYPOGRAPHY.ITEM_TITLE.size} fw={THEME_TYPOGRAPHY.ITEM_TITLE.weight} truncate style={{ flex: 1 }}>
                          {row.repository}
                        </Text>
                      </Group>
                      {latestPipeline && (
                        <Tooltip
                          label={`Last activity: ${latestPipeline.last_run ? new Date(latestPipeline.last_run).toLocaleString() : 'Never'}`}
                          withArrow
                        >
                          <div>{TableCells.status(latestPipeline.status)}</div>
                        </Tooltip>
                      )}
                    </Group>

                    <Group gap="md" wrap="wrap">
                      <Box style={{ flex: '1 1 auto' }}>
                        <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Organization</Text>
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{row.organization}</Text>
                      </Box>
                      <Box style={{ flex: '1 1 auto' }}>
                        <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Pipelines</Text>
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{row.pipelineCount}</Text>
                      </Box>
                    </Group>

                    <Box>
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL} mb={4}>Providers</Text>
                      <Stack gap={(row.providerIds?.size || 0) > 1 ? 8 : 0}>
                        {Array.from(row.providerIds || []).map((providerId) => {
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
                    </Box>
                  </Stack>
                </Card>
              )
            } else if (row.type === 'workflow' && row.pipeline) {
              const provider = getProvider(row.pipeline.provider_id)
              const { organization } = parseRepositoryName(row.pipeline.repository, row.pipeline.provider_id)

              return (
                <Card
                  key={row.id}
                  padding="md"
                  withBorder
                  ml="xl"
                  style={{ cursor: 'pointer', backgroundColor: 'var(--mantine-color-dark-8)' }}
                  onClick={() => onViewHistory(row.pipeline!)}
                >
                  <Stack gap="xs">
                    <Group justify="space-between" wrap="nowrap">
                      <Group gap={8} wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
                        <IconGitBranch size={16} color="var(--mantine-color-gray-6)" style={{ flexShrink: 0 }} />
                        <Text size={THEME_TYPOGRAPHY.ITEM_TITLE.size} fw={THEME_TYPOGRAPHY.ITEM_TITLE.weight} truncate style={{ flex: 1 }}>
                          {row.pipeline.name}
                        </Text>
                      </Group>
                      <Group gap={8}>
                        <Tooltip
                          label={`Last activity: ${row.pipeline.last_run ? new Date(row.pipeline.last_run).toLocaleString() : 'Never'}`}
                          withArrow
                        >
                          <div>{TableCells.status(row.pipeline.status)}</div>
                        </Tooltip>
                        {onViewMetrics && (
                          <ActionIcon
                            variant="subtle"
                            color="violet"
                            size="md"
                            onClick={(e) => {
                              e.stopPropagation()
                              onViewMetrics(row.pipeline!)
                            }}
                            title="View metrics"
                          >
                            <IconChartLine size={18} />
                          </ActionIcon>
                        )}
                        <ActionIcon
                          variant="subtle"
                          color="blue"
                          size="md"
                          onClick={(e) => {
                            e.stopPropagation()
                            onTrigger(row.pipeline!)
                          }}
                          title="Trigger workflow"
                        >
                          <IconPlayerPlayFilled size={18} />
                        </ActionIcon>
                      </Group>
                    </Group>

                    <Group gap="md" wrap="wrap">
                      <Box style={{ flex: '1 1 auto' }}>
                        <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Organization</Text>
                        <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{organization}</Text>
                      </Box>
                      {row.pipeline.branch && (
                        <Box style={{ flex: '1 1 auto' }}>
                          <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Branch</Text>
                          <Text size={THEME_TYPOGRAPHY.FIELD_VALUE.size} c={THEME_COLORS.VALUE_TEXT}>{row.pipeline.branch}</Text>
                        </Box>
                      )}
                    </Group>

                    <Box>
                      <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL} mb={4}>Provider</Text>
                      {TableCells.avatarName(
                        provider?.icon || null,
                        provider?.name || 'Unknown',
                        <IconPlugConnected size={14} />
                      )}
                    </Box>
                  </Stack>
                </Card>
              )
            }

return null
          })
        )}
      </Stack>
    )
  }

  if (loading) {
    return (
      <Container size="100%" pt={{ base: 'xs', sm: 'sm' }} pb={{ base: 'xs', sm: '2xl' }} px={{ base: 'xs', sm: 'xl' }} style={{ maxWidth: '100%' }}>
        <TableHeader title="Repositories & Workflows" count={repositoryCount} />
        <Center py="xl">
          <Stack align="center" gap="md">
            <Loader size="lg" />
            <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>Loading pipelines...</Text>
          </Stack>
        </Center>
      </Container>
    )
  }

  return (
    <Container size="100%" pt={{ base: 'xs', sm: 'sm' }} pb={{ base: 'xs', sm: '2xl' }} px={{ base: 'xs', sm: 'xl' }} style={{ maxWidth: '100%' }}>
      <TableHeader title="Repositories & Workflows" count={repositoryCount} />

      <FilterBar
        filters={{
          search: {
            value: search,
            onChange: setSearch,
            placeholder: 'Search repositories, workflows...',
          },
          status: {
            value: statusFilter,
            onChange: setStatusFilter,
          },
          provider: {
            value: providerFilter,
            onChange: setProviderFilter,
            options: uniqueProviderNames,
          },
          organization: {
            value: organizationFilter,
            onChange: setOrganizationFilter,
            options: uniqueOrganizations,
          },
          dateRange: {
            value: dateRange,
            onChange: setDateRange,
          },
        }}
      />

      {/* Render mobile cards on small screens, table on desktop */}
      {isMobile ? (
        renderMobileCards()
      ) : filteredRows.length === 0 ? (
        <Card padding="xl" withBorder mt="md">
          <Center>
            <Stack align="center" gap="md">
              <IconFolder size={48} color="var(--mantine-color-dimmed)" />
              <Stack align="center" gap="xs">
                <Text size={THEME_TYPOGRAPHY.MODAL_TITLE.size} fw={THEME_TYPOGRAPHY.MODAL_TITLE.weight}>No workflows found</Text>
                <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED} ta="center">
                  {pipelines.length === 0
                    ? 'This provider doesn\'t have any workflows configured yet.'
                    : 'No workflows match your current filters'}
                </Text>
              </Stack>
            </Stack>
          </Center>
        </Card>
      ) : (
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



return (
                  <Group gap={8} wrap="nowrap" style={{ overflow: 'hidden', maxWidth: '100%' }}>
                    <IconChevronRight
                      size={16}
                      style={{
                        transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
                        transition: 'transform 200ms ease',
                        flexShrink: 0,
                      }}
                    />
                    <IconFolder size={18} color="var(--mantine-color-blue-5)" style={{ flexShrink: 0 }} />
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
                // Show aggregate status for repository (most recent status from ALL pipelines)
                const repoPipelines = pipelines.filter(p => p.repository === row.repositoryFullName)


                if (repoPipelines.length > 0) {
                  // Get the most recent pipeline status
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
              if (row.type === 'workflow' && row.pipeline) {
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
            setExpandedRepos((prev) => {
              const newSet = new Set(prev)


              if (newSet.has(record.id)) {
                newSet.delete(record.id)
              } else {
                newSet.add(record.id)
              }

return newSet
            })
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
      )}
    </Container>
  )
}
