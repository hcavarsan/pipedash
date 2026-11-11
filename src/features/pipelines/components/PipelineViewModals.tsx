import { lazy, type ReactNode, Suspense, useState } from 'react'
import { LoadingState } from '../../../components/common/LoadingState'
import { StandardModal } from '../../../components/common/StandardModal'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { AddToViewModal } from './AddToViewModal'
import { ViewEditorModal } from './ViewEditorModal'

const ManageViewsModal = lazy(() => import('./ManageViewsModal').then((m) => ({ default: m.ManageViewsModal })))

function ManageViewsFallback({ opened }: { opened: boolean }): ReactNode {
  const closeManage = usePipelineViewStore((s) => s.closeManage)

  return (
    <StandardModal opened={opened} onClose={closeManage} size="lg" title="Manage views">
      <LoadingState message="Loading views…" />
    </StandardModal>
  )
}

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
          <Suspense fallback={<ManageViewsFallback opened={manageOpen} />}>
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
