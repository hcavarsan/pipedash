import type { ReactNode } from 'react'
import type { ProviderSummary } from '../../../types/providers'
import type { PipelineView } from '../../../types/views'
import { reorderSlots } from '../../../utils/viewTree'
import { SortableList } from './SortableList'

interface SortableViewTabsProps {
  movable: PipelineView[]
  movableIds: string[]
  providers: ProviderSummary[]
  renderTab: (view: PipelineView) => ReactNode
  renderProviderTab: (provider: ProviderSummary) => ReactNode
  disabled: boolean
  onReorder: (order: string[]) => void
}

export function SortableViewTabs({
  movable,
  movableIds,
  providers,
  renderTab,
  renderProviderTab,
  disabled,
  onReorder,
}: SortableViewTabsProps): ReactNode {
  return (
    <SortableList
      items={movable}
      getId={(view) => view.id}
      orientation="horizontal"
      handle={false}
      disabled={disabled}
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
