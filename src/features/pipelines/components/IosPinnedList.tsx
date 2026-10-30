import { IconMenu2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosPipelineRow, relativeOrNever, staleFooter } from '../../../components/common/IosPipelineList'
import rowClasses from '../../../components/common/IosPipelineList.module.css'
import {
  type PinToggle,
  type PipelineActions,
  type PipelineSelection,
  WorkflowActionMenu,
  workflowSwipeActions,
} from '../../../components/common/PipelineRowParts'
import { IosGroup } from '../../../components/ios/IosList'
import { useNow } from '../../../hooks/useNow'
import type { ProviderSummary, ViewPipeline } from '../../../types/domain'
import { getPipelineLabels, getPipelineStatusLabel, pipelineStatusKey } from '../../../utils/pipelinePresentation'
import { reorderSlots } from '../../../utils/viewTree'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { pinnedFromBlocks } from '../lib/pipelineRows'
import { usePipelineViews, useReorderPinned, useViewMembers } from '../queries/usePipelineViewQueries'
import { SortableList } from './SortableList'

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
      hint={`View ${getPipelineLabels(pipeline.provider_type).history.toLowerCase()}`}
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

export function IosPinnedList({
  model,
  actions,
}: {
  model: PipelineViewPageModel
  actions: PipelineActions
}): ReactNode {
  const { pinned, pinnedBlocks, getProvider, selection } = model
  const now = useNow(30_000)
  const extras = pinnedFromBlocks(pinned, pinnedBlocks)
  const { data: views = [] } = usePipelineViews()
  const manual = views.find((view) => view.kind === 'pinned')?.display.sort === 'manual'
  const reorderable = Boolean(selection) && manual && pinned.length > 1
  const { data: members } = useViewMembers('pinned', reorderable)
  const { mutate: reorder } = useReorderPinned()

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
  const footer = staleFooter(
    [...pinned, ...extras.map((extra) => extra.pipeline)].map((pipeline) => pipeline.provider_id),
    getProvider,
    now
  )
  const lastId = extras.at(-1)?.pipeline.id ?? pinned.at(-1)?.id

  return (
    <IosGroup header="Pinned" footer={footer}>
      {reorderable && members ? (
        <SortableList
          items={pinned}
          getId={(pipeline) => pipeline.id}
          orientation="vertical"
          handle
          onReorder={(next) =>
            reorder(
              reorderSlots(
                members.map((member) => member.pipeline_id),
                next
              )
            )
          }
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
              {row(
                pipeline,
                undefined,
                <button
                  type="button"
                  className={rowClasses.reorder}
                  data-reorder-handle
                  {...drag.handleProps}
                  {...drag.listeners}
                  aria-label={`Reorder ${pipeline.name}`}
                >
                  <IconMenu2 size={20} aria-hidden />
                </button>
              )}
            </div>
          )}
        </SortableList>
      ) : (
        pinned.map((pipeline, index) => (
          <div
            key={pipeline.id}
            className={rowClasses.cell}
            data-first={index === 0 || undefined}
            data-last={pipeline.id === lastId || undefined}
          >
            {row(pipeline)}
          </div>
        ))
      )}
      {extras.map((extra, index) => (
        <div
          key={extra.pipeline.id}
          className={rowClasses.cell}
          data-first={pinned.length + index === 0 || undefined}
          data-last={extra.pipeline.id === lastId || undefined}
        >
          {row(extra.pipeline, {
            pinned: true,
            label: 'Unpin repository',
            onToggle: () => actions.pinItem('repository', extra.block.ref).onClick(),
          })}
        </div>
      ))}
    </IosGroup>
  )
}
