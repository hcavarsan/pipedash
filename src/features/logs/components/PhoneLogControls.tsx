import { Loader, Menu } from '@mantine/core'
import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconChevronDown,
  IconChevronUp,
  IconDots,
  IconFilter,
  IconFilterFilled,
  IconSearch,
} from '@tabler/icons-react'
import type { ReactNode, RefObject } from 'react'
import { PhoneSearchField } from '../../../components/common/PhoneSearchField'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { LogDocumentMeta } from '../../../types/logs'
import type { LogSelection } from '../hooks/useLogNavigation'
import { errorCounter, logOptionItems } from '../lib/logActions'
import type { LogFilter } from '../lib/logRows'
import classes from './LogViewer.module.css'

interface PhoneLogSearchProps {
  queryText: string
  filtered: boolean
  onQueryChange: (value: string) => void
  onNextMatch: () => void
  inputRef: RefObject<HTMLInputElement | null>
}

export function PhoneLogSearch({
  queryText,
  filtered,
  onQueryChange,
  onNextMatch,
  inputRef,
}: PhoneLogSearchProps): ReactNode {
  return (
    <div className={classes.phoneReveal} data-search-reveal data-pinned={queryText || filtered ? true : undefined}>
      <div className={classes.phoneSearch}>
        <PhoneSearchField
          value={queryText}
          placeholder="Search logs"
          onChange={onQueryChange}
          onClear={() => onQueryChange('')}
          onSubmit={onNextMatch}
          cancelable={false}
          inputRef={inputRef}
        />
      </div>
    </div>
  )
}

interface PhoneLogBarProps {
  meta: LogDocumentMeta
  providerName: string
  selection: LogSelection | null
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  queryText: string
  matchStatus: string
  noMatches: boolean
  onPrevMatch: () => void
  onNextMatch: () => void
  wrap: boolean
  timestamps: boolean
  onWrapChange: (value: boolean) => void
  onTimestampsChange: (value: boolean) => void
  exporting: boolean
  onExport: () => void
  onOpenJob: (() => void) | null
  onPrevError: () => void
  onNextError: () => void
  onStart: () => void
  onEnd: () => void
  onSearch: () => void
}

export function PhoneLogBar({
  meta,
  providerName,
  selection,
  filter,
  onFilterChange,
  queryText,
  noMatches,
  matchStatus,
  onPrevMatch,
  onNextMatch,
  wrap,
  timestamps,
  onWrapChange,
  onTimestampsChange,
  exporting,
  onExport,
  onOpenJob,
  onPrevError,
  onNextError,
  onStart,
  onEnd,
  onSearch,
}: PhoneLogBarProps): ReactNode {
  const noErrors = meta.error_total === 0
  const failed = meta.sections.some((section) => section.status === 'failed')
  const searching = queryText !== ''
  const stepDisabled = searching ? noMatches : noErrors
  const counter = searching
    ? matchStatus
    : errorCounter(
        meta.error_lines,
        meta.error_total,
        selection && selection.from === selection.to ? selection.from : null,
        failed
      )

  const matchesOnly = filter === 'matches'
  const items = logOptionItems({
    wrap,
    onWrapChange,
    timestamps,
    onTimestampsChange,
    filter,
    onFilterChange,
    errorTotal: meta.error_total,
    exporting,
    onExport,
  })
  const toggles = items.filter((item) => item.checked !== undefined)
  const exportItem = items.find((item) => item.id === 'export')
  const toggleItems = toggles.map((item) => (
    <Menu.Item
      key={item.id}
      role="menuitemcheckbox"
      aria-checked={item.checked}
      disabled={item.disabled === true}
      rightSection={item.checked ? <IconCheck size={14} aria-hidden /> : null}
      onClick={item.onSelect}
    >
      {item.label}
    </Menu.Item>
  ))
  const exportEntry = exportItem && (
    <Menu.Item disabled={exportItem.disabled === true} onClick={exportItem.onSelect}>
      {exportItem.label}
    </Menu.Item>
  )

  return (
    <div className={classes.phoneBar}>
      <div className={classes.phoneBarGroup}>
        <button
          type="button"
          className={classes.phoneButton}
          aria-label={searching ? 'Previous match' : 'Previous error'}
          disabled={stepDisabled}
          onClick={searching ? onPrevMatch : onPrevError}
        >
          <IconChevronUp size={20} aria-hidden />
        </button>
        <button
          type="button"
          className={classes.phoneButton}
          aria-label={searching ? 'Next match' : 'Next error'}
          disabled={stepDisabled}
          onClick={searching ? onNextMatch : onNextError}
        >
          <IconChevronDown size={20} aria-hidden />
        </button>
        <span className={classes.phoneErrorCounter} aria-live={searching ? undefined : 'polite'}>
          {counter}
        </span>
      </div>
      <div className={classes.phoneBarGroup}>
        {queryText && (
          <button
            type="button"
            className={classes.phoneButton}
            aria-label={matchesOnly && matchStatus ? `Matching lines, ${matchStatus}` : 'Matching lines'}
            aria-pressed={matchesOnly}
            disabled={noMatches && !matchesOnly}
            onClick={() => onFilterChange(matchesOnly ? 'none' : 'matches')}
          >
            {matchesOnly ? <IconFilterFilled size={20} aria-hidden /> : <IconFilter size={20} aria-hidden />}
          </button>
        )}
        <button type="button" className={classes.phoneButton} aria-label="Search logs" onClick={onSearch}>
          <IconSearch size={20} aria-hidden />
        </button>
        <Menu position="top-end" withinPortal>
          <Menu.Target>
            <button
              type="button"
              className={classes.phoneButton}
              aria-label={exporting ? 'More, exporting log…' : 'More'}
              aria-busy={exporting}
            >
              {exporting ? <Loader size="xs" aria-hidden /> : <IconDots size={20} aria-hidden />}
            </button>
          </Menu.Target>
          <Menu.Dropdown>
            {toggleItems}
            <Menu.Divider />
            <Menu.Item leftSection={<IconArrowUp size={14} aria-hidden />} onClick={onStart}>
              Go to top
            </Menu.Item>
            <Menu.Item leftSection={<IconArrowDown size={14} aria-hidden />} onClick={onEnd}>
              Go to bottom
            </Menu.Item>
            <Menu.Divider />
            {exportEntry}
            {onOpenJob && <Menu.Item onClick={onOpenJob}>Open job in {providerName}</Menu.Item>}
            <ActionSheetCancel />
          </Menu.Dropdown>
        </Menu>
      </div>
    </div>
  )
}
