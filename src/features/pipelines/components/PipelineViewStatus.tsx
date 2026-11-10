import { Button, Group } from '@mantine/core'
import { IconEyeOff, IconFilterOff, IconFolder, IconFolderOff, IconPin, IconPlugConnected } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { BuildWatchSection } from '../../../components/common/BuildWatchControls'
import { EmptyState } from '../../../components/common/EmptyState'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { PageContainer } from '../../../components/common/PageContainer'
import { ProviderRecoveryActions } from '../../../components/common/ProviderRecoveryActions'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useProviderStore } from '../../../stores/providerStore'
import { getErrorCode } from '../../../types/errors'
import type { ProviderRecovery, ProviderSummary } from '../../../types/providers'
import type { ViewDisplay } from '../../../types/views'
import { isDesktopTauri } from '../../../utils/platform'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { PipelineViewNav, PipelineViewToolbar } from './PipelineViewToolbar'

interface EmptyTableIssue {
  id: number
  name: string
  recovery: ProviderRecovery | null
  headline: string
  detail: string | null
  hint: string | null
}

const toEmptyTableIssue = (provider: ProviderSummary): EmptyTableIssue | null => {
  const { attention } = provider
  const base = { id: provider.id, name: provider.name, recovery: attention.recovery }

  if (attention.headline) {
    return { ...base, headline: attention.headline, detail: attention.detail, hint: attention.hint }
  }

  if (provider.last_fetch_status === 'error' && provider.last_fetch_error) {
    return { ...base, headline: 'Fetch failed', detail: provider.last_fetch_error, hint: attention.fetch_hint }
  }

  return null
}

interface PipelineBuildWatchProps {
  model: PipelineViewPageModel
  onViewRun: (pipelineId: string, runNumber: number) => void
}

export function PipelineBuildWatch({ model, onViewRun }: PipelineBuildWatchProps): ReactNode {
  if (model.query.view !== 'all' || model.filters.provider !== null || model.pushed) {
    return null
  }

  return (
    <ErrorBoundary scope="feature" name="Build watch">
      <BuildWatchSection onViewRun={onViewRun} />
    </ErrorBoundary>
  )
}

interface PipelineViewStatusProps extends PipelineBuildWatchProps {
  onRetry: () => void
  onDisplayChange: (display: ViewDisplay) => void
  children: ReactNode
}

export function PipelineViewStatus({
  model,
  onRetry,
  onViewRun,
  onDisplayChange,
  children,
}: PipelineViewStatusProps): ReactNode {
  const { error, loading, retrying, providers, page, view, display } = model
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)

  if (getErrorCode(error) === 'NOT_FOUND') {
    return (
      <PageContainer>
        <PipelineViewNav model={model} />
        <EmptyState
          icon={<IconFolderOff size={24} aria-hidden />}
          title="View not found"
          description="It may have been deleted or renamed in config.toml."
          action={
            <Button component={Link} to="/pipelines" variant="default" size="sm">
              Show all pipelines
            </Button>
          }
        />
      </PageContainer>
    )
  }

  if (error && !page) {
    return (
      <PageContainer>
        <PipelineViewToolbar model={model} onDisplayChange={onDisplayChange} />
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
        <ErrorState title="Couldn't load pipelines" error={error} onRetry={onRetry} retrying={retrying} />
      </PageContainer>
    )
  }

  if (!loading && providers.length === 0) {
    return (
      <PageContainer>
        <PipelineViewToolbar model={model} onDisplayChange={onDisplayChange} />
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
        <EmptyState
          icon={<IconPlugConnected size={24} aria-hidden />}
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

  if (loading || !page || !view || !display) {
    return (
      <PageContainer>
        <PipelineViewToolbar model={model} onDisplayChange={onDisplayChange} />
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
        <LoadingState variant="page" message="Loading pipelines…" />
      </PageContainer>
    )
  }

  return children
}

export function PipelineViewReloadError({
  model,
  onRetry,
}: {
  model: PipelineViewPageModel
  onRetry: () => void
}): ReactNode {
  if (!model.error || !model.page) {
    return null
  }

  return <ErrorState title="Couldn't load pipelines" error={model.error} onRetry={onRetry} retrying={model.retrying} />
}

export function PipelineViewEmptyState({
  model,
  onRetry,
}: {
  model: PipelineViewPageModel
  onRetry: () => void
}): ReactNode {
  const { page, view, kind, mobileShell, showHidden, filtersActive, providers, query } = model

  if (!page || !view) {
    return null
  }

  const { copy, terminology } = page.presentation
  const issues = providers
    .filter((provider) => query.provider_id === undefined || provider.id === query.provider_id)
    .map(toEmptyTableIssue)
    .filter((issue): issue is EmptyTableIssue => issue !== null)
  const firstIssue = issues[0]

  if (filtersActive) {
    return (
      <EmptyState
        icon={<IconFilterOff size={24} aria-hidden />}
        title={copy.matching_empty.title}
        description={copy.matching_empty.message}
        action={
          <Button variant="default" size="sm" onClick={model.clearFilters}>
            Clear filters
          </Button>
        }
      />
    )
  }

  if (kind === 'all' && firstIssue) {
    return (
      <ErrorState
        title={`Couldn't fetch ${terminology.items} from ${issues.map((issue) => issue.name).join(', ')}`}
        error={[firstIssue.headline, firstIssue.detail].filter(Boolean).join(': ')}
        hint={firstIssue.hint}
        actions={<ProviderRecoveryActions providerId={firstIssue.id} recovery={firstIssue.recovery} />}
      />
    )
  }

  if (page.hidden_count > 0 && !showHidden) {
    return (
      <EmptyState
        icon={<IconEyeOff size={24} aria-hidden />}
        title={copy.hidden_empty.title}
        description={copy.hidden_empty.message}
        action={
          <Button variant="default" size="sm" onClick={() => model.setFilter('hidden', 'show')}>
            Show hidden
          </Button>
        }
      />
    )
  }

  if (kind === 'all') {
    return (
      <EmptyState
        icon={<IconFolder size={24} aria-hidden />}
        title={copy.empty.title}
        description={copy.empty.message}
        action={
          !mobileShell && (
            <Button variant="default" size="sm" onClick={onRetry}>
              Refresh
            </Button>
          )
        }
      />
    )
  }

  if (kind === 'pinned') {
    return (
      <EmptyState
        icon={<IconPin size={24} aria-hidden />}
        title={copy.pinned_empty.title}
        description={`${copy.pinned_empty.message}${isDesktopTauri() ? ' Pinned pipelines also appear in the optional desktop menu.' : ''}`}
        action={
          <Group gap="xs">
            <Button component={Link} to="/pipelines" variant="default" size="sm">
              Browse {terminology.items}
            </Button>
            <Button variant="default" size="sm" onClick={() => usePipelineViewStore.getState().openEditor('pinned')}>
              Manage pins
            </Button>
          </Group>
        }
      />
    )
  }

  return (
    <EmptyState
      icon={<IconFolder size={24} aria-hidden />}
      title="No pipelines in this view"
      description={
        view.capabilities.edit_rules
          ? "Use Add to view on any pipeline, or edit the view's rules."
          : "No pipelines match this view's rules in config.toml."
      }
      action={
        view.capabilities.edit_rules ? (
          <Button variant="default" size="sm" onClick={() => usePipelineViewStore.getState().openEditor(view.id)}>
            Edit view
          </Button>
        ) : undefined
      }
    />
  )
}
