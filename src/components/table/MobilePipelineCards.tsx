import { ActionIcon, Box, Card, Center, Group, Stack, Text, Tooltip } from '@mantine/core'
import { IconChartLine, IconChevronRight, IconFolder, IconGitBranch, IconPlayerPlayFilled, IconPlugConnected } from '@tabler/icons-react'

import type { Pipeline, ProviderSummary } from '../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../utils/dynamicRenderers'
import { TableCells } from '../../utils/tableCells'

interface TableRow {
  id: string
  type: 'repository' | 'workflow'
  parentId?: string
  repository?: string
  repositoryFullName?: string
  organization?: string
  pipelineCount?: number
  providerIds?: Set<number>
  lastUpdated?: string | null
  pipeline?: Pipeline
}

interface MobilePipelineCardsProps {
  filteredRows: TableRow[]
  pipelines: Pipeline[]
  expandedRepos: Set<string>
  setExpandedRepos: React.Dispatch<React.SetStateAction<Set<string>>>
  getProvider: (providerId: number) => ProviderSummary | undefined
  parseRepositoryName: (fullName: string, providerId: number) => { organization: string; repository: string }
  onViewHistory: (pipeline: Pipeline) => void
  onTrigger: (pipeline: Pipeline) => void
  onViewMetrics?: (pipeline: Pipeline) => void
}

export function MobilePipelineCards({
  filteredRows,
  pipelines,
  expandedRepos,
  setExpandedRepos,
  getProvider,
  parseRepositoryName,
  onViewHistory,
  onTrigger,
  onViewMetrics,
}: MobilePipelineCardsProps) {
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
                          aria-label="View metrics"
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
                        aria-label="Trigger workflow"
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
