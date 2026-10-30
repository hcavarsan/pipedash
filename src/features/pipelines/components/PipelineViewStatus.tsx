import { Button } from '@mantine/core'
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
import type { ProviderRecovery, ProviderSummary, ViewDisplay } from '../../../types/domain'
import { getErrorCode } from '../../../types/errors'
import { fetchHint, providerAttention } from '../../../utils/providerAttention'
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
  const attention = providerAttention(provider)
  const base = { id: provider.id, name: provider.name, recovery: attention.recovery }

  if (attention.headline) {
    return { ...base, headline: attention.headline, detail: attention.detail, hint: attention.hint }
  }

  if (provider.last_fetch_status === 'error' && provider.last_fetch_error) {
    return { ...base, headline: 'Fetch failed', detail: provider.last_fetch_error, hint: fetchHint(provider) }
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
  const { error, loading, providers, page, view, display } = model
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

  if (error) {
    return (
      <PageContainer>
        <PipelineViewToolbar model={model} onDisplayChange={onDisplayChange} />
        <PipelineBuildWatch model={model} onViewRun={onViewRun} />
        <ErrorState title="Couldn't load pipelines" error={error} onRetry={onRetry} retrying={loading} />
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

export function PipelineViewEmptyState({
  model,
  onRetry,
}: {
  model: PipelineViewPageModel
  onRetry: () => void
}): ReactNode {
  const { page, view, kind, isFluxView, labels, mobileShell, showHidden, filtersActive, providers, query } = model

  if (!page || !view) {
    return null
  }

  const issues = providers
    .filter((provider) => query.provider_id === undefined || provider.id === query.provider_id)
    .map(toEmptyTableIssue)
    .filter((issue): issue is EmptyTableIssue => issue !== null)
  const firstIssue = issues[0]

  if (filtersActive) {
    return (
      <EmptyState
        icon={<IconFilterOff size={24} aria-hidden />}
        title={isFluxView ? 'No matching resources' : 'No matching workflows'}
        description={
          isFluxView
            ? 'No resources match the current filters.'
            : kind === 'pinned'
              ? 'No pinned workflows match the current filters.'
              : 'No repositories or workflows match the current filters.'
        }
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
        title={`Couldn't fetch ${labels.items} from ${issues.map((issue) => issue.name).join(', ')}`}
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
        title={isFluxView ? 'All resources are hidden' : 'All workflows are hidden'}
        description="Turn on Show hidden pipelines in View options to list them and unhide them."
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
        title={isFluxView ? 'No FluxCD resources found' : 'No workflows yet'}
        description={
          isFluxView
            ? 'Check the configured namespaces and resource selection in Settings → Providers.'
            : 'Workflows appear here after they run on your configured providers.'
        }
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
        title={isFluxView ? 'No pinned resources' : 'No pinned workflows'}
        description={
          mobileShell
            ? `Swipe a ${labels.item} left or open its Actions menu and choose Pin to keep it here.`
            : `Use the pin icon on a ${labels.item} to keep it here.`
        }
        action={
          <Button component={Link} to="/pipelines" variant="default" size="sm">
            Browse {labels.items}
          </Button>
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
