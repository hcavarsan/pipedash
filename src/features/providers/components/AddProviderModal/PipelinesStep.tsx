import {
  Box,
  Button,
  Group,
  Loader,
  Checkbox as MantineCheckbox,
  NativeSelect,
  Paper,
  Select,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { IconBuildingSkyscraper, IconSearch, IconStack2 } from '@tabler/icons-react'
import { type Dispatch, type ReactNode, useMemo } from 'react'
import { EmptyState } from '../../../../components/common/EmptyState'
import { ErrorState } from '../../../../components/common/ErrorState'
import { FillScrollArea } from '../../../../components/common/FillScrollArea'
import { LoadingState } from '../../../../components/common/LoadingState'
import { SearchInput } from '../../../../components/common/SearchInput'
import { VirtualList } from '../../../../components/common/VirtualList'
import { VirtualTable, type VirtualTableColumn } from '../../../../components/common/VirtualTable'
import { LiveStatus } from '../../../../components/forms/LiveStatus'
import shell from '../../../../components/forms/ShellForm.module.css'
import { IosGroup, IosRow } from '../../../../components/ios/IosList'
import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import { TYPOGRAPHY } from '../../../../theme/typography'
import type { AvailablePipeline, ProviderConfig } from '../../../../types/domain'
import { getPipelineLabels, type PipelineLabels } from '../../../../utils/pipelinePresentation'
import { usePipelinePreview, useProviderOrganizations } from '../../queries/useProviderFieldQueries'
import { CheckCircle, ShellField, StepHeading } from './ShellForm'
import type { FormAction, FormState } from './types'

interface PipelinesStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  providerConfig: ProviderConfig
  isMobile: boolean
}

interface PipelineListProps {
  pipelines: AvailablePipeline[]
  selectedPipelines: Set<string>
  dispatch: Dispatch<FormAction>
  labels: PipelineLabels
}

const getPipelineId = (pipeline: AvailablePipeline): string => pipeline.id

function toggleSelection(
  { pipelines, selectedPipelines, dispatch }: Omit<PipelineListProps, 'labels'>,
  next: Set<string>
): void {
  const visible = pipelines.map(getPipelineId)
  const added = visible.filter((id) => next.has(id) && !selectedPipelines.has(id))
  const removed = visible.filter((id) => !next.has(id) && selectedPipelines.has(id))

  if (added.length > 0) {
    dispatch({ type: 'SELECT_ALL_PIPELINES', pipelineIds: added })
  }

  if (removed.length > 0) {
    dispatch({ type: 'DESELECT_PIPELINES', pipelineIds: removed })
  }
}

export function PipelinesStep({ state, dispatch, providerConfig, isMobile }: PipelinesStepProps): ReactNode {
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

  const availableOrganizations = organizationsData ?? []
  const [onlyOrganization] = availableOrganizations
  const effectiveOrganization =
    selectedOrganization || (availableOrganizations.length === 1 && onlyOrganization ? onlyOrganization.id : '')

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
    effectiveOrganization || undefined,
    debouncedSearch || undefined,
    true
  )

  const availablePipelines = useMemo(() => pipelinesData?.pages.flatMap((page) => page.items) ?? [], [pipelinesData])
  const lastPage = pipelinesData?.pages.at(-1) ?? null
  const searching =
    !!effectiveOrganization &&
    !loadingPipelines &&
    !loadingMore &&
    (searchingPipelines || repositorySearch !== debouncedSearch)
  const status = searching
    ? `Searching ${labels.items}`
    : effectiveOrganization && !loadingPipelines
      ? `${availablePipelines.length} ${labels.items} listed`
      : ''

  const handleOrganizationSelect = (org: string | null): void => {
    if (org) {
      dispatch({ type: 'SET_ORGANIZATION', organization: org })
    }
  }

  const handleSearchChange = (value: string): void => {
    dispatch({ type: 'SET_REPOSITORY_SEARCH', search: value })
  }

  const handleSelectAll = (): void => {
    dispatch({
      type: availablePipelines.every((pipeline) => selectedPipelines.has(pipeline.id))
        ? 'DESELECT_PIPELINES'
        : 'SELECT_ALL_PIPELINES',
      pipelineIds: availablePipelines.map(getPipelineId),
    })
  }

  const handleLoadMore = (): void => {
    if (hasNextPage && !loadingMore) {
      void fetchNextPage()
    }
  }

  const renderResults = (): ReactNode => {
    if (availablePipelines.length === 0) {
      return repositorySearch.trim() ? (
        <EmptyState
          icon={<IconSearch size={24} aria-hidden />}
          title={`No ${labels.items} match your search`}
          description="Try a different name, type or source."
          action={
            <Button variant="default" size="sm" onClick={() => handleSearchChange('')}>
              Clear search
            </Button>
          }
        />
      ) : (
        <EmptyState
          icon={<IconStack2 size={24} aria-hidden />}
          title={`No ${labels.items} found`}
          description={
            mobileShell
              ? `This ${labels.group} has no accessible ${labels.items}.`
              : `This ${labels.group} has no accessible ${labels.items}. Go back to Credentials to check scope and permissions.`
          }
        />
      )
    }

    const list = { pipelines: availablePipelines, selectedPipelines, dispatch, labels }

    if (mobileShell) {
      return <ShellPipelineList {...list} onSelectAll={handleSelectAll} />
    }

    if (isMobile) {
      return (
        <FillScrollArea type="auto">
          <Stack gap="xs">
            {availablePipelines.map((pipeline) => (
              <MobilePipelineCard
                key={pipeline.id}
                labels={labels}
                pipeline={pipeline}
                isSelected={selectedPipelines.has(pipeline.id)}
                onToggle={() => dispatch({ type: 'TOGGLE_PIPELINE', pipelineId: pipeline.id })}
              />
            ))}
          </Stack>
        </FillScrollArea>
      )
    }

    return <PipelinesTable {...list} />
  }

  const renderBody = (): ReactNode => {
    if (organizationsFailed) {
      return (
        <ErrorState
          title={`Couldn't load ${labels.groups}`}
          error={organizationsError}
          onRetry={() => void refetchOrganizations()}
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
          icon={<IconBuildingSkyscraper size={24} aria-hidden />}
          title={`No ${labels.groups} found`}
          description={
            mobileShell
              ? `No accessible ${labels.items} were found. Check the cluster, namespace scope and credentials.`
              : `Go back to Credentials to check the connection scope and access to your ${labels.groups}.`
          }
        />
      )
    }

    if (!effectiveOrganization) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} aria-hidden />}
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
          onRetry={() => void refetchPipelines()}
          retrying={searchingPipelines}
        />
      )
    }

    if (loadingPipelines) {
      return <LoadingState variant="section" message={`Loading ${labels.items}...`} />
    }

    return (
      <>
        {renderResults()}
        {lastPage?.has_more && (
          <Group justify="center" pt={mobileShell ? 4 : '3xs'}>
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
            <NativeSelect
              variant="unstyled"
              aria-label={labels.groupTitle}
              value={effectiveOrganization}
              onChange={(event) => handleOrganizationSelect(event.currentTarget.value)}
              data={[
                { value: '', label: `Select ${labels.group}` },
                ...availableOrganizations.map((org) => ({ value: org.id, label: org.name })),
              ]}
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
                searching ? (
                  <Loader size={16} aria-label={`Searching ${labels.items}`} />
                ) : (
                  <IconSearch size={16} aria-hidden />
                )
              }
              disabled={!effectiveOrganization}
              type="search"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              enterKeyHint="search"
            />
          </div>
        )}
        <LiveStatus message={status} />
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
                  value={effectiveOrganization || null}
                  onChange={handleOrganizationSelect}
                  data={availableOrganizations.map((org) => ({ value: org.id, label: org.name }))}
                  searchable
                  style={{ flex: 1 }}
                />
              )}
              <SearchInput
                aria-label={`Search ${labels.items}`}
                placeholder={`Search ${labels.items}...`}
                value={repositorySearch}
                onChange={handleSearchChange}
                loading={searching}
                loadingLabel={`Searching ${labels.items}`}
                disabled={!effectiveOrganization}
                style={{ flex: 1 }}
              />
            </Group>
          </Paper>
        )}

        <LiveStatus message={status} />
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

function MobilePipelineCard({ pipeline, isSelected, onToggle, labels }: MobilePipelineCardProps): ReactNode {
  return (
    <MantineCheckbox.Card checked={isSelected} onClick={onToggle} p="xs" aria-label={pipeline.name}>
      <Stack gap="3xs">
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

const TABLE_FIELDS = ['name', 'organization', 'repository', 'description'] as const

function PipelinesTable({ pipelines, selectedPipelines, dispatch, labels }: PipelineListProps): ReactNode {
  const titles = {
    name: 'Name',
    organization: labels.groupTitle,
    repository: labels.sourceTitle,
    description: 'Description',
  }
  const widths = { name: '35%', organization: '20%', repository: '20%', description: '25%' }
  const columns: VirtualTableColumn<AvailablePipeline>[] = TABLE_FIELDS.map((key) => ({
    id: key,
    title: titles[key],
    width: widths[key],
    render: (pipeline) => (
      <Tooltip label={pipeline[key] || '—'} openDelay={500}>
        <Text
          size="sm"
          {...(key === 'name' ? { fw: 500 } : {})}
          {...(key === 'description' ? { c: 'dimmed' } : {})}
          truncate="end"
        >
          {pipeline[key] || '—'}
        </Text>
      </Tooltip>
    ),
  }))

  return (
    <Box style={{ flex: 1, minHeight: 0 }}>
      <VirtualTable
        records={pipelines}
        columns={columns}
        getKey={getPipelineId}
        height="100%"
        rowHeight={44}
        label={`Available ${labels.items}`}
        selection={{
          selectedKeys: selectedPipelines,
          isSelectable: () => true,
          onChange: (next) => toggleSelection({ pipelines, selectedPipelines, dispatch }, next),
          label: (pipeline) => pipeline.name,
        }}
      />
    </Box>
  )
}

function ShellPipelineList({
  pipelines,
  selectedPipelines,
  dispatch,
  onSelectAll,
}: PipelineListProps & { onSelectAll: () => void }): ReactNode {
  const allSelected = pipelines.every((pipeline) => selectedPipelines.has(pipeline.id))

  return (
    <IosGroup>
      <IosRow
        title={allSelected ? 'Deselect all' : 'Select all'}
        value={selectedPipelines.size > 0 ? `${selectedPipelines.size} selected` : undefined}
        accessory={<CheckCircle checked={allSelected} />}
        selected={allSelected}
        onClick={onSelectAll}
      />
      <VirtualList
        items={pipelines}
        getKey={getPipelineId}
        estimateSize={64}
        renderItem={(pipeline) => {
          const selected = selectedPipelines.has(pipeline.id)
          const source = [pipeline.organization, pipeline.repository].filter(Boolean).join(' · ')

          return (
            <IosRow
              title={pipeline.name}
              subtitle={
                <>
                  {source && <span className={shell.clamp}>{source}</span>}
                  {pipeline.description && <span className={shell.clamp}>{pipeline.description}</span>}
                </>
              }
              accessory={<CheckCircle checked={selected} />}
              selected={selected}
              onClick={() => dispatch({ type: 'TOGGLE_PIPELINE', pipelineId: pipeline.id })}
              ariaLabel={pipeline.name}
            />
          )
        }}
      />
    </IosGroup>
  )
}
