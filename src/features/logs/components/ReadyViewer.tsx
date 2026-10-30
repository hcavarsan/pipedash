import { Alert, UnstyledButton, VisuallyHidden } from '@mantine/core'
import { IconArrowDown, IconCopy } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type TouchEvent,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react'
import { type ContextMenuItem, useContextMenu } from '../../../components/common/ContextMenu'
import { PRESENT_EASING, prefersReducedMotion } from '../../../lib/motion'
import { queryKeys } from '../../../lib/queryKeys'
import { useLogViewerPrefs } from '../../../stores/logViewerPrefsStore'
import type { LogDocumentMeta } from '../../../types/logs'
import { openExternal } from '../../../utils/openExternal'
import { useLiveTail } from '../hooks/useLiveTail'
import { useLogExport } from '../hooks/useLogExport'
import { useLogNavigation } from '../hooks/useLogNavigation'
import { useLogRows } from '../hooks/useLogRows'
import { useLogSearch } from '../hooks/useLogSearch'
import { type DocRef, logLineCache, useLogLines } from '../lib/lineCache'
import type { LogFilter } from '../lib/logRows'
import { sectionAtLine } from '../lib/logRows'
import type { LogBodyHandle } from '../lib/logWindowTypes'
import { FullLineModal } from './FullLineModal'
import { LiveMarker } from './LiveMarker'
import { LogBody } from './LogBody'
import { LogOverviewRuler, type LogRulerHandle } from './LogOverviewRuler'
import { LogToolbar } from './LogToolbar'
import type { LogViewerProps } from './LogViewer'
import classes from './LogViewer.module.css'
import { PhoneLogBar, PhoneLogSearch } from './PhoneLogControls'

const NO_GROUPS = new Map()
const SEARCH_SLIDE_MS = 200
const SWIPE_HIDE_PX = 12

const lineAt = (target: EventTarget | null): number | null => {
  const value = target instanceof HTMLElement ? target.closest<HTMLElement>('[data-line]')?.dataset.line : undefined

  return value === undefined ? null : Number(value)
}

function slideSearch(input: HTMLInputElement | null, open: boolean): void {
  const reveal = input?.closest<HTMLElement>('[data-search-reveal]')

  if (!input || !reveal) {
    return
  }

  const from = reveal.offsetHeight

  if (open) {
    reveal.setAttribute('data-open', '')
    input.focus({ preventScroll: true })
  } else if (from > 0) {
    input.blur()
    reveal.removeAttribute('data-open')
  }

  const to = reveal.offsetHeight

  if (from !== to && !prefersReducedMotion()) {
    reveal.animate([{ height: `${from}px` }, { height: `${to}px` }], {
      duration: SEARCH_SLIDE_MS,
      easing: PRESENT_EASING,
    })
  }
}

type ReadyViewerProps = LogViewerProps & { meta: LogDocumentMeta }

export function ReadyViewer({ ref, ...props }: ReadyViewerProps): ReactNode {
  const { pipelineId, pipelineName, runNumber, node, variant, providerName, meta } = props
  const phone = variant === 'phone'
  const queryClient = useQueryClient()
  const contextMenu = useContextMenu()
  const wrap = useLogViewerPrefs((s) => s.wrap)
  const timestamps = useLogViewerPrefs((s) => s.timestamps)
  const setWrap = useLogViewerPrefs((s) => s.setWrap)
  const setTimestamps = useLogViewerPrefs((s) => s.setTimestamps)
  const bodyRef = useRef<LogBodyHandle>(null)
  const rulerRef = useRef<LogRulerHandle>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const swipeStartY = useRef<number | null>(null)
  const contextLine = useRef<number | null>(null)
  const [filter, setFilter] = useState<LogFilter>('none')
  const [fullLine, setFullLine] = useState<number | null>(null)
  const [visibleLines, setVisibleLines] = useState<number[]>([])

  const doc: DocRef = {
    pipelineId,
    runNumber,
    nodeId: node.id,
    revision: meta.revision,
    lineCount: meta.line_count,
    previousRevision: meta.previous_revision,
    stableLines: meta.stable_lines,
  }
  const getLine = useLogLines(doc, visibleLines)
  const search = useLogSearch({ pipelineId, runNumber, nodeId: node.id, revision: meta.revision })
  const rows = useLogRows({ meta, filter, matchLines: search.matchLines })
  const navigation = useLogNavigation({
    meta,
    model: rows.model,
    filter,
    onFilterChange: setFilter,
    search,
    reveal: rows.reveal,
    bodyRef,
    rulerRef,
  })
  const { selection, currentIdx, nextMatch, prevMatch, stepError, goToLine, onViewport: navigateViewport } = navigation
  const { behind, freshFrom, onViewport: trackViewport } = useLiveTail(meta.revision, meta.line_count, rows.model.count)
  const { copyRange, copySelection, exportLog, exporting, copied } = useLogExport({
    doc,
    pipelineName,
    nodeName: node.name,
    selection,
  })

  const onFullLine = useCallback((line: number) => setFullLine(line), [])
  const onSeek = useCallback((line: number) => goToLine(line, 'start'), [goToLine])
  const onViewport = useCallback(
    (first: number, last: number) => {
      navigateViewport(first, last)
      trackViewport(last)
    },
    [navigateViewport, trackViewport]
  )

  useEffect(
    () =>
      logLineCache.onRevisionGone((gone) => {
        if (gone.pipelineId === pipelineId && gone.runNumber === runNumber && gone.nodeId === node.id) {
          void queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.doc(pipelineId, runNumber, node.id) })
        }
      }),
    [pipelineId, runNumber, node.id, queryClient]
  )

  const menuHandlers = contextMenu(() => {
    const line = contextLine.current

    if (line === null) {
      return []
    }

    const data = getLine(line)
    const step = sectionAtLine(meta, line)
    const items: Array<ContextMenuItem | false> = [
      {
        label: 'Copy line',
        icon: <IconCopy size={14} aria-hidden />,
        onClick: () => void copyRange(line, line + 1, "Couldn't copy the line"),
      },
      !!step && {
        label: 'Copy step output',
        icon: <IconCopy size={14} aria-hidden />,
        onClick: () => void copyRange(step.start_line, step.end_line, "Couldn't copy the step"),
      },
      typeof data === 'object' && data.omitted > 0 && { label: 'Show full line', onClick: () => setFullLine(line) },
    ]

    return items.filter((item) => item !== false)
  })
  const rowEvents = {
    onContextMenu: (event: MouseEvent<HTMLDivElement>) => {
      contextLine.current = lineAt(event.target)
      menuHandlers.onContextMenu(event)
    },
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
      contextLine.current = lineAt(event.target)
      menuHandlers.onKeyDown(event)
    },
  }

  const onBodyKeyDown = (event: Pick<globalThis.KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'preventDefault'>) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c' && selection) {
      if (window.getSelection()?.isCollapsed) {
        event.preventDefault()
        copySelection()
      }
    } else if (event.key === 'Escape' && selection) {
      navigation.clearSelection()
    }
  }

  const actions = useRef({
    focusSearch: () => {},
    nextMatch,
    prevMatch,
    stepError,
    initialFocus: () => {},
  })

  actions.current = {
    focusSearch: () => {
      searchRef.current?.focus()
      searchRef.current?.select()
    },
    nextMatch,
    prevMatch,
    stepError,
    initialFocus: navigation.initialFocus,
  }

  useImperativeHandle(
    ref,
    () => ({
      focusSearch: () => actions.current.focusSearch(),
      nextMatch: () => actions.current.nextMatch(),
      prevMatch: () => actions.current.prevMatch(),
      nextError: () => actions.current.stepError(1),
      prevError: () => actions.current.stepError(-1),
      scrollToStart: () => bodyRef.current?.scrollToRow(0, 'start'),
      scrollToEnd: () => bodyRef.current?.scrollToEnd(),
      focusBody: () => bodyRef.current?.focus(),
    }),
    []
  )

  useEffect(() => {
    actions.current.initialFocus()
  }, [])

  const { result, matches } = search
  const counter = result
    ? matches.length === 0
      ? 'No results'
      : `${currentIdx === null ? 0 : currentIdx + 1} of ${result.total}${result.capped ? '+' : ''}`
    : null
  const status = search.failure ?? counter ?? ''
  const noMatches = matches.length === 0
  const emptyLabel = filter === 'matches' ? 'No matching lines' : 'No error lines'
  const webUrl = node.web_url
  const openJob = webUrl ? () => void openExternal(webUrl) : null
  const linesFailed = visibleLines.some((line) => getLine(line) === 'error')

  return (
    <div className={classes.viewer}>
      {phone ? (
        <PhoneLogSearch
          queryText={search.queryText}
          onQueryChange={navigation.changeText}
          onNextMatch={nextMatch}
          inputRef={searchRef}
        />
      ) : (
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
          matchTotal={matches.length}
          onNextMatch={nextMatch}
          onPrevMatch={prevMatch}
          filter={filter}
          onFilterChange={setFilter}
          errorTotal={meta.error_total}
          onNextError={() => stepError(1)}
          onPrevError={() => stepError(-1)}
          wrap={wrap}
          onWrapChange={setWrap}
          timestamps={timestamps}
          onTimestampsChange={setTimestamps}
          selectionCount={selection ? selection.to - selection.from + 1 : 0}
          onCopySelection={copySelection}
          copied={copied}
          onDownload={() => void exportLog()}
          downloading={exporting}
          jobUrl={node.web_url}
          providerName={providerName}
          onOpenJob={() => openJob?.()}
        />
      )}
      {!meta.complete && !meta.truncated && <LiveMarker updatedAtMs={meta.updated_at_ms} variant={variant} />}
      {meta.truncated && (
        <Alert color="gray" variant="light" mx="sm" mb="xs">
          Showing the first 1 GB of this log. Open the job in {providerName} for the rest.
        </Alert>
      )}
      <div
        className={classes.main}
        {...(phone && {
          onTouchStart: (event: TouchEvent<HTMLDivElement>) => {
            swipeStartY.current = event.touches.length === 1 ? (event.touches[0]?.clientY ?? null) : null
          },
          onTouchMove: (event: TouchEvent<HTMLDivElement>) => {
            const start = swipeStartY.current
            const y = event.touches[0]?.clientY

            if (start !== null && y !== undefined && start - y > SWIPE_HIDE_PX) {
              swipeStartY.current = null
              slideSearch(searchRef.current, false)
            }
          },
          onTouchEnd: () => {
            swipeStartY.current = null
          },
          onTouchCancel: () => {
            swipeStartY.current = null
          },
        })}
      >
        <LogBody
          ref={bodyRef}
          jobName={node.name}
          variant={variant}
          meta={meta}
          model={rows.model}
          overrides={rows.overrides}
          groups={filter === 'none' ? rows.groups : NO_GROUPS}
          styleClasses={rows.styleClasses}
          getLine={getLine}
          wrap={wrap}
          timestamps={timestamps}
          stickyHeaders={filter === 'none'}
          selection={selection}
          freshFrom={freshFrom}
          matchIndex={search.matchIndex}
          currentMatch={currentIdx === null ? null : (matches[currentIdx] ?? null)}
          emptyLabel={emptyLabel}
          onVisibleLines={setVisibleLines}
          onViewport={onViewport}
          onToggle={rows.onToggle}
          onSelect={navigation.onSelect}
          onFullLine={onFullLine}
          rowEvents={rowEvents}
          onKeyDown={onBodyKeyDown}
        />
        {behind > 0 && (
          <UnstyledButton
            className={classes.jumpLatest}
            data-variant={variant}
            onClick={() => bodyRef.current?.scrollToEnd()}
          >
            <IconArrowDown size={14} aria-hidden />
            Jump to latest · {behind} new {behind === 1 ? 'line' : 'lines'}
          </UnstyledButton>
        )}
        {!phone && (
          <LogOverviewRuler
            ref={rulerRef}
            lineCount={meta.line_count}
            errorLines={meta.error_lines}
            warningLines={meta.warning_lines}
            matchLines={search.matchLines}
            onSeek={onSeek}
          />
        )}
      </div>
      {phone && (
        <PhoneLogBar
          meta={meta}
          providerName={providerName}
          selection={selection}
          filter={filter}
          onFilterChange={setFilter}
          queryText={search.queryText}
          matchStatus={status}
          noMatches={noMatches}
          onPrevMatch={prevMatch}
          onNextMatch={nextMatch}
          wrap={wrap}
          timestamps={timestamps}
          onWrapChange={setWrap}
          onTimestampsChange={setTimestamps}
          exporting={exporting}
          onExport={() => void exportLog()}
          onOpenJob={openJob}
          onPrevError={() => stepError(-1)}
          onNextError={() => stepError(1)}
          onStart={() => bodyRef.current?.scrollToRow(0, 'start')}
          onEnd={() => bodyRef.current?.scrollToEnd()}
          onSearch={() => slideSearch(searchRef.current, true)}
        />
      )}
      <VisuallyHidden role="status">{status}</VisuallyHidden>
      <VisuallyHidden role="status">{linesFailed ? "Some log lines couldn't be loaded" : ''}</VisuallyHidden>
      {fullLine !== null && <FullLineModal doc={doc} line={fullLine} onClose={() => setFullLine(null)} />}
    </div>
  )
}
