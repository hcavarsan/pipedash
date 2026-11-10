import { IconMenu2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosPipelineRow, relativeOrNever } from '../../../components/common/IosPipelineList'
import rowClasses from '../../../components/common/IosPipelineList.module.css'
import { WorkflowActionMenu } from '../../../components/common/PipelineRowActions'
import { VirtualList } from '../../../components/common/VirtualList'
import { IosGroup } from '../../../components/ios/IosList'
import { useNow } from '../../../hooks/useNow'
import type { PinToggle, PipelineActions, PipelineSelection } from '../../../types/pipelineRows'
import type { ProviderSummary } from '../../../types/providers'
import type { ViewPipeline } from '../../../types/views'
import { getPipelineStatusLabel, pipelineStatusKey } from '../../../utils/pipelinePresentation'
import { historyLabel, workflowSwipeActions } from '../../../utils/pipelineRowActions'
import { reorderSlots } from '../../../utils/viewTree'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { inMemberOrder, pinnedFromBlocks } from '../lib/pipelineRows'
import { useViewMembers } from '../queries/usePipelineViewQueries'
import { useReorderPinned } from '../queries/useViewReorderQueries'
import { type SortableDragProps, SortableList } from './SortableList'

const ROW_ESTIMATE = 64

interface PinnedEntry {
  pipeline: ViewPipeline
  pin?: PinToggle
}

interface PinnedRowProps {
  pipeline: ViewPipeline
  getProvider: (providerId: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection: PipelineSelection | undefined
  now: number
  pin?: PinToggle | undefined
  reorderHandle?: ReactNode
}

function PinnedRow({ pipeline, getProvider, actions, selection, now, pin, reorderHandle }: PinnedRowProps): ReactNode {
  const subtitle = [pipeline.repository, pipeline.branch, relativeOrNever(pipeline.last_run, now)]
    .filter(Boolean)
    .join(' · ')

  return (
    <IosPipelineRow
      status={pipelineStatusKey(pipeline)}
      statusText={getPipelineStatusLabel(pipeline)}
      title={pipeline.name}
      subtitle={subtitle}
      hint={`View ${historyLabel(actions, pipeline.provider_type)}`}
      longPressMenu
      selected={selection ? selection.selected.has(pipeline.id) : undefined}
      onClick={() => (selection ? selection.onToggle(pipeline) : actions.onViewHistory(pipeline))}
      swipeActions={workflowSwipeActions(pipeline, actions, pin)}
      menu={
        <WorkflowActionMenu
          pipeline={pipeline}
          provider={getProvider(pipeline.provider_id)}
          actions={actions}
          pin={pin}
        />
      }
      reorderHandle={reorderHandle}
    />
  )
}

function ReorderHandle({ name, drag }: { name: string; drag: SortableDragProps }): ReactNode {
  return (
    <button
      type="button"
      className={rowClasses.reorder}
      data-reorder-handle
      {...drag.handleProps}
      {...drag.listeners}
      aria-label={`Reorder ${name}`}
    >
      <IconMenu2 size={20} aria-hidden />
    </button>
  )
}

export function IosPinnedList({
  model,
  actions,
}: {
  model: PipelineViewPageModel
  actions: PipelineActions
}): ReactNode {
  const { page, view, pinnedBlocks, getProvider, selection } = model
  const now = useNow(30_000)
  const manual = view?.display.sort === 'manual'
  const { data: members } = useViewMembers('pinned', manual)
  const pinned = inMemberOrder(page?.sections ?? [], manual ? members : undefined)
  const extras = pinnedFromBlocks(pinned, pinnedBlocks)
  const reorderable = Boolean(selection) && manual && pinned.length > 1
  const reorder = useReorderPinned()

  if (pinned.length + extras.length === 0) {
    return null
  }

  const row = (pipeline: ViewPipeline, pin?: PinToggle, reorderHandle?: ReactNode) => (
    <PinnedRow
      pipeline={pipeline}
      getProvider={getProvider}
      actions={actions}
      selection={selection}
      now={now}
      pin={pin}
      reorderHandle={reorderHandle}
    />
  )
  const entries: PinnedEntry[] = [
    ...pinned.map((pipeline) => ({ pipeline })),
    ...extras.map(({ pipeline, block }) => {
      const item = actions.pinItem('repository', block.ref)

      return {
        pipeline,
        pin: { pinned: true, label: 'Unpin repository', pending: item.disabled ?? false, onToggle: item.onClick },
      }
    }),
  ]
  const sorting = reorderable && members
  const reorderPinned = (next: string[]) =>
    reorder.mutate(
      reorderSlots(
        (members ?? []).map((member) => member.pipeline_id),
        next
      )
    )
  const offset = sorting ? pinned.length : 0
  const lastId = entries.at(-1)?.pipeline.id

  return (
    <IosGroup>
      {sorting && (
        <SortableList
          items={pinned}
          getId={(pipeline) => pipeline.id}
          orientation="vertical"
          handle
          disabled={reorder.isPending}
          onReorder={reorderPinned}
        >
          {(pipeline, drag) => (
            <div
              ref={drag.setNodeRef}
              style={drag.style}
              className={rowClasses.cell}
              data-first={pipeline.id === pinned[0]?.id || undefined}
              data-last={pipeline.id === lastId || undefined}
              data-dragging={drag.isDragging || undefined}
            >
              {row(pipeline, undefined, <ReorderHandle name={pipeline.name} drag={drag} />)}
            </div>
          )}
        </SortableList>
      )}
      <VirtualList
        items={entries.slice(offset)}
        getKey={(entry) => entry.pipeline.id}
        estimateSize={ROW_ESTIMATE}
        renderItem={(entry, index) => (
          <div
            className={rowClasses.cell}
            data-first={offset + index === 0 || undefined}
            data-last={entry.pipeline.id === lastId || undefined}
          >
            {row(entry.pipeline, entry.pin)}
          </div>
        )}
      />
    </IosGroup>
  )
}
