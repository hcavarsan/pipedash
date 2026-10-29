import { Text } from '@mantine/core'
import { useCallback } from 'react'

import { logger } from '../../lib/logger'
import { useDeletePipelineView, usePipelineViews } from '../../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import type { PipelineView } from '../../types'
import { openConfirm } from '../../utils/confirm'
import { childViews } from '../../utils/viewTree'

export function useConfirmDeleteView(): (view: PipelineView) => void {
  const { mutateAsync: deleteView } = useDeletePipelineView()
  const { data: views = [] } = usePipelineViews()

  return useCallback(
    (view: PipelineView) => {
      openConfirm({
        title: 'Delete view',
        children: (
          <Text size="sm">
            {`Delete "${view.name}"? Pipelines stay in Pipedash.${childViews(views, view.id).length > 0 ? ' Its sub-views move up a level.' : ''} This action cannot be undone.`}
          </Text>
        ),
        labels: { confirm: 'Delete view', cancel: 'Cancel' },
        confirmProps: { color: 'red' },
        onConfirm: async () => {
          try {
            await deleteView(view.id)
            const { editor, closeEditor } = usePipelineViewStore.getState()

            if (editor.viewId === view.id) {
              closeEditor()
            }
          } catch (err) {
            logger.error('useConfirmDeleteView', 'Failed to delete view', err)
          }
        },
      })
    },
    [deleteView, views]
  )
}
