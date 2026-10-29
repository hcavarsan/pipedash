import { Button, Menu } from '@mantine/core'
import { IconAdjustmentsHorizontal } from '@tabler/icons-react'
import { Fragment, type ReactNode } from 'react'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { PipelineView, SectionMode, ViewDisplay } from '../../../types/domain'
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
    { key: 'sections' as const, label: 'Group by', options: sections },
    ...(mobileShell ? [] : [{ key: 'layout' as const, label: 'Layout', options: layouts }]),
  ]
  const displayGroups = view.capabilities.edit_display ? views : []
  const flipped = display.direction === 'asc' ? 'desc' : 'asc'
  const arrow = display.direction === 'asc' ? '↑' : '↓'
  const expandAll = mobileShell ? undefined : onExpandAll

  return (
    <>
      {displayGroups.map(({ key, label, options }, index) => (
        <Fragment key={key}>
          {index > 0 && <Menu.Divider />}
          <Menu.Label>{label}</Menu.Label>
          {options.map((option) => {
            const active = display[key] === option.value
            const toggles = active && key === 'sort' && inlineDirection

            return (
              <Menu.Item
                key={option.value}
                role="menuitemradio"
                aria-checked={active}
                {...(toggles
                  ? { 'aria-label': `${option.label}, ${display.direction === 'asc' ? 'ascending' : 'descending'}` }
                  : {})}
                onClick={() =>
                  onChange(toggles ? { ...display, direction: flipped } : { ...display, [key]: option.value })
                }
              >
                {toggles ? `${option.label} ${arrow}` : active ? `${option.label} ✓` : option.label}
              </Menu.Item>
            )
          })}
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
