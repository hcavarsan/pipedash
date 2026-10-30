import { ActionIcon, Stack, Tooltip } from '@mantine/core'
import { IconFolder, IconPinFilled, IconPinnedOff } from '@tabler/icons-react'
import { type ReactNode, useId } from 'react'
import {
  type PipelineActions,
  type PipelineSelection,
  SectionHeading,
} from '../../../components/common/PipelineRowParts'
import type { PinnedBlock, ProviderSummary } from '../../../types/domain'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { IosPinnedList } from './IosPinnedList'
import { PipelineCardGrid } from './PipelineCardGrid'

interface PinnedRepositoryBlockProps {
  block: PinnedBlock
  expanded: boolean
  onToggle: () => void
  getProvider: (id: number) => ProviderSummary | undefined
  actions: PipelineActions
  selection?: PipelineSelection | undefined
}

function PinnedRepositoryBlock({
  block,
  expanded,
  onToggle,
  getProvider,
  actions,
  selection,
}: PinnedRepositoryBlockProps): ReactNode {
  const unpin = () => actions.pinItem(block.kind, block.ref).onClick()

  return (
    <Stack gap="sm" component="section">
      <SectionHeading
        icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
        label={block.name}
        count={block.pipelines.length}
        expanded={expanded}
        onToggle={onToggle}
        actions={
          <Tooltip label={`Unpin ${block.name}`}>
            <ActionIcon aria-label={`Unpin ${block.name}`} onClick={unpin}>
              <IconPinnedOff size={16} aria-hidden />
            </ActionIcon>
          </Tooltip>
        }
      />
      {expanded && (
        <PipelineCardGrid
          pipelines={block.pipelines}
          getProvider={getProvider}
          actions={actions}
          selection={selection}
        />
      )}
    </Stack>
  )
}

export function PipelinePinnedArea({
  model,
  actions,
}: {
  model: PipelineViewPageModel
  actions: PipelineActions
}): ReactNode {
  const pinnedTitleId = useId()
  const { pinned, pinnedBlocks, mobileShell, kind, isOpen, toggleOpen, getProvider, selection } = model

  if (mobileShell) {
    return kind === 'pinned' && <IosPinnedList model={model} actions={actions} />
  }

  return (
    <>
      {pinned.length > 0 && (
        <Stack gap="sm" component="section" aria-labelledby={pinnedTitleId}>
          <SectionHeading
            id={pinnedTitleId}
            icon={<IconPinFilled size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
            label="Pinned"
            count={pinned.length}
            expanded={isOpen('__pinned', true)}
            onToggle={() => toggleOpen('__pinned', true)}
          />
          {isOpen('__pinned', true) && (
            <PipelineCardGrid pipelines={pinned} getProvider={getProvider} actions={actions} selection={selection} />
          )}
        </Stack>
      )}

      {pinnedBlocks.map((block) => {
        const blockId = `pin:${block.kind}:${block.ref}`

        return (
          <PinnedRepositoryBlock
            key={blockId}
            block={block}
            expanded={isOpen(blockId, true)}
            onToggle={() => toggleOpen(blockId, true)}
            getProvider={getProvider}
            actions={actions}
            selection={selection}
          />
        )
      })}
    </>
  )
}
