import { Alert, Button, Code, Group, Menu, Stack, Text, VisuallyHidden } from '@mantine/core'
import { useClipboard } from '@mantine/hooks'
import {
  IconArrowDown,
  IconArrowUp,
  IconCheck,
  IconChevronDown,
  IconChevronUp,
  IconCircleX,
  IconCopy,
  IconDots,
  IconExternalLink,
  IconFilter,
} from '@tabler/icons-react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  forwardRef,
  type KeyboardEvent,
  type MouseEvent,
  type Ref,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react'
import { queryKeys } from '../../lib/queryKeys'
import { useRetryRunLog, useRunLogDocument, useRunLogSearch } from '../../queries/useRunLogQueries'
import { service } from '../../services'
import { useLogViewerPrefs } from '../../stores/logViewerPrefsStore'
import { getErrorCode } from '../../types/errors'
import type { LogDocumentMeta, LogSearchQuery, RunLogNode } from '../../types/logs'
import { displayErrorNotification, errorMessage } from '../../utils/errorDisplay'
import { formatBytes } from '../../utils/formatBytes'
import { formatDuration } from '../../utils/formatDuration'
import { openExternal } from '../../utils/openExternal'
import { CopyButton } from '../atoms/CopyButton'
import { StatusIcon } from '../atoms/StatusBadge'
import { type ContextMenuItem, useContextMenu } from '../common/ContextMenu'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { PhoneSearchField } from '../common/PhoneSearchField'
import { StandardModal } from '../common/StandardModal'
import { LogBody, type LogBodyHandle } from './LogBody'
import { LogOverviewRuler, type LogRulerHandle } from './LogOverviewRuler'
import { type MatchTriple, styleClassNames } from './LogRow'
import { LogToolbar } from './LogToolbar'
import classes from './LogViewer.module.css'
import { type DocRef, logLineCache, useLogLines } from './lineCache'
import { collectLines, exportFileName, FULL_LINE_UNITS, newSessionId } from './logActions'
import {
  buildRows,
  filterAfterJump,
  filterRows,
  type LogFilter,
  lineOfItem,
  overridesFor,
  type RowModel,
  revealLine,
  rowAt,
  rowNearLine,
  rowOfLine,
  sectionAtLine,
} from './logRows'

export interface LogViewerHandle {
  focusSearch: () => void
  nextMatch: () => void
  prevMatch: () => void
  nextError: () => void
  prevError: () => void
  scrollToStart: () => void
  scrollToEnd: () => void
  focusBody: () => void
}

export interface LogViewerProps {
  pipelineId: string
  pipelineName: string
  runNumber: number
  node: RunLogNode
  steps: RunLogNode[]
  variant: 'desktop' | 'narrow' | 'phone'
  providerName: string
}

type SessionProps = LogViewerProps & { handleRef: Ref<LogViewerHandle> }

const NO_GROUPS = new Map()
const SEARCH_ERROR = 'Invalid search pattern'

export const LogViewer = forwardRef<LogViewerHandle, LogViewerProps>(function LogViewer(props, ref) {
  return <LogSession key={`${props.pipelineId}:${props.runNumber}:${props.node.id}`} {...props} handleRef={ref} />
})

function OpenJobButton({ node, providerName }: Pick<LogViewerProps, 'node' | 'providerName'>) {
  const url = node.web_url

  return url ? (
    <Button
      variant="default"
      size="xs"
      leftSection={<IconExternalLink size={14} />}
      onClick={() => void openExternal(url)}
    >
      Open job in {providerName}
    </Button>
  ) : null
}

function LogSession({ handleRef, ...props }: SessionProps) {
  const { pipelineId, runNumber, node, steps, providerName } = props
  const docQuery = useRunLogDocument(pipelineId, runNumber, node.id, true)
  const retry = useRetryRunLog()
  const data = docQuery.data
  const open = <OpenJobButton node={node} providerName={providerName} />

  if (!data) {
    if (docQuery.error) {
      return (
        <ErrorState
          title="Couldn't load the job log"
          error={docQuery.error}
          actions={open}
          retrying={docQuery.isFetching}
          onRetry={() => void docQuery.refetch()}
        />
      )
    }

    return <LoadingState message="Preparing log…" />
  }

  if (data.state === 'loading') {
    return (
      <LoadingState
        message={data.received_bytes ? `Downloading log… ${formatBytes(data.received_bytes)}` : 'Preparing log…'}
      />
    )
  }

  if (data.state === 'pending') {
    return (
      <EmptyState
        title="Logs appear when this job finishes"
        description="This job is still running. Its log opens here by itself."
        action={
          <Stack gap="xs" align="stretch" w="100%" maw={420}>
            {steps.map((step) => (
              <Group key={step.id} gap="xs" wrap="nowrap" justify="space-between">
                <Group gap="xs" wrap="nowrap">
                  <StatusIcon status={step.status} />
                  <Text size="sm">{step.name}</Text>
                </Group>
                <Text size="xs" c="dimmed">
                  {formatDuration(step.duration_seconds)}
                </Text>
              </Group>
            ))}
            {open}
          </Stack>
        }
      />
    )
  }

  if (data.state === 'unavailable') {
    return (
      <ErrorState
        title="Couldn't load the job log"
        error={data.message}
        actions={open}
        retrying={retry.isPending}
        onRetry={() => retry.mutate({ pipelineId, runNumber, nodeId: node.id })}
      />
    )
  }

  if (data.line_count === 0) {
    return data.complete ? (
      <EmptyState title="This job didn't write any output" description="The provider returned an empty log." />
    ) : (
      <LoadingState message="Waiting for output…" />
    )
  }

  return <ReadyViewer ref={handleRef} {...props} meta={data} />
}

function FullLineModal({ doc, line, onClose }: { doc: DocRef; line: number; onClose: () => void }) {
  const { pipelineId, runNumber, nodeId, revision } = doc
  const query = useQuery({
    queryKey: [...queryKeys.runLogs.all, 'line', pipelineId, runNumber, nodeId, revision, line],
    queryFn: () => service.readRunLogLines(pipelineId, runNumber, nodeId, revision, line, 1, FULL_LINE_UNITS),
    gcTime: 0,
    staleTime: Infinity,
    retry: false,
  })
  const text = query.data?.lines[0]?.text ?? ''

  return (
    <StandardModal opened onClose={onClose} title={`Line ${line + 1}`} size="lg">
      {query.isPending ? (
        <LoadingState message="Loading line…" />
      ) : query.error ? (
        <ErrorState title="Couldn't load the line" error={query.error} onRetry={() => void query.refetch()} />
      ) : (
        <Stack gap="xs">
          <Group justify="flex-end">
            <CopyButton value={text} label="Copy line" />
          </Group>
          <Code block mah={400} style={{ overflow: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
            {text}
          </Code>
        </Stack>
      )}
    </StandardModal>
  )
}

interface RowsState {
  meta: LogDocumentMeta
  overrides: Set<number>
}

const lineAt = (target: EventTarget | null): number | null => {
  const value = target instanceof HTMLElement ? target.closest<HTMLElement>('[data-line]')?.dataset.line : undefined

  return value === undefined ? null : Number(value)
}

const ReadyViewer = forwardRef<LogViewerHandle, LogViewerProps & { meta: LogDocumentMeta }>(
  function ReadyViewer(props, ref) {
    const { pipelineId, pipelineName, runNumber, node, variant, providerName, meta } = props
    const phone = variant === 'phone'
    const queryClient = useQueryClient()
    const clipboard = useClipboard({ timeout: 2000 })
    const contextMenu = useContextMenu()
    const prefs = useLogViewerPrefs()
    const sessionId = useMemo(() => newSessionId(), [])
    const bodyRef = useRef<LogBodyHandle>(null)
    const rulerRef = useRef<LogRulerHandle>(null)
    const searchRef = useRef<HTMLInputElement>(null)
    const contextLine = useRef<number | null>(null)
    const lastError = useRef<number | null>(null)
    const pendingJump = useRef(false)
    const modelRef = useRef<RowModel>({ count: 0, segments: [] })
    const [rowsState, setRowsState] = useState<RowsState>(() => ({
      meta,
      overrides: meta.focus_line === null ? new Set() : revealLine(meta, new Set(), meta.focus_line),
    }))
    const [filter, setFilter] = useState<LogFilter>('none')
    const [queryText, setQueryText] = useState('')
    const [regex, setRegex] = useState(false)
    const [caseSensitive, setCaseSensitive] = useState(false)
    const [currentIdx, setCurrentIdx] = useState<number | null>(null)
    const [selection, setSelection] = useState<{ anchor: number; from: number; to: number } | null>(null)
    const [fullLine, setFullLine] = useState<number | null>(null)
    const [jump, setJump] = useState<{ line: number; align: 'center' | 'start' } | null>(null)
    const [exporting, setExporting] = useState(false)
    const [visibleLines, setVisibleLines] = useState<number[]>([])

    let current = rowsState

    if (rowsState.meta !== meta) {
      current = {
        meta,
        overrides:
          rowsState.meta.revision === meta.revision
            ? rowsState.overrides
            : overridesFor(meta, rowsState.meta, rowsState.overrides),
      }
      setRowsState(current)
    }

    const { overrides } = current
    const doc = useMemo<DocRef>(
      () => ({ pipelineId, runNumber, nodeId: node.id, revision: meta.revision, lineCount: meta.line_count }),
      [pipelineId, runNumber, node.id, meta.revision, meta.line_count]
    )
    const getLine = useLogLines(doc, visibleLines)

    const searchQuery = useMemo<LogSearchQuery>(
      () => ({ text: queryText, regex, case_sensitive: caseSensitive }),
      [queryText, regex, caseSensitive]
    )
    const search = useRunLogSearch(pipelineId, runNumber, node.id, meta.revision, sessionId, searchQuery)
    const result = queryText ? search.data : undefined
    const matchIndex = useMemo(() => {
      const index = new Map<number, MatchTriple[]>()

      for (const match of result?.matches ?? []) {
        const list = index.get(match[0])

        if (list) {
          list.push(match)
        } else {
          index.set(match[0], [match])
        }
      }

      return index
    }, [result])
    const matches = result?.matches ?? []
    const matchLines = useMemo(() => [...matchIndex.keys()], [matchIndex])

    const model = useMemo(() => {
      if (filter === 'matches') {
        return filterRows(matchLines)
      }

      return filter === 'errors' ? filterRows(meta.error_lines) : buildRows(meta, overrides)
    }, [filter, matchLines, meta, overrides])

    modelRef.current = model

    const groups = useMemo(() => {
      const map = new Map()

      for (const section of meta.sections) {
        if (section.kind === 'group' && section.header_line !== null) {
          map.set(section.header_line, section)
        }
      }

      return map
    }, [meta.sections])
    const styleClasses = useMemo(() => styleClassNames(meta.styles), [meta.styles])

    const anchorLine = useCallback(() => {
      const range = bodyRef.current?.getVisibleRange()

      return range ? lineOfItem(rowAt(modelRef.current, range.first)) : 0
    }, [])

    const goToLine = useCallback(
      (line: number, align: 'center' | 'start' = 'center') => {
        const next = revealLine(current.meta, current.overrides, line)
        const nextFilter = filterAfterJump(filter, line, matchLines, current.meta.error_lines)

        if (next !== current.overrides) {
          setRowsState({ meta: current.meta, overrides: next })
        }

        if (nextFilter !== filter) {
          setFilter(nextFilter)
        }

        setJump({ line, align })
      },
      [current, filter, matchLines]
    )

    useEffect(() => {
      if (!jump) {
        return
      }

      const row = rowOfLine(modelRef.current, jump.line) ?? rowNearLine(modelRef.current, jump.line)

      bodyRef.current?.scrollToRow(row, jump.align)
    }, [jump])

    const goToMatch = useCallback(
      (index: number) => {
        const match = matches[index]

        if (match) {
          setCurrentIdx(index)
          goToLine(match[0])
        }
      },
      [matches, goToLine]
    )

    const nextMatch = () => {
      if (matches.length === 0) {
        return
      }

      if (currentIdx !== null) {
        goToMatch((currentIdx + 1) % matches.length)

        return
      }

      const anchor = anchorLine()
      const found = matches.findIndex((match) => match[0] >= anchor)

      goToMatch(found === -1 ? 0 : found)
    }

    const prevMatch = () => {
      if (matches.length === 0) {
        return
      }

      if (currentIdx !== null) {
        goToMatch((currentIdx - 1 + matches.length) % matches.length)

        return
      }

      const anchor = anchorLine()
      const found = matches.findLastIndex((match) => match[0] < anchor)

      goToMatch(found === -1 ? matches.length - 1 : found)
    }

    function changeQuery<T>(set: (value: T) => void, value: T) {
      pendingJump.current = true
      setCurrentIdx(null)
      set(value)
    }

    function changeText(value: string) {
      if (!value && filter === 'matches') {
        setFilter('none')
      }
      changeQuery(setQueryText, value)
    }

    useEffect(() => {
      if (!result || search.isPlaceholderData) {
        return
      }

      if (pendingJump.current) {
        pendingJump.current = false

        if (result.matches.length > 0) {
          const anchor = anchorLine()
          const found = result.matches.findIndex((match) => match[0] >= anchor)
          const index = found === -1 ? 0 : found

          setCurrentIdx(index)
          goToLine(result.matches[index][0])
        }
      } else {
        setCurrentIdx((index) => (index !== null && index < result.matches.length ? index : null))
      }
    }, [result, search.isPlaceholderData, anchorLine, goToLine])

    const stepError = (direction: 1 | -1) => {
      const errors = meta.error_lines

      if (errors.length === 0) {
        return
      }

      const range = bodyRef.current?.getVisibleRange()
      const last = lastError.current
      const lastRow = last === null ? null : rowOfLine(modelRef.current, last)
      const visible = lastRow !== null && range !== undefined && lastRow >= range.first && lastRow <= range.last
      const reference = visible && last !== null ? last : anchorLine()
      const target =
        direction > 0
          ? (errors.find((line) => line > reference) ?? errors[0])
          : (errors.findLast((line) => line < reference) ?? errors[errors.length - 1])

      lastError.current = target
      setSelection({ anchor: target, from: target, to: target })
      goToLine(target)
    }

    const copyRange = async (from: number, to: number, title: string) => {
      try {
        const text = await collectLines(from, to, (start, count) =>
          service.readRunLogLines(pipelineId, runNumber, node.id, meta.revision, start, count, FULL_LINE_UNITS)
        )

        clipboard.copy(text)
      } catch (error) {
        displayErrorNotification(error, title)
      }
    }

    const copySelection = () => {
      if (selection) {
        void copyRange(selection.from, selection.to + 1, "Couldn't copy the lines")
      }
    }

    const exportLog = async () => {
      setExporting(true)

      try {
        await service.exportRunLog(
          pipelineId,
          runNumber,
          node.id,
          meta.revision,
          exportFileName(pipelineName, runNumber, node.name)
        )
      } catch (error) {
        displayErrorNotification(error, "Couldn't export the log")
      } finally {
        setExporting(false)
      }
    }

    const onSelect = useCallback(
      (line: number, extend: boolean) => {
        if (filter !== 'none') {
          setFilter('none')
          setSelection({ anchor: line, from: line, to: line })
          goToLine(line)

          return
        }

        setSelection((previous) => {
          if (extend && previous) {
            return {
              anchor: previous.anchor,
              from: Math.min(previous.anchor, line),
              to: Math.max(previous.anchor, line),
            }
          }

          return previous && previous.from === line && previous.to === line
            ? null
            : { anchor: line, from: line, to: line }
        })
      },
      [filter, goToLine]
    )

    const onToggle = useCallback(
      (sectionId: number) =>
        setRowsState((previous) => {
          const overrides = new Set(previous.overrides)

          if (!overrides.delete(sectionId)) {
            overrides.add(sectionId)
          }

          return { ...previous, overrides }
        }),
      []
    )

    const onViewport = useCallback((firstRow: number, lastRow: number) => {
      rulerRef.current?.setViewport(
        lineOfItem(rowAt(modelRef.current, firstRow)),
        lineOfItem(rowAt(modelRef.current, lastRow))
      )
    }, [])

    const onFullLine = useCallback((line: number) => setFullLine(line), [])

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
          icon: <IconCopy size={14} />,
          onClick: () => void copyRange(line, line + 1, "Couldn't copy the line"),
        },
        !!step && {
          label: 'Copy step output',
          icon: <IconCopy size={14} />,
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

    const onBodyKeyDown = (event: KeyboardEvent<HTMLElement>) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c' && selection) {
        if (window.getSelection()?.isCollapsed) {
          event.preventDefault()
          copySelection()
        }
      } else if (event.key === 'Escape' && selection) {
        setSelection(null)
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
      initialFocus: () => {
        if (meta.focus_line !== null) {
          setSelection({ anchor: meta.focus_line, from: meta.focus_line, to: meta.focus_line })
          goToLine(meta.focus_line)
        } else if (!meta.complete) {
          bodyRef.current?.scrollToEnd()
        }
      },
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

    const searchFailure = search.error
      ? getErrorCode(search.error) === 'BAD_REQUEST'
        ? SEARCH_ERROR
        : errorMessage(search.error, "Couldn't search the log")
      : null
    const counter = result
      ? result.matches.length === 0
        ? 'No results'
        : `${currentIdx === null ? 0 : currentIdx + 1} of ${result.total}${result.capped ? '+' : ''}`
      : null
    const selectionCount = selection ? selection.to - selection.from + 1 : 0
    const noErrors = meta.error_total === 0
    const noMatches = matches.length === 0
    const emptyLabel = filter === 'matches' ? 'No matching lines' : 'No error lines'
    const openJob = node.web_url ? () => void openExternal(node.web_url as string) : null

    const body = (
      <LogBody
        ref={bodyRef}
        jobName={node.name}
        variant={variant}
        meta={meta}
        model={model}
        overrides={overrides}
        groups={filter === 'none' ? groups : NO_GROUPS}
        styleClasses={styleClasses}
        getLine={getLine}
        wrap={prefs.wrap}
        timestamps={prefs.timestamps}
        stickyHeaders={filter === 'none'}
        selection={selection}
        matchIndex={matchIndex}
        currentMatch={currentIdx === null ? null : (matches[currentIdx] ?? null)}
        emptyLabel={emptyLabel}
        onVisibleLines={setVisibleLines}
        onViewport={onViewport}
        onToggle={onToggle}
        onSelect={onSelect}
        onFullLine={onFullLine}
        rowEvents={rowEvents}
        onKeyDown={onBodyKeyDown}
      />
    )

    return (
      <div className={classes.viewer}>
        {phone ? (
          <>
            <div className={classes.phoneSearch}>
              <PhoneSearchField
                value={queryText}
                placeholder="Search logs"
                onChange={changeText}
                onClear={() => changeText('')}
                onSubmit={nextMatch}
              />
            </div>
            {queryText && (
              <div className={classes.phoneMatches}>
                <span>{searchFailure ?? counter ?? ''}</span>
                <Group gap={0} wrap="nowrap">
                  <button
                    type="button"
                    className={classes.phoneButton}
                    aria-label="Previous match"
                    disabled={noMatches}
                    onClick={prevMatch}
                  >
                    <IconChevronUp size="1.2em" aria-hidden />
                  </button>
                  <button
                    type="button"
                    className={classes.phoneButton}
                    aria-label="Next match"
                    disabled={noMatches}
                    onClick={nextMatch}
                  >
                    <IconChevronDown size="1.2em" aria-hidden />
                  </button>
                </Group>
              </div>
            )}
          </>
        ) : (
          <LogToolbar
            variant={variant}
            searchRef={searchRef}
            query={queryText}
            onQueryChange={changeText}
            regex={regex}
            onRegexChange={(value) => changeQuery(setRegex, value)}
            caseSensitive={caseSensitive}
            onCaseSensitiveChange={(value) => changeQuery(setCaseSensitive, value)}
            counter={counter}
            searchError={searchFailure}
            matchTotal={matches.length}
            onNextMatch={nextMatch}
            onPrevMatch={prevMatch}
            filter={filter}
            onFilterChange={setFilter}
            errorTotal={meta.error_total}
            onNextError={() => stepError(1)}
            onPrevError={() => stepError(-1)}
            wrap={prefs.wrap}
            onWrapChange={prefs.setWrap}
            timestamps={prefs.timestamps}
            onTimestampsChange={prefs.setTimestamps}
            selectionCount={selectionCount}
            onCopySelection={copySelection}
            copied={clipboard.copied}
            onDownload={() => void exportLog()}
            downloading={exporting}
            jobUrl={node.web_url}
            providerName={providerName}
            onOpenJob={() => openJob?.()}
          />
        )}
        {meta.truncated && (
          <Alert color="gray" variant="light" mx="sm" mb="xs">
            Showing the first 1 GB of this log. Open the job in {providerName} for the rest.
          </Alert>
        )}
        <div className={classes.main}>
          {body}
          {!phone && (
            <LogOverviewRuler
              ref={rulerRef}
              lineCount={meta.line_count}
              errorLines={meta.error_lines}
              warningLines={meta.warning_lines}
              matchLines={matchLines}
              onSeek={(line) => goToLine(line, 'start')}
            />
          )}
        </div>
        {phone && (
          <div className={classes.phoneBar}>
            <button
              type="button"
              className={classes.phoneButton}
              aria-label="Next error"
              disabled={noErrors}
              onClick={() => stepError(1)}
            >
              <IconCircleX size="1.2em" aria-hidden />
              {meta.error_total > 0 && <span>{meta.error_total}</span>}
            </button>
            {queryText && (
              <button
                type="button"
                className={classes.phoneButton}
                aria-label="Matching lines"
                aria-pressed={filter === 'matches'}
                disabled={noMatches && filter !== 'matches'}
                onClick={() => setFilter(filter === 'matches' ? 'none' : 'matches')}
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
                  aria-checked={prefs.wrap}
                  rightSection={prefs.wrap ? <IconCheck size={14} /> : null}
                  onClick={() => prefs.setWrap(!prefs.wrap)}
                >
                  Wrap lines
                </Menu.Item>
                <Menu.Item
                  role="menuitemcheckbox"
                  aria-checked={prefs.timestamps}
                  rightSection={prefs.timestamps ? <IconCheck size={14} /> : null}
                  onClick={() => prefs.setTimestamps(!prefs.timestamps)}
                >
                  Show timestamps
                </Menu.Item>
                <Menu.Item
                  role="menuitemcheckbox"
                  aria-checked={filter === 'errors'}
                  disabled={noErrors && filter !== 'errors'}
                  rightSection={filter === 'errors' ? <IconCheck size={14} /> : null}
                  onClick={() => setFilter(filter === 'errors' ? 'none' : 'errors')}
                >
                  Show error lines only
                </Menu.Item>
                <Menu.Divider />
                <Menu.Item
                  leftSection={<IconArrowUp size={14} />}
                  onClick={() => bodyRef.current?.scrollToRow(0, 'start')}
                >
                  Go to top
                </Menu.Item>
                <Menu.Item leftSection={<IconArrowDown size={14} />} onClick={() => bodyRef.current?.scrollToEnd()}>
                  Go to bottom
                </Menu.Item>
                <Menu.Divider />
                <Menu.Item disabled={exporting} onClick={() => void exportLog()}>
                  Export log
                </Menu.Item>
                {openJob && <Menu.Item onClick={openJob}>Open job in {providerName}</Menu.Item>}
              </Menu.Dropdown>
            </Menu>
          </div>
        )}
        <VisuallyHidden role="status">{searchFailure ?? counter ?? ''}</VisuallyHidden>
        {fullLine !== null && <FullLineModal doc={doc} line={fullLine} onClose={() => setFullLine(null)} />}
      </div>
    )
  }
)
