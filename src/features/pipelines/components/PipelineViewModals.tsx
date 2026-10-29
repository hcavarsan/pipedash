import { lazy, type ReactNode, Suspense, useState } from 'react'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { AddToViewModal } from './AddToViewModal'
import { ViewEditorModal } from './ViewEditorModal'

const ManageViewsModal = lazy(() => import('./ManageViewsModal').then((m) => ({ default: m.ManageViewsModal })))

export function PipelineViewModals(): ReactNode {
  const manageOpen = usePipelineViewStore((s) => s.manageOpen)
  const addToViewOpen = usePipelineViewStore((s) => s.addToViewIds !== null)
  const editorOpen = usePipelineViewStore((s) => s.editor.open)
  const [manageRequested, setManageRequested] = useState(false)

  if (manageOpen && !manageRequested) {
    setManageRequested(true)
  }

  return (
    <>
      {manageRequested && (
        <ErrorBoundary scope="feature" name="Manage views" resetKeys={[manageOpen]}>
          <Suspense fallback={null}>
            <ManageViewsModal />
          </Suspense>
        </ErrorBoundary>
      )}
      <ErrorBoundary scope="feature" name="Add to view" resetKeys={[addToViewOpen]}>
        <AddToViewModal />
      </ErrorBoundary>
      <ErrorBoundary scope="feature" name="View editor" resetKeys={[editorOpen]}>
        <ViewEditorModal />
      </ErrorBoundary>
    </>
  )
}
