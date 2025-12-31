import { Dispatch, useEffect, useMemo } from 'react'

import {
  Box,
  Button,
  Card,
  Checkbox as MantineCheckbox,
  Group,
  Loader,
  Paper,
  ScrollArea,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { IconSearch } from '@tabler/icons-react'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { useDebounce } from '../../../hooks/useDebounce'
import { usePipelinePreview, useProviderOrganizations } from '../../../queries/useProviderFieldQueries'
import type { AvailablePipeline, ProviderConfig } from '../../../types'
import { THEME_COLORS, THEME_TYPOGRAPHY } from '../../../utils/dynamicRenderers'
import { LoadingState } from '../../common/LoadingState'

import type { FormAction, FormState } from './types'

interface PipelinesStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  providerConfig: ProviderConfig
  editMode: boolean
  existingProvider?: ProviderConfig & { id: number }
  isMobile: boolean
}

export function PipelinesStep({
  state,
  dispatch,
  providerConfig,
  editMode,
  existingProvider,
  isMobile,
}: PipelinesStepProps) {
  const {
    selectedPlugin,
    selectedOrganization,
    selectedPipelines,
    repositorySearch,
  } = state

  const debouncedSearch = useDebounce(repositorySearch, DEBOUNCE_DELAYS.FILTER)

  const {
    data: organizationsData,
    isLoading: loadingOrganizations,
  } = useProviderOrganizations(
    selectedPlugin?.provider_type || '',
    providerConfig,
    true
  )

  const {
    data: pipelinesData,
    isLoading: loadingPipelines,
    isFetching: searchingPipelines,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage: loadingMore,
  } = usePipelinePreview(
    selectedPlugin?.provider_type || '',
    providerConfig,
    selectedOrganization || undefined,
    debouncedSearch || undefined,
    true
  )

  const availableOrganizations = useMemo(() => organizationsData || [], [organizationsData])
  const availablePipelines = useMemo(() =>
    pipelinesData?.pages.flatMap((page) => page.items) || [],
  [pipelinesData]
  )
  const allPipelines = useMemo(() =>
    pipelinesData?.pages[pipelinesData.pages.length - 1] || null,
  [pipelinesData]
  )

  const filteredPipelines = useMemo(() => {
    if (!repositorySearch.trim()) {
      return availablePipelines
    }
    const searchLower = repositorySearch.toLowerCase()

    return availablePipelines.filter((pipeline) => {
      return pipeline.repository?.toLowerCase().includes(searchLower)
    })
  }, [availablePipelines, repositorySearch])

  useEffect(() => {
    if (availableOrganizations.length === 1 && !selectedOrganization) {
      dispatch({ type: 'SET_ORGANIZATION', organization: availableOrganizations[0].id })
    }

    if (editMode && existingProvider && availablePipelines.length > 0) {
      const existingPipelineIds = new Set<string>()
      const selectedItems = existingProvider.config.selected_items || ''

      if (selectedItems) {
        selectedItems.split(',').forEach((id) => {
          const trimmed = id.trim()

          if (trimmed) {
            existingPipelineIds.add(trimmed)
          }
        })
      }
      dispatch({ type: 'SET_SELECTED_PIPELINES', pipelineIds: existingPipelineIds })
    }
  }, [availableOrganizations, selectedOrganization, editMode, existingProvider, availablePipelines.length, dispatch])

  const handleOrganizationSelect = (org: string | null) => {
    if (org) {
      dispatch({ type: 'SET_ORGANIZATION', organization: org })
    }
  }

  const handleSearchChange = (value: string) => {
    dispatch({ type: 'SET_REPOSITORY_SEARCH', search: value })
  }

  const handlePipelineToggle = (pipelineId: string) => {
    dispatch({ type: 'TOGGLE_PIPELINE', pipelineId })
  }

  const handleSelectAll = () => {
    if (selectedPipelines.size === filteredPipelines.length) {
      dispatch({ type: 'CLEAR_SELECTED_PIPELINES' })
    } else {
      dispatch({ type: 'SELECT_ALL_PIPELINES', pipelineIds: filteredPipelines.map((p) => p.id) })
    }
  }

  const handleLoadMore = () => {
    if (hasNextPage && !loadingMore) {
      fetchNextPage()
    }
  }

  const renderMobilePipelineCards = () => {
    return (
      <Stack gap="xs">
        {filteredPipelines.map((pipeline) => {
          const isSelected = selectedPipelines.has(pipeline.id)

          return (
            <MobilePipelineCard
              key={pipeline.id}
              pipeline={pipeline}
              isSelected={isSelected}
              onToggle={() => handlePipelineToggle(pipeline.id)}
            />
          )
        })}
      </Stack>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }} px="md">
      <Stack gap="xs" style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <Paper p="sm" withBorder style={{ flexShrink: 0, marginTop: 'var(--mantine-spacing-md)' }}>
          {loadingOrganizations ? (
            <LoadingState variant="section" message="Loading organizations..." minHeight="100px" />
          ) : (
            <Group gap="sm" wrap="nowrap" align="flex-start">
              {availableOrganizations.length > 1 && (
                <Select
                  placeholder="Select organization"
                  value={selectedOrganization}
                  onChange={handleOrganizationSelect}
                  data={availableOrganizations.map((org) => ({ value: org.id, label: org.name }))}
                  searchable
                  disabled={loadingPipelines}
                  rightSection={loadingPipelines ? <Loader size="xs" /> : undefined}
                  style={{ flex: 1 }}
                  styles={{
                    input: {
                      height: 36,
                      fontSize: '0.875rem',
                    },
                  }}
                />
              )}
              <TextInput
                placeholder="Search repositories..."
                value={repositorySearch}
                onChange={(e) => handleSearchChange(e.currentTarget.value)}
                leftSection={searchingPipelines ? <Loader size={16} /> : <IconSearch size={16} />}
                disabled={!selectedOrganization}
                style={{ flex: 1 }}
                styles={{
                  input: {
                    height: 36,
                    fontSize: '0.875rem',
                  },
                }}
              />
            </Group>
          )}
        </Paper>

        {loadingOrganizations ? (
          <LoadingState variant="section" message="Fetching organizations..." />
        ) : loadingPipelines ? (
          <LoadingState variant="section" message="Loading pipelines..." />
        ) : !selectedOrganization ? (
          <Box style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Text size={THEME_TYPOGRAPHY.HELPER_TEXT.size} c={THEME_COLORS.DIMMED}>
              {availableOrganizations.length > 1 ? 'Select organization above' : 'No organizations found'}
            </Text>
          </Box>
        ) : isMobile ? (
          <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto">
            {renderMobilePipelineCards()}
          </ScrollArea>
        ) : (
          <>
            <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto">
              <PipelinesTable
                pipelines={filteredPipelines}
                selectedPipelines={selectedPipelines}
                onToggle={handlePipelineToggle}
                onSelectAll={handleSelectAll}
              />
            </ScrollArea>

            {allPipelines && selectedOrganization && allPipelines.has_more && (
              <Group justify="center" pt={4}>
                <Button
                  size="xs"
                  variant="filled"
                  color="dark.5"
                  onClick={handleLoadMore}
                  loading={loadingMore}
                  disabled={loadingMore}
                >
                  Load More
                </Button>
              </Group>
            )}
          </>
        )}
      </Stack>
    </Box>
  )
}


interface MobilePipelineCardProps {
  pipeline: AvailablePipeline
  isSelected: boolean
  onToggle: () => void
}

function MobilePipelineCard({ pipeline, isSelected, onToggle }: MobilePipelineCardProps) {
  return (
    <Card
      padding="xs"
      withBorder
      style={{
        cursor: 'pointer',
        backgroundColor: isSelected ? 'var(--mantine-color-blue-light)' : undefined,
      }}
      onClick={onToggle}
    >
      <Stack gap={4}>
        <Group justify="space-between" wrap="nowrap">
          <Group gap={8} wrap="nowrap" style={{ flex: 1, overflow: 'hidden' }}>
            <MantineCheckbox
              checked={isSelected}
              onChange={onToggle}
              style={{ flexShrink: 0 }}
            />
            <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
              {pipeline.name}
            </Text>
          </Group>
        </Group>

        <Group gap="xs" wrap="nowrap" align="flex-start">
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Org</Text>
            <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.VALUE_TEXT} truncate>
              {pipeline.organization || '—'}
            </Text>
          </Box>
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={THEME_TYPOGRAPHY.FIELD_LABEL.size} c={THEME_COLORS.FIELD_LABEL}>Repo</Text>
            <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.VALUE_TEXT} truncate>
              {pipeline.repository || '—'}
            </Text>
          </Box>
        </Group>

        {pipeline.description && (
          <Box>
            <Text size={THEME_TYPOGRAPHY.FIELD_VALUE_SMALL.size} c={THEME_COLORS.DIMMED} lineClamp={1}>
              {pipeline.description}
            </Text>
          </Box>
        )}
      </Stack>
    </Card>
  )
}

interface PipelinesTableProps {
  pipelines: AvailablePipeline[]
  selectedPipelines: Set<string>
  onToggle: (id: string) => void
  onSelectAll: () => void
}

function PipelinesTable({ pipelines, selectedPipelines, onToggle, onSelectAll }: PipelinesTableProps) {
  return (
    <Table
      highlightOnHover
      verticalSpacing="xs"
      styles={{
        tr: {
          height: 44,
        },
      }}
    >
      <Table.Thead>
        <Table.Tr>
          <Table.Th style={{ width: 50 }}>
            {pipelines.length > 0 && (
              <MantineCheckbox
                checked={selectedPipelines.size === pipelines.length && pipelines.length > 0}
                indeterminate={selectedPipelines.size > 0 && selectedPipelines.size < pipelines.length}
                onChange={onSelectAll}
              />
            )}
          </Table.Th>
          <Table.Th style={{ width: '25%' }}>Name</Table.Th>
          <Table.Th style={{ width: '15%' }}>Organization</Table.Th>
          <Table.Th style={{ width: '20%' }}>Repository</Table.Th>
          <Table.Th style={{ width: '40%' }}>Description</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {pipelines.map((pipeline) => (
          <Table.Tr
            key={pipeline.id}
            onClick={() => onToggle(pipeline.id)}
            style={{ cursor: 'pointer', height: 44 }}
          >
            <Table.Td onClick={(e) => e.stopPropagation()}>
              <MantineCheckbox
                checked={selectedPipelines.has(pipeline.id)}
                onChange={() => onToggle(pipeline.id)}
              />
            </Table.Td>
            <Table.Td style={{ maxWidth: 0 }}>
              <Tooltip label={pipeline.name} openDelay={500}>
                <Text size="sm" fw={500} truncate="end">
                  {pipeline.name}
                </Text>
              </Tooltip>
            </Table.Td>
            <Table.Td style={{ maxWidth: 0 }}>
              <Tooltip label={pipeline.organization || '—'} openDelay={500}>
                <Text size="sm" truncate="end">
                  {pipeline.organization || '—'}
                </Text>
              </Tooltip>
            </Table.Td>
            <Table.Td style={{ maxWidth: 0 }}>
              <Tooltip label={pipeline.repository || '—'} openDelay={500}>
                <Text size="sm" truncate="end">
                  {pipeline.repository || '—'}
                </Text>
              </Tooltip>
            </Table.Td>
            <Table.Td style={{ maxWidth: 0 }}>
              <Tooltip label={pipeline.description || '—'} openDelay={500}>
                <Text size="sm" c="dimmed" truncate="end">
                  {pipeline.description || '—'}
                </Text>
              </Tooltip>
            </Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  )
}
