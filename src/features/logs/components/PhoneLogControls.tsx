import { Group, Menu } from '@mantine/core'
import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconChevronDown,
  IconChevronUp,
  IconDots,
  IconFilter,
} from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { PhoneSearchField } from '../../../components/common/PhoneSearchField'
import { ActionSheetCancel } from '../../../components/ios/ActionSheetCancel'
import type { LogDocumentMeta } from '../../../types/logs'
import type { LogSelection } from '../hooks/useLogNavigation'
import { errorCounter } from '../lib/logActions'
import type { LogFilter } from '../lib/logRows'
import classes from './LogViewer.module.css'

interface PhoneLogSearchProps {
  queryText: string
  status: string
  noMatches: boolean
  onQueryChange: (value: string) => void
  onNextMatch: () => void
  onPrevMatch: () => void
}

export function PhoneLogSearch({
  queryText,
  status,
  noMatches,
  onQueryChange,
  onNextMatch,
  onPrevMatch,
}: PhoneLogSearchProps): ReactNode {
  return (
    <div className={classes.phoneReveal} data-search-reveal data-pinned={queryText ? true : undefined}>
      <div className={classes.phoneSearch}>
        <PhoneSearchField
          value={queryText}
          placeholder="Search logs"
          onChange={onQueryChange}
          onClear={() => onQueryChange('')}
          onSubmit={onNextMatch}
        />
      </div>
      {queryText && (
        <div className={classes.phoneMatches}>
          <span>{status}</span>
          <Group gap={0} wrap="nowrap">
            <button
              type="button"
              className={classes.phoneButton}
              aria-label="Previous match"
              disabled={noMatches}
              onClick={onPrevMatch}
            >
              <IconChevronUp size="1.2em" aria-hidden />
            </button>
            <button
              type="button"
              className={classes.phoneButton}
              aria-label="Next match"
              disabled={noMatches}
              onClick={onNextMatch}
            >
              <IconChevronDown size="1.2em" aria-hidden />
            </button>
          </Group>
        </div>
      )}
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
  noMatches: boolean
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
}

export function PhoneLogBar({
  meta,
  providerName,
  selection,
  filter,
  onFilterChange,
  queryText,
  noMatches,
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
}: PhoneLogBarProps): ReactNode {
  const noErrors = meta.error_total === 0
  const failed = meta.sections.some((section) => section.status === 'failed')

  return (
    <div className={classes.phoneBar}>
      <div className={classes.phoneBarGroup}>
        <span className={classes.phoneErrorCounter} aria-live="polite">
          {errorCounter(
            meta.error_lines,
            meta.error_total,
            selection && selection.from === selection.to ? selection.from : null,
            failed
          )}
        </span>
        <button
          type="button"
          className={classes.phoneButton}
          aria-label="Previous error"
          disabled={noErrors}
          onClick={onPrevError}
        >
          <IconChevronUp size="1.2em" aria-hidden />
        </button>
        <button
          type="button"
          className={classes.phoneButton}
          aria-label="Next error"
          disabled={noErrors}
          onClick={onNextError}
        >
          <IconChevronDown size="1.2em" aria-hidden />
        </button>
      </div>
      <div className={classes.phoneBarGroup}>
        {queryText && (
          <button
            type="button"
            className={classes.phoneButton}
            aria-label="Matching lines"
            aria-pressed={filter === 'matches'}
            disabled={noMatches && filter !== 'matches'}
            onClick={() => onFilterChange(filter === 'matches' ? 'none' : 'matches')}
          >
            <IconFilter size="1.2em" aria-hidden />
          </button>
        )}
        <Menu position="top-end" withinPortal>
          <Menu.Target>
            <button type="button" className={classes.phoneButton} aria-label="More">
              <IconDots size="1.2em" aria-hidden />
            </button>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={wrap}
              rightSection={wrap ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => onWrapChange(!wrap)}
            >
              Wrap lines
            </Menu.Item>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={timestamps}
              rightSection={timestamps ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => onTimestampsChange(!timestamps)}
            >
              Show timestamps
            </Menu.Item>
            <Menu.Item
              role="menuitemcheckbox"
              aria-checked={filter === 'errors'}
              disabled={noErrors && filter !== 'errors'}
              rightSection={filter === 'errors' ? <IconCheck size={14} aria-hidden /> : null}
              onClick={() => onFilterChange(filter === 'errors' ? 'none' : 'errors')}
            >
              Show error lines only
            </Menu.Item>
            <Menu.Divider />
            <Menu.Item leftSection={<IconArrowUp size={14} aria-hidden />} onClick={onStart}>
              Go to top
            </Menu.Item>
            <Menu.Item leftSection={<IconArrowDown size={14} aria-hidden />} onClick={onEnd}>
              Go to bottom
            </Menu.Item>
            <Menu.Divider />
            <Menu.Item disabled={exporting} onClick={onExport}>
              Export log
            </Menu.Item>
            {onOpenJob && <Menu.Item onClick={onOpenJob}>Open job in {providerName}</Menu.Item>}
            <ActionSheetCancel />
          </Menu.Dropdown>
        </Menu>
      </div>
    </div>
  )
}
