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
import { useMobileShell } from '../../../hooks/useIsMobile'
import { usePipelinePreview, useProviderOrganizations } from '../../../queries/useProviderFieldQueries'
import { TYPOGRAPHY } from '../../../theme/typography'
import type { AvailablePipeline, ProviderConfig } from '../../../types'
import { getPipelineLabels, type PipelineLabels } from '../../../utils/pipelinePresentation'
import { EmptyState } from '../../common/EmptyState'
import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'

import { CheckCircle, ShellField, StepHeading } from './ShellForm'
import shell from './ShellForm.module.css'
import type { FormAction, FormState } from './types'

interface PipelinesStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  providerConfig: ProviderConfig
  isMobile: boolean
}

export function PipelinesStep({ state, dispatch, providerConfig, isMobile }: PipelinesStepProps) {
  const mobileShell = useMobileShell()
  const { selectedPlugin, selectedOrganization, selectedPipelines, repositorySearch } = state
  const labels = getPipelineLabels(selectedPlugin?.provider_type)

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

  useEffect(() => {
    if (availableOrganizations.length === 1 && !selectedOrganization) {
      dispatch({ type: 'SET_ORGANIZATION', organization: availableOrganizations[0].id })
    }
  }, [availableOrganizations, selectedOrganization, dispatch])

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
    dispatch({
      type: availablePipelines.every((pipeline) => selectedPipelines.has(pipeline.id))
        ? 'DESELECT_PIPELINES'
        : 'SELECT_ALL_PIPELINES',
      pipelineIds: availablePipelines.map((pipeline) => pipeline.id),
    })
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
          title={`Couldn't load ${labels.groups}`}
          error={organizationsError}
          onRetry={() => refetchOrganizations()}
          retrying={fetchingOrganizations}
        />
      )
    }

    if (loadingOrganizations) {
      return <LoadingState variant="section" message={`Loading ${labels.groups}...`} />
    }

    if (availableOrganizations.length === 0) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} />}
          title={`No ${labels.groups} found`}
          description={`No accessible ${labels.items} were found. Check the cluster, namespace scope and credentials.`}
        />
      )
    }

    if (!selectedOrganization) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} />}
          title={`Select a ${labels.group}`}
          description={`Choose a ${labels.group} above to list its ${labels.items}.`}
        />
      )
    }

    if (pipelinesFailed) {
      return (
        <ErrorState
          title={`Couldn't load ${labels.items}`}
          error={pipelinesError}
          onRetry={() => refetchPipelines()}
          retrying={searchingPipelines}
        />
      )
    }

    if (loadingPipelines) {
      return <LoadingState variant="section" message={`Loading ${labels.items}...`} />
    }

    const results =
      availablePipelines.length === 0 ? (
        repositorySearch.trim() ? (
          <EmptyState
            icon={<IconSearch size={24} />}
            title={`No ${labels.items} match your search`}
            description={`Try a different name, type or source.`}
            action={
              <Button variant="default" size="sm" onClick={() => handleSearchChange('')}>
                Clear search
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<IconStack2 size={24} />}
            title={`No ${labels.items} found`}
            description={`This ${labels.group} has no accessible ${labels.items}.`}
          />
        )
      ) : mobileShell ? (
        <ShellPipelineList
          pipelines={availablePipelines}
          selectedPipelines={selectedPipelines}
          onToggle={handlePipelineToggle}
          onSelectAll={handleSelectAll}
        />
      ) : (
        <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto">
          {isMobile ? (
            <Stack gap="xs">
              {availablePipelines.map((pipeline) => (
                <MobilePipelineCard
                  key={pipeline.id}
                  labels={labels}
                  pipeline={pipeline}
                  isSelected={selectedPipelines.has(pipeline.id)}
                  onToggle={() => handlePipelineToggle(pipeline.id)}
                />
              ))}
            </Stack>
          ) : (
            <PipelinesTable
              pipelines={availablePipelines}
              labels={labels}
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
            <Button
              size={mobileShell ? 'md' : 'sm'}
              variant="default"
              fullWidth={mobileShell}
              onClick={handleLoadMore}
              loading={loadingMore}
              disabled={loadingMore}
            >
              Load more
            </Button>
          </Group>
        )}
      </>
    )
  }

  if (mobileShell) {
    const ready = !loadingOrganizations && !organizationsFailed

    return (
      <div className={shell.step}>
        <StepHeading title={`Select ${labels.items}`} subtitle={`Choose ${labels.items} to monitor`} />
        {ready && availableOrganizations.length > 1 && (
          <ShellField label={labels.groupTitle}>
            <Select
              variant="unstyled"
              aria-label={labels.groupTitle}
              placeholder={`Select ${labels.group}`}
              value={selectedOrganization}
              onChange={handleOrganizationSelect}
              data={availableOrganizations.map((org) => ({ value: org.id, label: org.name }))}
              searchable
            />
          </ShellField>
        )}
        {ready && (
          <div className={shell.search}>
            <TextInput
              aria-label={`Search ${labels.items}`}
              placeholder={`Search ${labels.items}`}
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
            />
          </div>
        )}
        {renderBody()}
      </div>
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
                  aria-label={labels.groupTitle}
                  placeholder={`Select ${labels.group}`}
                  value={selectedOrganization}
                  onChange={handleOrganizationSelect}
                  data={availableOrganizations.map((org) => ({ value: org.id, label: org.name }))}
                  searchable
                  style={{ flex: 1 }}
                />
              )}
              <TextInput
                aria-label={`Search ${labels.items}`}
                placeholder={`Search ${labels.items}...`}
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
  labels: PipelineLabels
}

function MobilePipelineCard({ pipeline, isSelected, onToggle, labels }: MobilePipelineCardProps) {
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
              {labels.groupTitle}
            </Text>
            <Text size={TYPOGRAPHY.FIELD_VALUE_SMALL.size} truncate>
              {pipeline.organization || '—'}
            </Text>
          </Box>
          <Box style={{ flex: 1, minWidth: 0 }}>
            <Text size={TYPOGRAPHY.FIELD_LABEL.size} c="dimmed">
              {labels.sourceTitle}
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
  labels: PipelineLabels
}

function PipelinesTable({ pipelines, selectedPipelines, onToggle, onSelectAll, labels }: PipelinesTableProps) {
  const selectedCount = pipelines.filter((pipeline) => selectedPipelines.has(pipeline.id)).length
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
              aria-label={`Select all displayed ${labels.items}`}
              checked={pipelines.length > 0 && selectedCount === pipelines.length}
              indeterminate={selectedCount > 0 && selectedCount < pipelines.length}
              onChange={onSelectAll}
            />
          </Table.Th>
          <Table.Th style={{ width: '35%' }}>Name</Table.Th>
          <Table.Th style={{ width: '20%' }}>{labels.groupTitle}</Table.Th>
          <Table.Th style={{ width: '20%' }}>{labels.sourceTitle}</Table.Th>
          <Table.Th style={{ width: '25%' }}>Description</Table.Th>
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

interface ShellPipelineListProps {
  pipelines: AvailablePipeline[]
  selectedPipelines: Set<string>
  onToggle: (id: string) => void
  onSelectAll: () => void
}

function ShellPipelineList({ pipelines, selectedPipelines, onToggle, onSelectAll }: ShellPipelineListProps) {
  const allSelected = pipelines.every((pipeline) => selectedPipelines.has(pipeline.id))

  return (
    <IosGroup>
      <IosRow
        title={allSelected ? 'Deselect all' : 'Select all'}
        accessory={<CheckCircle checked={allSelected} />}
        onClick={onSelectAll}
      />
      {pipelines.map((pipeline) => {
        const selected = selectedPipelines.has(pipeline.id)
        const source = [pipeline.organization, pipeline.repository].filter(Boolean).join(' · ')

        return (
          <IosRow
            key={pipeline.id}
            title={pipeline.name}
            subtitle={
              <>
                {source && <span className={shell.clamp}>{source}</span>}
                {pipeline.description && <span className={shell.clamp}>{pipeline.description}</span>}
              </>
            }
            accessory={<CheckCircle checked={selected} />}
            onClick={() => onToggle(pipeline.id)}
            ariaLabel={`${pipeline.name}, ${selected ? 'selected' : 'not selected'}`}
          />
        )
      })}
    </IosGroup>
  )
}
