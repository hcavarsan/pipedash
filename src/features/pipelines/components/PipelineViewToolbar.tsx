import { Button, Group, Menu } from '@mantine/core'
import { IconCheck, IconListCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { PageHeader } from '../../../components/common/PageHeader'
import { NavBarMenu } from '../../../components/layout/NavBarMenu'
import { useMobileTitle } from '../../../stores/mobileNavStore'
import type { PipelineViewPage, ViewDisplay } from '../../../types/domain'
import type { PipelineViewPageModel } from '../hooks/usePipelineViewModel'
import { usePipelineViewStore } from '../stores/pipelineViewStore'
import { PipelineViewTabs } from './PipelineViewTabs'
import { SubViewBar } from './SubViewBar'
import { hasViewOptions, ViewOptionsItems, ViewOptionsMenu, type ViewOptionsProps } from './ViewOptionsMenu'

interface PipelineViewToolbarProps {
  model: PipelineViewPageModel
  onDisplayChange: (display: ViewDisplay) => void
  onOpenFilters?: (() => void) | undefined
}

function viewOptions(
  { view, page, kind, showHidden, labels, collapsibleIds, setAll, setFilter }: PipelineViewPageModel,
  onDisplayChange: (display: ViewDisplay) => void
): ViewOptionsProps | null {
  if (!view) {
    return null
  }

  const hidden =
    page && kind !== 'pinned' && (page.hidden_count > 0 || showHidden)
      ? {
          value: showHidden,
          count: page.hidden_count,
          onChange: (value: boolean) => setFilter('hidden', value ? 'show' : null),
        }
      : undefined
  const options: ViewOptionsProps = {
    view,
    groupTitle: labels.groupTitle,
    onChange: onDisplayChange,
    onExpandAll: collapsibleIds.length > 0 ? (open: boolean) => setAll(collapsibleIds, open) : undefined,
    hidden,
  }

  return hasViewOptions(options) ? options : null
}

const hasSelectable = (page: PipelineViewPage): boolean =>
  page.total + page.pinned.length > 0 || page.pinned_blocks.some((block) => block.pipelines.length > 0)

function PipelineNavMenu({ model, onDisplayChange, onOpenFilters }: PipelineViewToolbarProps): ReactNode {
  const openManage = usePipelineViewStore((s) => s.openManage)
  const { page, rows, pinnedBlocks, pushed, selecting, startSelection, query } = model
  const options = viewOptions(model, onDisplayChange)
  const selectable = rows.some((row) => row.type === 'workflow') || pinnedBlocks.length > 0

  if (!page || (pushed && !selectable)) {
    return null
  }

  return (
    <NavBarMenu label="More">
      {!selecting && selectable && <Menu.Item onClick={startSelection}>Select</Menu.Item>}
      {!pushed && onOpenFilters && <Menu.Item onClick={onOpenFilters}>Filter</Menu.Item>}
      {!pushed && query.view !== 'pinned' && <Menu.Item onClick={openManage}>Edit Views</Menu.Item>}
      {!pushed && options && (
        <>
          <Menu.Divider />
          <ViewOptionsItems {...options} />
        </>
      )}
    </NavBarMenu>
  )
}

export function PipelineViewNav({ model }: { model: PipelineViewPageModel }): ReactNode {
  return (
    <>
      {!model.mobileShell && <PipelineViewTabs />}
      <SubViewBar />
    </>
  )
}

export function PipelineViewToolbar(props: PipelineViewToolbarProps): ReactNode {
  const { model, onDisplayChange } = props
  const { mobileShell, mobileTitle, title, page, selecting, startSelection, finishSelection } = model

  useMobileTitle(mobileTitle)

  if (mobileShell) {
    return <PipelineNavMenu {...props} />
  }

  const options = viewOptions(model, onDisplayChange)
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
    hasSelectable(page) && (
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
        <Group gap="xs" wrap="nowrap">
          {selectButton}
          {options && <ViewOptionsMenu {...options} />}
        </Group>
      }
    />
  )
}
