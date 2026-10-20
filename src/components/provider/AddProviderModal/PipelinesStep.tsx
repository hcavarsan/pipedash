import {
  Box,
  Button,
  Group,
  Loader,
  Checkbox as MantineCheckbox,
  Paper,
  ScrollArea,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { IconBuildingSkyscraper, IconSearch, IconStack2 } from '@tabler/icons-react'
import { type Dispatch, useEffect, useMemo } from 'react'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { useDebounce } from '../../../hooks/useDebounce'
import { usePipelinePreview, useProviderOrganizations } from '../../../queries/useProviderFieldQueries'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { AvailablePipeline, ProviderConfig } from '../../../types'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
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
  const { selectedPlugin, selectedOrganization, selectedPipelines, repositorySearch } = state

  const debouncedSearch = useDebounce(repositorySearch, DEBOUNCE_DELAYS.FILTER)

  const {
    data: organizationsData,
    isLoading: loadingOrganizations,
    isError: organizationsFailed,
    error: organizationsError,
    refetch: refetchOrganizations,
    isFetching: fetchingOrganizations,
  } = useProviderOrganizations(selectedPlugin?.provider_type || '', providerConfig, true)

  const {
    data: pipelinesData,
    isLoading: loadingPipelines,
    isFetching: searchingPipelines,
    isError: pipelinesFailed,
    error: pipelinesError,
    refetch: refetchPipelines,
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
  const availablePipelines = useMemo(() => pipelinesData?.pages.flatMap((page) => page.items) || [], [pipelinesData])
  const allPipelines = useMemo(() => pipelinesData?.pages[pipelinesData.pages.length - 1] || null, [pipelinesData])

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

  const renderBody = () => {
    if (organizationsFailed) {
      return (
        <ErrorState
          title="Couldn't load organizations"
          error={organizationsError}
          onRetry={() => refetchOrganizations()}
          retrying={fetchingOrganizations}
        />
      )
    }

    if (loadingOrganizations) {
      return <LoadingState variant="section" message="Loading organizations..." />
    }

    if (availableOrganizations.length === 0) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} />}
          title="No organizations found"
          description="This token can't access any organizations. Go back and check the credentials."
        />
      )
    }

    if (!selectedOrganization) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} />}
          title="Select an organization"
          description="Choose an organization above to list its pipelines."
        />
      )
    }

    if (pipelinesFailed) {
      return (
        <ErrorState
          title="Couldn't load pipelines"
          error={pipelinesError}
          onRetry={() => refetchPipelines()}
          retrying={searchingPipelines}
        />
      )
    }

    if (loadingPipelines) {
      return <LoadingState variant="section" message="Loading pipelines..." />
    }

    const results =
      filteredPipelines.length === 0 ? (
        repositorySearch.trim() ? (
          <EmptyState
            icon={<IconSearch size={24} />}
            title="No pipelines match your search"
            description={
              allPipelines?.has_more
                ? 'No loaded pipeline matches the current search. Load more to search further.'
                : 'No pipeline repository matches the current search.'
            }
            action={
              <Button variant="default" size="sm" onClick={() => handleSearchChange('')}>
                Clear search
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<IconStack2 size={24} />}
            title="No pipelines found"
            description="This organization has no pipelines available to this token."
          />
        )
      ) : (
        <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto">
          {isMobile ? (
            <Stack gap="xs">
              {filteredPipelines.map((pipeline) => (
                <MobilePipelineCard
                  key={pipeline.id}
                  pipeline={pipeline}
                  isSelected={selectedPipelines.has(pipeline.id)}
                  onToggle={() => handlePipelineToggle(pipeline.id)}
                />
              ))}
            </Stack>
          ) : (
            <PipelinesTable
              pipelines={filteredPipelines}
              selectedPipelines={selectedPipelines}
              onToggle={handlePipelineToggle}
              onSelectAll={handleSelectAll}
            />
          )}
        </ScrollArea>
      )

    return (
      <>
        {results}
        {allPipelines?.has_more && (
          <Group justify="center" pt={4}>
            <Button size="sm" variant="default" onClick={handleLoadMore} loading={loadingMore} disabled={loadingMore}>
              Load more
            </Button>
          </Group>
        )}
      </>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }} px="lg" pb="lg">
      <Stack gap="xs" style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {!loadingOrganizations && !organizationsFailed && (
          <Paper p="sm" style={{ flexShrink: 0, marginTop: 'var(--mantine-spacing-md)' }}>
            <Group gap="sm" wrap="nowrap" align="flex-start">
              {availableOrganizations.length > 1 && (
                <Select
                  aria-label="Organization"
                  placeholder="Select organization"
                  value={selectedOrganization}
                  onChange={handleOrganizationSelect}
                  data={availableOrganizations.map((org) => ({ value: org.id, label: org.name }))}
                  searchable
                  style={{ flex: 1 }}
                />
              )}
              <TextInput
                aria-label="Search pipelines by repository"
                placeholder="Search repositories..."
                value={repositorySearch}
                onChange={(e) => handleSearchChange(e.currentTarget.value)}
                leftSection={
                  searchingPipelines && !loadingPipelines && !loadingMore ? (
                    <Loader size={16} />
                  ) : (
                    <IconSearch size={16} />
                  )
                }
                disabled={!selectedOrganization}
                style={{ flex: 1 }}
              />
            </Group>
          </Paper>
        )}

        {renderBody()}
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
    <MantineCheckbox.Card checked={isSelected} onClick={onToggle} p="xs" aria-label={pipeline.name}>
      <Stack gap={4}>
        <Group gap="xs" wrap="nowrap">
          <MantineCheckbox.Indicator />
          <Text size="sm" fw={500} truncate style={{ flex: 1 }}>
            {pipeline.name}
          </Text>
        </Group>

        <Group gap="xs" wrap="nowrap" align="flex-start">
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
              Org
            </Text>
            <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} truncate>
              {pipeline.organization || '—'}
            </Text>
          </Box>
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
              Repo
            </Text>
            <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} truncate>
              {pipeline.repository || '—'}
            </Text>
          </Box>
        </Group>

        {pipeline.description && (
          <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} c="dimmed" lineClamp={1}>
            {pipeline.description}
          </Text>
        )}
      </Stack>
    </MantineCheckbox.Card>
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
            <MantineCheckbox
              aria-label="Select all displayed pipelines"
              checked={selectedPipelines.size === pipelines.length}
              indeterminate={selectedPipelines.size > 0 && selectedPipelines.size < pipelines.length}
              onChange={onSelectAll}
            />
          </Table.Th>
          <Table.Th style={{ width: '25%' }}>Name</Table.Th>
          <Table.Th style={{ width: '15%' }}>Organization</Table.Th>
          <Table.Th style={{ width: '20%' }}>Repository</Table.Th>
          <Table.Th style={{ width: '40%' }}>Description</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {pipelines.map((pipeline) => (
          <Table.Tr key={pipeline.id}>
            <Table.Td>
              <MantineCheckbox
                aria-label={pipeline.name}
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
