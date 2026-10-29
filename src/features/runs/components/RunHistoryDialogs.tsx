import type { ReactNode } from 'react'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useSaveTablePreferences } from '../../../queries/useRunHistoryQueries'
import type { Pipeline } from '../../../types/domain'
import type { RunHistoryColumns } from '../hooks/useRunHistoryColumns'
import { RerunConfirmModal } from './RerunConfirmModal'
import { useRunActions } from './RunActionsContext'
import { TableCustomizationModal } from './TableCustomizationModal'

interface RunHistoryDialogsProps {
  pipeline: Pipeline | null
  columns: RunHistoryColumns
  customizationSession: number
  customizationOpened: boolean
  onCloseCustomization: () => void
  providerName: string
}

export function RunHistoryDialogs({
  pipeline,
  columns,
  customizationSession,
  customizationOpened,
  onCloseCustomization,
  providerName,
}: RunHistoryDialogsProps): ReactNode {
  const { mutateAsync: savePreferences } = useSaveTablePreferences()
  const { rerunTarget, closeRerun, confirmRerun } = useRunActions()

  const applyCustomization = async (
    columnOrder: string[],
    columnVisibility: Record<string, boolean>
  ): Promise<void> => {
    if (!pipeline?.provider_id) {
      return
    }

    await savePreferences({
      providerId: pipeline.provider_id,
      tableId: 'pipeline_runs',
      preferences: { columnOrder, columnVisibility },
    })
  }

  return (
    <>
      <ErrorBoundary scope="feature" name="Column customization">
        <TableCustomizationModal
          sessionKey={customizationSession}
          opened={customizationOpened}
          onClose={onCloseCustomization}
          columns={columns.allColumns}
          visibleColumns={columns.columns}
          currentOrder={columns.columnOrder}
          currentVisibility={columns.columnVisibility}
          onApply={applyCustomization}
        />
      </ErrorBoundary>

      <ErrorBoundary scope="feature" name="Rerun confirmation">
        <RerunConfirmModal
          run={rerunTarget}
          pipeline={pipeline ?? undefined}
          providerName={providerName}
          onClose={closeRerun}
          onConfirm={confirmRerun}
        />
      </ErrorBoundary>
    </>
  )
}
