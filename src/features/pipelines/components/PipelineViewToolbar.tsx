import { Button, Group } from '@mantine/core'
import { IconCheck, IconListCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { PageHeader } from '../../../components/common/PageHeader'
import type { ViewDisplay } from '../../../types/domain'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { PipelineViewStrip } from './PipelineViewStrip'
import { PipelineViewTabs } from './PipelineViewTabs'
import { SubViewBar } from './SubViewBar'
import { ViewOptionsMenu } from './ViewOptionsMenu'

interface PipelineViewToolbarProps {
  model: PipelineViewPageModel
  onDisplayChange: (display: ViewDisplay) => void
}

export function PipelineViewOptions({ model, onDisplayChange }: PipelineViewToolbarProps): ReactNode {
  const { view, page, kind, showHidden, labels, collapsibleIds, setAll, setFilter } = model

  if (!view) {
    return null
  }

  const hiddenOption =
    page && kind !== 'pinned' && (page.hidden_count > 0 || showHidden)
      ? {
          value: showHidden,
          count: page.hidden_count,
          onChange: (value: boolean) => setFilter('hidden', value ? 'show' : null),
        }
      : undefined

  return (
    <ViewOptionsMenu
      view={view}
      groupTitle={labels.groupTitle}
      onChange={onDisplayChange}
      onExpandAll={collapsibleIds.length > 0 ? (open) => setAll(collapsibleIds, open) : undefined}
      hidden={hiddenOption}
    />
  )
}

export function PipelineViewNav({ model }: { model: PipelineViewPageModel }): ReactNode {
  return (
    <>
      {model.mobileShell ? model.query.view !== 'pinned' && <PipelineViewStrip /> : <PipelineViewTabs />}
      <SubViewBar />
    </>
  )
}

export function PipelineViewToolbar({ model, onDisplayChange }: PipelineViewToolbarProps): ReactNode {
  const { mobileShell, isFluxView, title, page, selecting, startSelection, finishSelection } = model

  if (mobileShell && !isFluxView) {
    return null
  }

  const selectButton = selecting ? (
    <Button
      size="sm"
      miw={100}
      variant="filled"
      leftSection={<IconCheck size={16} aria-hidden />}
      onClick={finishSelection}
    >
      Done
    </Button>
  ) : (
    page &&
    (page.total + page.pinned.length > 0 || page.pinned_blocks.some((block) => block.pipelines.length > 0)) && (
      <Button
        size="sm"
        miw={100}
        variant="default"
        leftSection={<IconListCheck size={16} aria-hidden />}
        onClick={startSelection}
      >
        Select
      </Button>
    )
  )

  return (
    <PageHeader
      title={title}
      actions={
        mobileShell ? undefined : (
          <Group gap="xs" wrap="nowrap">
            {selectButton}
            <PipelineViewOptions model={model} onDisplayChange={onDisplayChange} />
          </Group>
        )
      }
    />
  )
}
