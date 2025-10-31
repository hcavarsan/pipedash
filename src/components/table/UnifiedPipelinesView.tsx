import { useMemo, useState } from 'react'
import { DataTableSortStatus } from 'mantine-datatable'

import { ActionIcon, Box, Card, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconChevronRight, IconFolder, IconGitBranch, IconPlayerPlayFilled, IconPlugConnected } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import type { Pipeline, ProviderSummary } from '../../types'
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
  onViewHistory: (pipeline: Pipeline) => void;
  onTrigger: (pipeline: Pipeline) => void;
}

export const UnifiedPipelinesView = ({
  pipelines,
  providers,
  onViewHistory,
  onTrigger,
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

    Array.from(repoMap.entries()).forEach(([repoName, repoData]) => {
      const providerId = repoData.pipelines[0]?.provider_id || 0
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
  }, [pipelines, expandedRepos])

  // Apply filters
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

  // Mobile card view
  const renderMobileCards = () => {
    return (
      <Stack gap="sm">
        {filteredRows.length === 0 ? (
          <Card padding="lg" withBorder>
            <Text size="sm" c="dimmed" ta="center">
              No repositories found
            </Text>
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
                        <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
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
                        <Text size="xs" c="dimmed">Organization</Text>
                        <Text size="sm">{row.organization}</Text>
                      </Box>
                      <Box style={{ flex: '1 1 auto' }}>
                        <Text size="xs" c="dimmed">Pipelines</Text>
                        <Text size="sm">{row.pipelineCount}</Text>
                      </Box>
                    </Group>

                    <Box>
                      <Text size="xs" c="dimmed" mb={4}>Providers</Text>
                      <Group gap={8}>
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
                      </Group>
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
                        <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
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
                      </Group>
                    </Group>

                    <Group gap="md" wrap="wrap">
                      <Box style={{ flex: '1 1 auto' }}>
                        <Text size="xs" c="dimmed">Organization</Text>
                        <Text size="sm">{organization}</Text>
                      </Box>
                      {row.pipeline.branch && (
                        <Box style={{ flex: '1 1 auto' }}>
                          <Text size="xs" c="dimmed">Branch</Text>
                          <Text size="sm">{row.pipeline.branch}</Text>
                        </Box>
                      )}
                    </Group>

                    <Box>
                      <Text size="xs" c="dimmed" mb={4}>Provider</Text>
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

  return (
    <Box>
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
                return (
                  <Group gap={8} wrap="nowrap">
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
                  </Group>
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
            width: 100,
            textAlign: 'center' as const,
            render: (row) => {
              if (row.type === 'workflow' && row.pipeline) {
                return (
                  <Box style={{ display: 'flex', justifyContent: 'center' }}>
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
    </Box>
  )
}
