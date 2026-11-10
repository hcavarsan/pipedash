import { ActionIcon, Group, Menu, Tooltip } from '@mantine/core'
import { IconCheck, IconDots, IconDownload, IconExternalLink, IconLetterCase, IconRegex } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { shortcutLabel } from '../../../lib/shortcuts'

import { type LogOptionsState, logOptionItems } from '../lib/logActions'

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

interface LogOptionsMenuProps extends LogOptionsState {
  jobUrl: string | null
  providerName: string
  onOpenJob: () => void
}

export function LogOptionsMenu(props: LogOptionsMenuProps): ReactNode {
  const items = logOptionItems(props)
  const toggles = items.filter((item) => item.checked !== undefined)
  const exportItem = items.find((item) => item.id === 'export')

  return (
    <Menu position="bottom-end" withinPortal>
      <Menu.Target>
        <Tooltip label="Log options">
          <ActionIcon aria-label="Log options" loading={props.exporting}>
            <IconDots size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      </Menu.Target>
      <Menu.Dropdown>
        {toggles.map((item) => (
          <Menu.Item
            key={item.id}
            role="menuitemcheckbox"
            aria-checked={item.checked}
            disabled={item.disabled === true}
            closeMenuOnClick={false}
            rightSection={item.checked ? <IconCheck size={14} aria-hidden /> : null}
            onClick={item.onSelect}
          >
            {item.label}
          </Menu.Item>
        ))}
        <Menu.Divider />
        {exportItem && (
          <Menu.Item
            leftSection={<IconDownload size={14} aria-hidden />}
            disabled={exportItem.disabled === true}
            onClick={exportItem.onSelect}
          >
            {exportItem.label}
          </Menu.Item>
        )}
        {props.jobUrl && (
          <Menu.Item leftSection={<IconExternalLink size={14} aria-hidden />} onClick={props.onOpenJob}>
            Open job in {props.providerName}
          </Menu.Item>
        )}
      </Menu.Dropdown>
    </Menu>
  )
}
