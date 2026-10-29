import { ActionIcon, Group, Menu, Text, TextInput, Tooltip } from '@mantine/core'
import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconChevronDown,
  IconChevronUp,
  IconCircleX,
  IconCopy,
  IconDots,
  IconDownload,
  IconExternalLink,
  IconFilter,
  IconLetterCase,
  IconRegex,
  IconSearch,
} from '@tabler/icons-react'
import type { KeyboardEvent, ReactNode, Ref } from 'react'

import { shortcutLabel } from '../../lib/shortcuts'

import type { LogFilter } from './logRows'

export interface LogToolbarProps {
  variant: 'desktop' | 'narrow'
  searchRef: Ref<HTMLInputElement>
  query: string
  onQueryChange: (value: string) => void
  regex: boolean
  onRegexChange: (value: boolean) => void
  caseSensitive: boolean
  onCaseSensitiveChange: (value: boolean) => void
  counter: string | null
  searchError: string | null
  matchTotal: number
  onNextMatch: () => void
  onPrevMatch: () => void
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  errorTotal: number
  onNextError: () => void
  onPrevError: () => void
  wrap: boolean
  onWrapChange: (value: boolean) => void
  timestamps: boolean
  onTimestampsChange: (value: boolean) => void
  selectionCount: number
  onCopySelection: () => void
  copied: boolean
  onDownload: () => void
  downloading: boolean
  jobUrl: string | null
  providerName: string
  onOpenJob: () => void
}

interface ToolbarButtonProps {
  label: string
  shortcut?: string | undefined
  icon: ReactNode
  onClick: () => void
  disabled?: boolean
  pressed?: boolean
  loading?: boolean
}

function ToolbarButton({ label, shortcut, icon, onClick, disabled, pressed, loading }: ToolbarButtonProps) {
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

export function LogToolbar(props: LogToolbarProps): ReactNode {
  const { query, searchError, counter, filter } = props

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return
    }

    if (event.key === 'Enter') {
      event.preventDefault()

      if (event.shiftKey) {
        props.onPrevMatch()
      } else {
        props.onNextMatch()
      }
    } else if (event.key === 'Escape' && query) {
      event.stopPropagation()
      props.onQueryChange('')
    }
  }

  const noMatches = props.matchTotal === 0
  const noErrors = props.errorTotal === 0

  return (
    <Group gap="xs" wrap="wrap" px="sm" py="xs" justify="space-between" style={{ flex: 'none' }}>
      <Group gap="3xs" wrap="nowrap" style={{ flex: props.variant === 'desktop' ? '0 1 460px' : '1 1 100%' }}>
        <TextInput
          ref={props.searchRef}
          size="xs"
          style={{ flex: 1, minWidth: 0 }}
          aria-label="Search logs"
          placeholder="Search logs"
          maxLength={1000}
          value={query}
          error={searchError || false}
          leftSection={<IconSearch size={14} aria-hidden />}
          rightSectionWidth={56}
          rightSection={
            <Group gap={0} wrap="nowrap">
              <ToolbarButton
                label="Match case"
                icon={<IconLetterCase size={16} aria-hidden />}
                pressed={props.caseSensitive}
                onClick={() => props.onCaseSensitiveChange(!props.caseSensitive)}
              />
              <ToolbarButton
                label="Use regular expression"
                icon={<IconRegex size={16} aria-hidden />}
                pressed={props.regex}
                onClick={() => props.onRegexChange(!props.regex)}
              />
            </Group>
          }
          onChange={(event) => props.onQueryChange(event.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        {counter && (
          <Text size="xs" c="dimmed" style={{ whiteSpace: 'nowrap' }}>
            {counter}
          </Text>
        )}
        <ToolbarButton
          label="Previous match"
          shortcut="shift+mod+G"
          icon={<IconChevronUp size={16} aria-hidden />}
          disabled={noMatches}
          onClick={props.onPrevMatch}
        />
        <ToolbarButton
          label="Next match"
          shortcut="mod+G"
          icon={<IconChevronDown size={16} aria-hidden />}
          disabled={noMatches}
          onClick={props.onNextMatch}
        />
        <ToolbarButton
          label="Show matching lines only"
          icon={<IconFilter size={16} aria-hidden />}
          pressed={filter === 'matches'}
          disabled={noMatches && filter !== 'matches'}
          onClick={() => props.onFilterChange(filter === 'matches' ? 'none' : 'matches')}
        />
      </Group>
      <Group gap="3xs" wrap="nowrap">
        <Group gap="3xs" wrap="nowrap" aria-label="Errors" role="group">
          <IconCircleX size={16} color="var(--mantine-color-red-text)" aria-hidden />
          <Text size="xs" c="dimmed" style={{ fontVariantNumeric: 'tabular-nums' }}>
            {props.errorTotal}
          </Text>
          <ToolbarButton
            label={noErrors ? 'No error lines found' : 'Previous error'}
            shortcut={noErrors ? undefined : 'shift+F8'}
            icon={<IconArrowUp size={16} aria-hidden />}
            disabled={noErrors}
            onClick={props.onPrevError}
          />
          <ToolbarButton
            label={noErrors ? 'No error lines found' : 'Next error'}
            shortcut={noErrors ? undefined : 'F8'}
            icon={<IconArrowDown size={16} aria-hidden />}
            disabled={noErrors}
            onClick={props.onNextError}
          />
        </Group>
        {props.selectionCount > 0 && (
          <ToolbarButton
            label={
              props.copied ? 'Copied' : `Copy ${props.selectionCount} ${props.selectionCount === 1 ? 'line' : 'lines'}`
            }
            icon={props.copied ? <IconCheck size={16} aria-hidden /> : <IconCopy size={16} aria-hidden />}
            onClick={props.onCopySelection}
          />
        )}
        <Menu position="bottom-end" withinPortal>
          <Menu.Target>
            <Tooltip label="Log options">
              <ActionIcon aria-label="Log options">
                <IconDots size={16} aria-hidden />
              </ActionIcon>
            </Tooltip>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={props.wrap}
              closeMenuOnClick={false}
              rightSection={props.wrap ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => props.onWrapChange(!props.wrap)}
            >
              Wrap lines
            </Menu.Item>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={props.timestamps}
              closeMenuOnClick={false}
              rightSection={props.timestamps ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => props.onTimestampsChange(!props.timestamps)}
            >
              Show timestamps
            </Menu.Item>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={filter === 'errors'}
              disabled={noErrors && filter !== 'errors'}
              closeMenuOnClick={false}
              rightSection={filter === 'errors' ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => props.onFilterChange(filter === 'errors' ? 'none' : 'errors')}
            >
              Show error lines only
            </Menu.Item>
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
      </Group>
    </Group>
  )
}
