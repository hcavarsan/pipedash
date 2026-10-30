import { Stack } from '@mantine/core'
import { IconFolder } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosPipelineList } from '../../../components/common/IosPipelineList'
import { type PipelineActions, SectionHeading } from '../../../components/common/PipelineRowParts'
import type { ViewDisplay } from '../../../types/domain'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { MobilePipelineCards } from './MobilePipelineCards'
import { PipelineRowsTable } from './PipelineRowsTable'
import { PipelineSectionCards } from './PipelineSectionCards'

interface PipelineViewResultsProps {
  model: PipelineViewPageModel
  actions: PipelineActions
  emptyState: ReactNode
  onDisplayChange: (display: ViewDisplay) => void
}

function PipelineResultsBody({ model, actions, emptyState, onDisplayChange }: PipelineViewResultsProps): ReactNode {
  const { page, display, rows, mobileShell, kind, pinnedBlocks, isMobile, hasPinnedArea, isOpen, toggleSection } = model

  if (!page || !display) {
    return null
  }

  if (mobileShell && kind === 'pinned') {
    return rows.length === 0 && pinnedBlocks.length === 0 ? emptyState : null
  }

  if (rows.length === 0) {
    return emptyState
  }

  if (!mobileShell && hasPinnedArea && !isOpen('__sections', true)) {
    return null
  }

  if (mobileShell) {
    return (
      <IosPipelineList
        header={model.group ? undefined : model.sectionsLabel}
        rows={rows}
        getProvider={model.getProvider}
        actions={actions}
        selection={model.selection}
        stale={model.pushed}
      />
    )
  }

  if (isMobile) {
    return (
      <MobilePipelineCards
        rows={rows}
        getProvider={model.getProvider}
        onToggleRepo={toggleSection}
        actions={actions}
        selection={model.selection}
      />
    )
  }

  if (display.layout === 'cards') {
    return <PipelineSectionCards sections={page.sections} model={model} actions={actions} />
  }

  return <PipelineRowsTable model={model} actions={actions} onDisplayChange={onDisplayChange} />
}

export function PipelineViewResults(props: PipelineViewResultsProps): ReactNode {
  const { model } = props
  const { mobileShell, hasPinnedArea, rows, isOpen, toggleOpen } = model

  return (
    <Stack gap="sm" flex={1} mih={0}>
      {!mobileShell && hasPinnedArea && rows.length > 0 && (
        <SectionHeading
          icon={<IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
          label={model.sectionsLabel}
          count={model.sectionCount}
          expanded={isOpen('__sections', true)}
          onToggle={() => toggleOpen('__sections', true)}
        />
      )}
      <PipelineResultsBody {...props} />
    </Stack>
  )
}
