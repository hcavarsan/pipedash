import { Button, Menu, UnstyledButton } from '@mantine/core'
import { IconAdjustmentsHorizontal, IconArrowDown, IconArrowUp } from '@tabler/icons-react'
import { Fragment } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import type { PipelineView, SectionMode, ViewDisplay } from '../../types'
import { ActionSheetCancel } from '../ios/ActionSheetCancel'

import classes from './IosPipelineList.module.css'

interface ViewOptionsMenuProps {
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

export function ViewOptionsMenu({ view, groupTitle, onChange, onExpandAll, hidden }: ViewOptionsMenuProps) {
  const mobileShell = useMobileShell()
  const { display } = view

  if (!view.capabilities.edit_display && !hidden && !onExpandAll) {
    return null
  }

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
  const sections = (Object.keys(sectionLabels) as SectionMode[]).map((value) => ({
    value,
    label: sectionLabels[value],
  }))
  const layouts: Option<'layout'>[] = [
    { value: 'table', label: 'Table' },
    { value: 'cards', label: 'Cards' },
  ]
  const views = [
    { key: 'sort' as const, label: 'Sort by', options: sorts },
    ...(display.sort === 'manual' ? [] : [{ key: 'direction' as const, label: 'Order', options: directions }]),
    { key: 'sections' as const, label: 'Group by', options: sections },
    ...(mobileShell ? [] : [{ key: 'layout' as const, label: 'Layout', options: layouts }]),
  ]
  const displayGroups = view.capabilities.edit_display ? views : []
  const currentSort: Option<'sort'> = sorts.find((option) => option.value === display.sort) ?? {
    value: 'name',
    label: 'Name',
  }
  const ascending = display.direction === 'asc'
  const DirectionIcon = ascending ? IconArrowUp : IconArrowDown

  return (
    <Menu position="bottom-end">
      <Menu.Target>
        {mobileShell ? (
          <UnstyledButton
            className={classes.sort}
            aria-label={`View options, sorted by ${currentSort.label}${display.sort === 'manual' ? '' : `, ${ascending ? 'ascending' : 'descending'}`}`}
          >
            {currentSort.label}
            {display.sort !== 'manual' && <DirectionIcon size={14} aria-hidden />}
          </UnstyledButton>
        ) : (
          <Button variant="default" size="sm" leftSection={<IconAdjustmentsHorizontal size={16} />}>
            View options
          </Button>
        )}
      </Menu.Target>
      <Menu.Dropdown>
        {displayGroups.map(({ key, label, options }, index) => (
          <Fragment key={key}>
            {index > 0 && <Menu.Divider />}
            <Menu.Label>{label}</Menu.Label>
            {options.map((option) => {
              const active = display[key] === option.value

              return (
                <Menu.Item
                  key={option.value}
                  role="menuitemradio"
                  aria-checked={active}
                  onClick={() => onChange({ ...display, [key]: option.value })}
                >
                  {active ? `${option.label} ✓` : option.label}
                </Menu.Item>
              )
            })}
          </Fragment>
        ))}
        {hidden && (
          <>
            {displayGroups.length > 0 && <Menu.Divider />}
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={hidden.value}
              onClick={() => hidden.onChange(!hidden.value)}
            >
              {`Show hidden pipelines${hidden.count > 0 ? ` (${hidden.count})` : ''}${hidden.value ? ' ✓' : ''}`}
            </Menu.Item>
          </>
        )}
        {onExpandAll && (
          <>
            {(displayGroups.length > 0 || hidden) && <Menu.Divider />}
            <Menu.Item onClick={() => onExpandAll(true)}>Expand all</Menu.Item>
            <Menu.Item onClick={() => onExpandAll(false)}>Collapse all</Menu.Item>
          </>
        )}
        <ActionSheetCancel />
      </Menu.Dropdown>
    </Menu>
  )
}
