import { Alert, UnstyledButton, VisuallyHidden } from '@mantine/core'
import { IconArrowDown } from '@tabler/icons-react'
import { useQueryClient } from '@tanstack/react-query'
import { type ReactNode, useCallback, useEffect, useImperativeHandle, useRef, useState } from 'react'
import { queryKeys } from '../../../lib/queryKeys'
import { useLogViewerPrefs } from '../../../stores/logViewerPrefsStore'
import type { LogDocumentMeta } from '../../../types/logs'
import { openLink } from '../../../utils/openExternal'
import { useLiveTail } from '../hooks/useLiveTail'
import { useLogExport } from '../hooks/useLogExport'
import { useLogLines } from '../hooks/useLogLines'
import { useLogNavigation } from '../hooks/useLogNavigation'
import { useLogRows } from '../hooks/useLogRows'
import { useLogSearch } from '../hooks/useLogSearch'
import { slideSearch, useLogViewerInteractions } from '../hooks/useLogViewerInteractions'
import { logLineCache } from '../lib/lineCache'
import type { DocRef } from '../lib/lineCacheTypes'
import type { LogFilter } from '../lib/logRows'
import type { LogViewerProps } from '../lib/logViewerTypes'
import type { LogBodyHandle } from '../lib/logWindowTypes'
import { searchCounter } from '../lib/searchCounter'
import { FullLineModal } from './FullLineModal'
import { LiveMarker } from './LiveMarker'
import { LogBody } from './LogBody'
import { LogOverviewRuler, type LogRulerHandle } from './LogOverviewRuler'
import classes from './LogViewer.module.css'
import { PhoneLogBar } from './PhoneLogControls'
import { ReadyToolbar } from './ReadyToolbar'

const NO_GROUPS = new Map()

type ReadyViewerProps = LogViewerProps & { meta: LogDocumentMeta }

export function ReadyViewer({ ref, ...props }: ReadyViewerProps): ReactNode {
  const { pipelineId, pipelineName, runNumber, node, variant, providerName, meta } = props
  const phone = variant === 'phone'
  const queryClient = useQueryClient()
  const wrap = useLogViewerPrefs((s) => s.wrap)
  const timestamps = useLogViewerPrefs((s) => s.timestamps)
  const setWrap = useLogViewerPrefs((s) => s.setWrap)
  const setTimestamps = useLogViewerPrefs((s) => s.setTimestamps)
  const bodyRef = useRef<LogBodyHandle>(null)
  const rulerRef = useRef<LogRulerHandle>(null)
  const searchRef = useRef<HTMLInputElement>(null)
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
  const logExport = useLogExport({ doc, pipelineName, nodeName: node.name, selection })
  const { copyRange, copySelection, exportLog, exporting } = logExport

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

  const { rowEvents, onBodyKeyDown, touchEvents } = useLogViewerInteractions({
    meta,
    phone,
    selection,
    searchRef,
    getLine,
    copyRange,
    copySelection,
    clearSelection: navigation.clearSelection,
    onFullLine,
  })

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

  const counter = searchCounter(search, currentIdx)
  const status = search.failure ?? counter ?? ''
  const noMatches = search.matches.length === 0
  const emptyLabel = filter === 'matches' ? 'No matching lines' : 'No error lines'
  const webUrl = node.web_url
  const openJob = webUrl ? () => openLink(webUrl) : null
  const linesFailed = visibleLines.some((line) => getLine(line) === 'error')

  return (
    <div className={classes.viewer}>
      <ReadyToolbar
        variant={variant}
        searchRef={searchRef}
        search={search}
        navigation={navigation}
        counter={counter}
        filter={filter}
        onFilterChange={setFilter}
        errorTotal={meta.error_total}
        logExport={logExport}
        jobUrl={node.web_url}
        providerName={providerName}
        onOpenJob={openJob}
      />
      {!meta.complete && !meta.truncated && <LiveMarker meta={meta} target={doc} variant={variant} />}
      {meta.truncated && (
        <Alert color="gray" variant="light" mx="sm" mb="xs">
          Showing the first 1 GB of this log. Open the job in {providerName} for the rest.
        </Alert>
      )}
      <div className={classes.main} {...touchEvents}>
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
          currentMatch={currentIdx === null ? null : (search.matches[currentIdx] ?? null)}
          emptyLabel={emptyLabel}
          onVisibleLines={setVisibleLines}
          onViewport={onViewport}
          onToggle={rows.onToggle}
          onSelect={navigation.onSelect}
          onFullLine={onFullLine}
          onRetry={() => logLineCache.retry(doc)}
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
      {fullLine !== null && (
        <FullLineModal
          doc={doc}
          line={fullLine}
          pipelineName={pipelineName}
          nodeName={node.name}
          onClose={() => setFullLine(null)}
        />
      )}
    </div>
  )
}
