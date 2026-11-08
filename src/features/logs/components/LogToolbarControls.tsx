import { ActionIcon, Group, Menu, Tooltip } from '@mantine/core'
import { IconCheck, IconDots, IconDownload, IconExternalLink, IconLetterCase, IconRegex } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { shortcutLabel } from '../../../lib/shortcuts'

import type { LogFilter } from '../lib/logRows'

interface ToolbarButtonProps {
  label: string
  shortcut?: string | undefined
  icon: ReactNode
  onClick: () => void
  disabled?: boolean
  pressed?: boolean
  loading?: boolean
}

export function ToolbarButton({
  label,
  shortcut,
  icon,
  onClick,
  disabled,
  pressed,
  loading,
}: ToolbarButtonProps): ReactNode {
  const text = shortcut ? `${label} (${shortcutLabel(shortcut)})` : label

  return (
    <Tooltip label={text}>
      <ActionIcon
        variant={pressed ? 'light' : 'subtle'}
        color={pressed ? 'blue' : 'gray'}
        aria-label={text}
        loading={loading ?? false}
        {...(pressed === undefined ? {} : { 'aria-pressed': pressed })}
        {...(disabled ? { 'aria-disabled': true, 'data-disabled': true } : { onClick })}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  )
}

interface LogSearchOptionsProps {
  caseSensitive: boolean
  onCaseSensitiveChange: (value: boolean) => void
  regex: boolean
  onRegexChange: (value: boolean) => void
}

export function LogSearchOptions({
  caseSensitive,
  onCaseSensitiveChange,
  regex,
  onRegexChange,
}: LogSearchOptionsProps): ReactNode {
  return (
    <Group gap={0} wrap="nowrap">
      <ToolbarButton
        label="Match case"
        icon={<IconLetterCase size={16} aria-hidden />}
        pressed={caseSensitive}
        onClick={() => onCaseSensitiveChange(!caseSensitive)}
      />
      <ToolbarButton
        label="Use regular expression"
        icon={<IconRegex size={16} aria-hidden />}
        pressed={regex}
        onClick={() => onRegexChange(!regex)}
      />
    </Group>
  )
}

interface MenuToggle {
  key: string
  label: string
  checked: boolean
  disabled: boolean
  onToggle: () => void
}

interface LogOptionsMenuProps {
  wrap: boolean
  onWrapChange: (value: boolean) => void
  timestamps: boolean
  onTimestampsChange: (value: boolean) => void
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  errorTotal: number
  downloading: boolean
  onDownload: () => void
  jobUrl: string | null
  providerName: string
  onOpenJob: () => void
}

export function LogOptionsMenu(props: LogOptionsMenuProps): ReactNode {
  const { wrap, timestamps, filter, errorTotal } = props
  const errorsOnly = filter === 'errors'
  const toggles: MenuToggle[] = [
    { key: 'wrap', label: 'Wrap lines', checked: wrap, disabled: false, onToggle: () => props.onWrapChange(!wrap) },
    {
      key: 'timestamps',
      label: 'Show timestamps',
      checked: timestamps,
      disabled: false,
      onToggle: () => props.onTimestampsChange(!timestamps),
    },
    {
      key: 'errors',
      label: 'Show error lines only',
      checked: errorsOnly,
      disabled: errorTotal === 0 && !errorsOnly,
      onToggle: () => props.onFilterChange(errorsOnly ? 'none' : 'errors'),
    },
  ]
  const toggleItems = toggles.map((toggle) => (
    <Menu.Item
      key={toggle.key}
      role="menuitemcheckbox"
      aria-checked={toggle.checked}
      disabled={toggle.disabled}
      closeMenuOnClick={false}
      rightSection={toggle.checked ? <IconCheck size={14} aria-hidden /> : null}
      onClick={toggle.onToggle}
    >
      {toggle.label}
    </Menu.Item>
  ))

  return (
    <Menu position="bottom-end" withinPortal>
      <Menu.Target>
        <Tooltip label="Log options">
          <ActionIcon aria-label="Log options">
            <IconDots size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      </Menu.Target>
      <Menu.Dropdown>
        {toggleItems}
        <Menu.Divider />
        <Menu.Item
          leftSection={<IconDownload size={14} aria-hidden />}
          disabled={props.downloading}
          onClick={props.onDownload}
        >
          Download log
        </Menu.Item>
        {props.jobUrl && (
          <Menu.Item leftSection={<IconExternalLink size={14} aria-hidden />} onClick={props.onOpenJob}>
            Open job in {props.providerName}
          </Menu.Item>
        )}
      </Menu.Dropdown>
    </Menu>
  )
}
