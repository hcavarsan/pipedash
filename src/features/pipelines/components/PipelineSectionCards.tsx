import { Stack } from '@mantine/core'
import { IconFolder, IconFolders } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { FetchInfo } from '../../../components/common/PipelineFetchInfo'
import { SectionActions } from '../../../components/common/PipelineRowActions'
import { SectionHeading } from '../../../components/common/SectionHeading'
import type { PipelineActions } from '../../../types/pipelineRows'
import type { ViewSection } from '../../../types/views'
import { rowMenuItems } from '../../../utils/pipelineRowActions'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { sectionTitle } from '../lib/pipelineRows'
import { PipelineCardGrid } from './PipelineCardGrid'

interface PipelineSectionCardProps {
  section: ViewSection
  model: PipelineViewPageModel
  actions: PipelineActions
}

function PipelineSectionCard({ section, model, actions }: PipelineSectionCardProps): ReactNode {
  const { isOpen, toggleOpen, getProvider, selection } = model
  const open = section.kind === 'none' || isOpen(section.id, true)
  const name = sectionTitle(section)
  let heading: ReactNode = null

  if (section.kind !== 'none') {
    const Icon = section.kind === 'view' ? IconFolders : IconFolder
    const items = rowMenuItems(
      { id: section.id, type: 'section', name, kind: section.kind, pipelines: section.pipelines },
      actions
    )

    heading = (
      <SectionHeading
        icon={<Icon size={16} color="var(--mantine-color-dimmed)" aria-hidden />}
        label={name}
        info={
          <FetchInfo
            providers={section.provider_ids.flatMap((id) => getProvider(id) ?? [])}
            freshness={section.freshness}
          />
        }
        actions={<SectionActions name={name} providerIds={section.provider_ids} items={items} actions={actions} />}
        count={section.pipelines.length}
        expanded={open}
        onToggle={() => toggleOpen(section.id, true)}
      />
    )
  }

  return (
    <Stack gap="sm">
      {heading}
      {open && (
        <PipelineCardGrid
          pipelines={section.pipelines}
          getProvider={getProvider}
          actions={actions}
          selection={selection}
        />
      )}
    </Stack>
  )
}

interface PipelineSectionCardsProps {
  sections: ViewSection[]
  model: PipelineViewPageModel
  actions: PipelineActions
}

export function PipelineSectionCards({ sections, model, actions }: PipelineSectionCardsProps): ReactNode {
  return (
    <Stack gap="lg">
      {sections.map((section) => (
        <PipelineSectionCard key={section.id} section={section} model={model} actions={actions} />
      ))}
    </Stack>
  )
}
