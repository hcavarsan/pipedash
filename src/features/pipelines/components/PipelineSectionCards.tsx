import { Stack } from '@mantine/core'
import { IconFolder, IconFolders } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import {
  FetchInfo,
  type PipelineActions,
  rowMenuItems,
  SectionActions,
  SectionHeading,
} from '../../../components/common/PipelineRowParts'
import type { ViewSection } from '../../../types/domain'
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

  return (
    <Stack gap="sm">
      {section.kind !== 'none' && (
        <SectionHeading
          icon={
            section.kind === 'view' ? (
              <IconFolders size={16} color="var(--mantine-color-dimmed)" aria-hidden />
            ) : (
              <IconFolder size={16} color="var(--mantine-color-dimmed)" aria-hidden />
            )
          }
          label={name}
          info={<FetchInfo providers={section.provider_ids.flatMap((id) => getProvider(id) ?? [])} />}
          actions={
            <SectionActions
              name={name}
              providerIds={section.provider_ids}
              items={rowMenuItems(
                { id: section.id, type: 'section', name, kind: section.kind, pipelines: section.pipelines },
                actions
              )}
              actions={actions}
            />
          }
          count={section.pipelines.length}
          expanded={open}
          onToggle={() => toggleOpen(section.id, true)}
        />
      )}
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
