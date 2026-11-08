import { Button, Menu } from '@mantine/core'
import { IconAdjustmentsHorizontal } from '@tabler/icons-react'
import { Fragment, type ReactNode } from 'react'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { PipelineView, SectionMode, ViewDisplay } from '../../../types/views'
import { typedKeys } from '../../../utils/typedKeys'

export interface ViewOptionsProps {
  view: PipelineView
  groupTitle: string
  onChange: (display: ViewDisplay) => void
  onExpandAll?: ((open: boolean) => void) | undefined
  hidden?: { value: boolean; count: number; onChange: (value: boolean) => void } | undefined
}

interface Option<K extends keyof ViewDisplay> {
  value: ViewDisplay[K]
  label: string
}

export const hasViewOptions = ({ view, onExpandAll, hidden }: ViewOptionsProps): boolean =>
  view.capabilities.edit_display || Boolean(hidden) || Boolean(onExpandAll)

type DisplayKey = 'sort' | 'direction' | 'sections' | 'layout'

interface OptionItemsInput {
  groupKey: DisplayKey
  options: ReadonlyArray<{ value: string; label: string }>
  display: ViewDisplay
  inlineDirection: boolean
  onChange: (display: ViewDisplay) => void
}

function optionItems({ groupKey, options, display, inlineDirection, onChange }: OptionItemsInput): ReactNode[] {
  const ascending = display.direction === 'asc'

  return options.map((option) => {
    const active = display[groupKey] === option.value
    const toggles = active && groupKey === 'sort' && inlineDirection
    let text = option.label
    let next: ViewDisplay = { ...display, [groupKey]: option.value }

    if (toggles) {
      text = `${option.label} ${ascending ? '↑' : '↓'}`
      next = { ...display, direction: ascending ? 'desc' : 'asc' }
    } else if (active) {
      text = `${option.label} ✓`
    }

    return (
      <Menu.Item
        key={option.value}
        role="menuitemradio"
        aria-checked={active}
        {...(toggles ? { 'aria-label': `${option.label}, ${ascending ? 'ascending' : 'descending'}` } : {})}
        onClick={() => onChange(next)}
      >
        {text}
      </Menu.Item>
    )
  })
}

export function ViewOptionsItems({ view, groupTitle, onChange, onExpandAll, hidden }: ViewOptionsProps): ReactNode {
  const mobileShell = useMobileShell()
  const { display } = view
  const sorts: Option<'sort'>[] = [
    { value: 'manual', label: 'Manual' },
    { value: 'name', label: 'Name' },
    { value: 'last_run', label: 'Last Run' },
    { value: 'status', label: 'Status' },
    { value: 'organization', label: groupTitle },
    { value: 'provider', label: 'Provider' },
  ]
  const lastRun = display.sort === 'last_run'
  const directions: Option<'direction'>[] = [
    { value: 'asc', label: lastRun ? 'Oldest First' : 'Ascending' },
    { value: 'desc', label: lastRun ? 'Newest First' : 'Descending' },
  ]
  const sectionLabels: Record<SectionMode, string> = {
    repository: 'Repository',
    organization: groupTitle,
    provider: 'Provider',
    status: 'Status',
    none: 'None',
  }
  const sections = typedKeys(sectionLabels).map((value) => ({
    value,
    label: sectionLabels[value],
  }))
  const layouts: Option<'layout'>[] = [
    { value: 'table', label: 'Table' },
    { value: 'cards', label: 'Cards' },
  ]
  const inlineDirection = mobileShell && display.sort !== 'manual'
  const views = [
    { key: 'sort' as const, label: 'Sort by', options: sorts },
    ...(display.sort === 'manual' || mobileShell
      ? []
      : [{ key: 'direction' as const, label: 'Order', options: directions }]),
    ...(mobileShell && view.kind === 'pinned'
      ? []
      : [{ key: 'sections' as const, label: 'Group by', options: sections }]),
    ...(mobileShell ? [] : [{ key: 'layout' as const, label: 'Layout', options: layouts }]),
  ]
  const displayGroups = view.capabilities.edit_display ? views : []
  const expandAll = mobileShell ? undefined : onExpandAll

  return (
    <>
      {displayGroups.map(({ key, label, options }, index) => (
        <Fragment key={key}>
          {index > 0 && <Menu.Divider />}
          <Menu.Label>{label}</Menu.Label>
          {optionItems({ groupKey: key, options, display, inlineDirection, onChange })}
        </Fragment>
      ))}
      {hidden && (
        <>
          {displayGroups.length > 0 && <Menu.Divider />}
          <Menu.Item role="menuitemcheckbox" aria-checked={hidden.value} onClick={() => hidden.onChange(!hidden.value)}>
            {`Show hidden pipelines${hidden.count > 0 ? ` (${hidden.count})` : ''}${hidden.value ? ' ✓' : ''}`}
          </Menu.Item>
        </>
      )}
      {expandAll && (
        <>
          {(displayGroups.length > 0 || hidden) && <Menu.Divider />}
          <Menu.Item onClick={() => expandAll(true)}>Expand all</Menu.Item>
          <Menu.Item onClick={() => expandAll(false)}>Collapse all</Menu.Item>
        </>
      )}
    </>
  )
}

export function ViewOptionsMenu(props: ViewOptionsProps): ReactNode {
  if (!hasViewOptions(props)) {
    return null
  }

  return (
    <Menu position="bottom-end">
      <Menu.Target>
        <Button variant="default" size="sm" leftSection={<IconAdjustmentsHorizontal size={16} aria-hidden />}>
          View options
        </Button>
      </Menu.Target>
      <Menu.Dropdown>
        <ViewOptionsItems {...props} />
      </Menu.Dropdown>
    </Menu>
  )
}
