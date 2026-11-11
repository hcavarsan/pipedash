import { Button, SegmentedControl, Stack, Text } from '@mantine/core'
import { lazy, type ReactNode, Suspense } from 'react'
import { useNavigate } from 'react-router-dom'
import { EmptyState } from '../../../components/common/EmptyState'
import { LoadingState } from '../../../components/common/LoadingState'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { StandardModal } from '../../../components/common/StandardModal'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { useProviders } from '../../../queries/useProvidersQueries'
import type { ProviderSummary } from '../../../types/providers'
import type { PipelineView, ViewLayout } from '../../../types/views'
import { viewAncestors } from '../../../utils/viewTree'
import { TOP_LEVEL, useViewEditorDraft, type ViewEditorDraftModel } from '../hooks/useViewEditorDraft'
import { usePipelineViews, useViewEditorOptions } from '../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { ViewEditorForm } from './ViewEditorForm'
import { ViewRulesFields } from './ViewRulesFields'

const ViewMembersFields = lazy(() => import('./ViewMembersFields').then((m) => ({ default: m.ViewMembersFields })))

interface ViewEditorExtrasProps {
  draft: ViewEditorDraftModel
  providers: ProviderSummary[]
  pending: boolean
}

function ViewEditorExtras({ draft, providers, pending }: ViewEditorExtrasProps): ReactNode {
  return (
    <>
      {draft.showRules && <ViewRulesFields rules={draft.draft.rules} providers={providers} onChange={draft.setRules} />}

      {draft.showLayout && (
        <Stack gap="3xs">
          <Text size="sm" fw={500}>
            Layout
          </Text>
          <SegmentedControl<ViewLayout>
            aria-label="Layout"
            data={[
              { value: 'table', label: 'Table' },
              { value: 'cards', label: 'Cards' },
            ]}
            value={draft.draft.display.layout}
            onChange={(layout) => draft.setDraft((d) => ({ ...d, display: { ...d.display, layout } }))}
          />
        </Stack>
      )}

      {draft.showMembers && (
        <Suspense fallback={<LoadingState variant="compact" message="Loading members…" />}>
          <ViewMembersFields
            query={draft.membersQuery}
            members={draft.members}
            disabled={pending}
            onChange={draft.setMemberIds}
          />
        </Suspense>
      )}
    </>
  )
}

export function ViewEditorModal(): ReactNode {
  const editor = usePipelineViewStore((s) => s.editor)
  const closeEditor = usePipelineViewStore((s) => s.closeEditor)
  const navigate = useNavigate()
  const openManage = usePipelineViewStore((s) => s.openManage)
  const mobileShell = useMobileShell()
  const viewsQuery = usePipelineViews()
  const views = viewsQuery.data ?? []
  const { data: providers = [] } = useProviders()
  const view = editor.viewId === null ? undefined : views.find((g) => g.id === editor.viewId)
  const optionsQuery = useViewEditorOptions(editor.viewId, true)
  const draft = useViewEditorDraft({ editor, view, mobileShell })
  const parentOptions = [
    { value: TOP_LEVEL, label: 'Top level' },
    ...(optionsQuery.data?.eligible_parents ?? []).map((v) => ({
      value: v.id,
      label: [...viewAncestors(views, v.id), v].map((n) => n.name).join(' / '),
    })),
  ]

  const onSaved = (created: PipelineView | null) => {
    closeEditor()

    if (created) {
      void navigate(`/pipelines?view=${encodeURIComponent(created.id)}`)
    }
  }

  if (editor.open && editor.viewId !== null && viewsQuery.data === undefined) {
    return (
      <StandardModal opened onClose={closeEditor} size="lg" title="Edit view">
        <QueryBoundary query={viewsQuery} loading="Loading view…" failure="Couldn't load views">
          {() => null}
        </QueryBoundary>
      </StandardModal>
    )
  }

  if (editor.open && editor.viewId !== null && !view) {
    const manageViews = () => {
      closeEditor()
      openManage()
    }

    return (
      <StandardModal opened onClose={closeEditor} size="lg" title="Edit view">
        <EmptyState
          title="This view no longer exists"
          description="It may have been deleted. Open Manage views to see what is left."
          action={<Button onClick={manageViews}>Manage views</Button>}
        />
      </StandardModal>
    )
  }

  return (
    <ViewEditorForm
      editor={editor}
      view={view}
      draft={draft}
      parentOptions={parentOptions}
      parentQuery={optionsQuery}
      onClose={closeEditor}
      onSaved={onSaved}
    >
      {(pending) => <ViewEditorExtras draft={draft} providers={providers} pending={pending} />}
    </ViewEditorForm>
  )
}
