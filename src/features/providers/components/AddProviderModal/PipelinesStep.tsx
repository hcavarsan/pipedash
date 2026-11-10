import { Box, Group, Loader, Stack, Text } from '@mantine/core'
import { IconBuildingSkyscraper } from '@tabler/icons-react'
import { type Dispatch, type ReactNode, useMemo, useRef } from 'react'
import { EmptyState } from '../../../../components/common/EmptyState'
import { ErrorState } from '../../../../components/common/ErrorState'
import { LoadingState } from '../../../../components/common/LoadingState'
import { LiveStatus } from '../../../../components/forms/LiveStatus'
import shell from '../../../../components/forms/ShellForm.module.css'
import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { useMobileShell } from '../../../../hooks/useIsMobile'
import type { PipelineTerminology } from '../../../../types/presentation'
import type { ProviderConfig } from '../../../../types/providers'
import { usePipelinePreview, useProviderOrganizations } from '../../queries/useProviderFieldQueries'
import { PipelineResults } from './PipelineResults'
import { getPipelineId } from './PipelineSelectionLists'
import { DesktopPipelineControls, ShellPipelineControls } from './PipelinesStepControls'
import { StepHeading } from './ShellForm'
import type { FormAction, FormState } from './types'
import { useSelectEveryMatch } from './useSelectEveryMatch'

interface PipelinesStepProps {
  state: FormState
  dispatch: Dispatch<FormAction>
  providerConfig: ProviderConfig
  terms: PipelineTerminology
  isMobile: boolean
}

interface ListStatusParams {
  items: string
  selectingAll: boolean
  searching: boolean
  listed: number | null
}

function listStatus({ items, selectingAll, searching, listed }: ListStatusParams): string {
  if (selectingAll) {
    return `Selecting all ${items}`
  }

  if (searching) {
    return `Searching ${items}`
  }

  return listed === null ? '' : `${listed} ${items} listed`
}

export function PipelinesStep({ state, dispatch, providerConfig, terms, isMobile }: PipelinesStepProps): ReactNode {
  const mobileShell = useMobileShell()
  const { selectedPlugin, selectedOrganization, selectedPipelines, repositorySearch, selectingAll } = state

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
    isFetchNextPageError: nextPageFailed,
    isFetchingNextPage: loadingMore,
  } = usePipelinePreview(
    selectedPlugin?.provider_type || '',
    providerConfig,
    effectiveOrganization || undefined,
    debouncedSearch || undefined,
    true
  )

  const availablePipelines = useMemo(() => pipelinesData?.pages.flatMap((page) => page.items) ?? [], [pipelinesData])
  const hasMore = !!hasNextPage
  const listChanges = useRef(0)
  const searchPending = repositorySearch !== debouncedSearch

  const searching =
    !!effectiveOrganization && !loadingPipelines && !loadingMore && (searchingPipelines || searchPending)
  const status = listStatus({
    items: terms.items,
    selectingAll,
    searching,
    listed: effectiveOrganization && !loadingPipelines ? availablePipelines.length : null,
  })

  const handleOrganizationSelect = (org: string | null): void => {
    if (org) {
      listChanges.current += 1
      dispatch({ type: 'SET_ORGANIZATION', organization: org })
    }
  }

  const handleSearchChange = (value: string): void => {
    listChanges.current += 1
    dispatch({ type: 'SET_REPOSITORY_SEARCH', search: value })
  }

  const selectEveryMatch = useSelectEveryMatch({
    dispatch,
    providerType: selectedPlugin?.provider_type ?? '',
    config: providerConfig.config,
    organization: effectiveOrganization,
    search: debouncedSearch,
    listChanges,
  })

  const handleSelectAll = (): void => {
    if (selectingAll || searchPending) {
      return
    }
    if (!hasMore && availablePipelines.every((pipeline) => selectedPipelines.has(pipeline.id))) {
      dispatch({ type: 'DESELECT_PIPELINES', pipelineIds: availablePipelines.map(getPipelineId) })
      return
    }
    void selectEveryMatch(state.selectAllRun + 1)
  }

  const handleLoadMore = (): void => {
    if (hasMore && !loadingMore && !selectingAll && !nextPageFailed) {
      void fetchNextPage()
    }
  }

  const retryPipelines = (): void => {
    void (nextPageFailed ? fetchNextPage() : refetchPipelines())
  }

  const renderBody = (): ReactNode => {
    if (organizationsFailed) {
      return (
        <ErrorState
          title={`Couldn't load ${terms.groups}`}
          error={organizationsError}
          onRetry={() => void refetchOrganizations()}
          retrying={fetchingOrganizations}
        />
      )
    }

    if (loadingOrganizations) {
      return <LoadingState variant="section" message={`Loading ${terms.groups}...`} />
    }

    if (availableOrganizations.length === 0) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} aria-hidden />}
          title={`No ${terms.groups} found`}
          description={
            mobileShell
              ? `No accessible ${terms.items} were found. Check the cluster, namespace scope and credentials.`
              : `Go back to Credentials to check the connection scope and access to your ${terms.groups}.`
          }
        />
      )
    }

    if (!effectiveOrganization) {
      return (
        <EmptyState
          icon={<IconBuildingSkyscraper size={24} aria-hidden />}
          title={`Select a ${terms.group}`}
          description={`Choose a ${terms.group} above to list its ${terms.items}.`}
        />
      )
    }

    if (pipelinesFailed && !pipelinesData) {
      return (
        <ErrorState
          title={`Couldn't load ${terms.items}`}
          error={pipelinesError}
          onRetry={retryPipelines}
          retrying={searchingPipelines}
        />
      )
    }

    if (loadingPipelines) {
      return <LoadingState variant="section" message={`Loading ${terms.items}...`} />
    }

    return (
      <>
        <PipelineResults
          pipelines={availablePipelines}
          selectedPipelines={selectedPipelines}
          dispatch={dispatch}
          terms={terms}
          hasMore={hasMore}
          searchActive={repositorySearch.trim() !== ''}
          isMobile={isMobile}
          onClearSearch={() => handleSearchChange('')}
          onSelectAll={handleSelectAll}
          onEndReached={handleLoadMore}
        />
        {pipelinesFailed && (
          <ErrorState
            title={`Couldn't load ${terms.items}`}
            error={pipelinesError}
            onRetry={retryPipelines}
            retrying={searchingPipelines}
          />
        )}
        {(loadingMore || selectingAll) && (
          <Group justify="center" gap="xs" py="3xs" style={{ flexShrink: 0 }}>
            <Loader size="xs" aria-hidden />
            <Text size="sm" c="dimmed">
              {selectingAll ? `Selecting all ${terms.items}…` : `Loading more ${terms.items}…`}
            </Text>
          </Group>
        )}
      </>
    )
  }

  const controls = {
    terms,
    organizations: availableOrganizations,
    effectiveOrganization,
    search: repositorySearch,
    onOrganizationSelect: handleOrganizationSelect,
    onSearchChange: handleSearchChange,
  }

  if (mobileShell) {
    const ready = !loadingOrganizations && !organizationsFailed

    return (
      <div className={shell.step}>
        <StepHeading title={`Select ${terms.items}`} subtitle={`Choose ${terms.items} to monitor`} />
        {ready && <ShellPipelineControls {...controls} />}
        <LiveStatus message={status} />
        {renderBody()}
      </div>
    )
  }

  return (
    <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }} px="lg" pb="lg">
      <Stack gap="xs" style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {!loadingOrganizations && !organizationsFailed && (
          <DesktopPipelineControls {...controls} searching={searching} />
        )}

        <LiveStatus message={status} />
        {renderBody()}
      </Stack>
    </Box>
  )
}
