import { SegmentedControl, Stack, Text } from '@mantine/core'
import { lazy, type ReactNode, Suspense } from 'react'
import { useNavigate } from 'react-router-dom'

import { useMobileShell } from '../../hooks/useIsMobile'
import { usePipelineViews } from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import type { PipelineView, ViewLayout } from '../../types'
import { eligibleViewParents, viewAncestors } from '../../utils/viewTree'
import { LoadingState } from '../common/LoadingState'
import { TOP_LEVEL, useViewEditorDraft } from './useViewEditorDraft'
import { ViewEditorForm } from './ViewEditorForm'
import { ViewRulesFields } from './ViewRulesFields'

const ViewMembersFields = lazy(() => import('./ViewMembersFields').then((m) => ({ default: m.ViewMembersFields })))

export function ViewEditorModal(): ReactNode {
  const editor = usePipelineViewStore((s) => s.editor)
  const closeEditor = usePipelineViewStore((s) => s.closeEditor)
  const navigate = useNavigate()
  const mobileShell = useMobileShell()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const view = editor.viewId === null ? undefined : views.find((g) => g.id === editor.viewId)
  const draft = useViewEditorDraft({ editor, view, mobileShell })
  const parentOptions = [
    { value: TOP_LEVEL, label: 'Top level' },
    ...eligibleViewParents(views, editor.viewId).map((v) => ({
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

  return (
    <ViewEditorForm
      editor={editor}
      view={view}
      draft={draft}
      parentOptions={parentOptions}
      onClose={closeEditor}
      onSaved={onSaved}
    >
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
        <Suspense fallback={<LoadingState variant="compact" message="Loading members..." />}>
          <ViewMembersFields members={draft.members} onChange={draft.setMemberIds} />
        </Suspense>
      )}
    </ViewEditorForm>
  )
}
