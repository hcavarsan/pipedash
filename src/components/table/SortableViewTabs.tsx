import type { ReactNode } from 'react'

import type { PipelineView, ProviderSummary } from '../../types'
import { reorderSlots } from '../../utils/viewTree'
import { SortableList } from '../common/SortableList'

interface SortableViewTabsProps {
  movable: PipelineView[]
  movableIds: string[]
  providers: ProviderSummary[]
  renderTab: (view: PipelineView) => ReactNode
  renderProviderTab: (provider: ProviderSummary) => ReactNode
  onReorder: (order: string[]) => void
}

export function SortableViewTabs({
  movable,
  movableIds,
  providers,
  renderTab,
  renderProviderTab,
  onReorder,
}: SortableViewTabsProps): ReactNode {
  return (
    <SortableList
      items={movable}
      getId={(view) => view.id}
      orientation="horizontal"
      handle={false}
      onReorder={(order) => onReorder(reorderSlots(movableIds, order))}
    >
      {(view, drag) => (
        <>
          <div ref={drag.setNodeRef} style={drag.style} {...drag.listeners}>
            {renderTab(view)}
          </div>
          {view.kind === 'pinned' && providers.map(renderProviderTab)}
        </>
      )}
    </SortableList>
  )
}
