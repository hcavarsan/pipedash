import type { ReactNode, RefObject } from 'react'
import { useLogViewerPrefs } from '../../../stores/logViewerPrefsStore'
import type { LogDocumentMeta } from '../../../types/logs'
import type { LogExportState } from '../hooks/useLogExport'
import type { LogNavigationState } from '../hooks/useLogNavigation'
import type { LogSearchState } from '../hooks/useLogSearch'
import type { LogFilter } from '../lib/logRows'
import { LogToolbar } from './LogToolbar'
import { PhoneLogSearch } from './PhoneLogControls'

interface ReadyToolbarProps {
  variant: 'desktop' | 'narrow' | 'phone'
  searchRef: RefObject<HTMLInputElement | null>
  search: LogSearchState
  navigation: LogNavigationState
  counter: string | null
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  errorTotal: LogDocumentMeta['error_total']
  logExport: LogExportState
  jobUrl: string | null
  providerName: string
  onOpenJob: (() => void) | null
}

export function ReadyToolbar({
  variant,
  searchRef,
  search,
  navigation,
  counter,
  filter,
  onFilterChange,
  errorTotal,
  logExport,
  jobUrl,
  providerName,
  onOpenJob,
}: ReadyToolbarProps): ReactNode {
  const wrap = useLogViewerPrefs((s) => s.wrap)
  const timestamps = useLogViewerPrefs((s) => s.timestamps)
  const setWrap = useLogViewerPrefs((s) => s.setWrap)
  const setTimestamps = useLogViewerPrefs((s) => s.setTimestamps)
  const { selection, stepError } = navigation

  if (variant === 'phone') {
    return (
      <PhoneLogSearch
        queryText={search.queryText}
        onQueryChange={navigation.changeText}
        onNextMatch={navigation.nextMatch}
        inputRef={searchRef}
        filtered={filter !== 'none'}
      />
    )
  }

  return (
    <LogToolbar
      variant={variant}
      searchRef={searchRef}
      query={search.queryText}
      onQueryChange={navigation.changeText}
      regex={search.regex}
      onRegexChange={navigation.changeRegex}
      caseSensitive={search.caseSensitive}
      onCaseSensitiveChange={navigation.changeCaseSensitive}
      counter={counter}
      searchError={search.failure}
      matchTotal={search.matches.length}
      onNextMatch={navigation.nextMatch}
      onPrevMatch={navigation.prevMatch}
      filter={filter}
      onFilterChange={onFilterChange}
      errorTotal={errorTotal}
      onNextError={() => stepError(1)}
      onPrevError={() => stepError(-1)}
      wrap={wrap}
      onWrapChange={setWrap}
      timestamps={timestamps}
      onTimestampsChange={setTimestamps}
      selectionCount={selection ? selection.to - selection.from + 1 : 0}
      onCopySelection={logExport.copySelection}
      copied={logExport.copied}
      onExport={() => void logExport.exportLog()}
      exporting={logExport.exporting}
      jobUrl={jobUrl}
      providerName={providerName}
      onOpenJob={() => onOpenJob?.()}
    />
  )
}
