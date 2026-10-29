import { Stack } from '@mantine/core'
import { IconFolder } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import type { ViewDisplay } from '../../types'

import { IosPipelineList } from './IosPipelineList'
import { MobilePipelineCards } from './MobilePipelineCards'
import { type PipelineActions, SectionHeading } from './PipelineRowParts'
import { PipelineRowsTable } from './PipelineRowsTable'
import { PipelineSectionCards } from './PipelineSectionCards'
import { PipelineViewOptions } from './PipelineViewToolbar'
import type { PipelineViewPageModel } from './usePipelineViewModel'

interface PipelineViewResultsProps {
  model: PipelineViewPageModel
  actions: PipelineActions
  emptyState: ReactNode
  onDisplayChange: (display: ViewDisplay) => void
}

function PipelineResultsBody({ model, actions, emptyState, onDisplayChange }: PipelineViewResultsProps): ReactNode {
  const { page, display, rows, mobileShell, isMobile, hasPinnedArea, isOpen, toggleSection } = model

  if (!page || !display) {
    return null
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
        header={model.sectionsLabel}
        headerAction={<PipelineViewOptions model={model} onDisplayChange={onDisplayChange} />}
        rows={rows}
        getProvider={model.getProvider}
        onToggleRepo={toggleSection}
        actions={actions}
        selection={model.selection}
        onSelect={model.selecting ? undefined : model.startSelection}
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
