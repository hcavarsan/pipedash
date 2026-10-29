import { Anchor, Text, UnstyledButton } from '@mantine/core'
import { IconAlertTriangle, IconChevronDown, IconChevronRight, IconCircleX } from '@tabler/icons-react'
import { memo, type ReactNode, useMemo } from 'react'
import type { AnsiColor, LogSection, LogStyle } from '../../types/logs'
import { formatDuration } from '../../utils/formatDuration'
import { isOneOf } from '../../utils/isOneOf'
import { openExternal } from '../../utils/openExternal'
import { typedKeys } from '../../utils/typedKeys'
import { StatusIcon } from '../atoms/StatusBadge'
import classes from './LogViewer.module.css'
import type { LineState } from './lineCache'
import { type Highlight, segmentsFor } from './segments'

export type MatchTriple = [line: number, start: number, end: number]

type LogRowCommonProps = {
  styleClasses: readonly string[]
  gutterCh: number
  timestamps: boolean
  onToggle: (sectionId: number) => void
}

export type LogRowProps = LogRowCommonProps &
  (
    | {
        kind: 'step'
        section: LogSection
        open: boolean
      }
    | {
        kind: 'line'
        line: number
        data: LineState
        section: LogSection | null
        open: boolean
        selected: boolean
        matches: readonly MatchTriple[] | undefined
        current: MatchTriple | null
        onSelect: (line: number, extend: boolean) => void
        onFullLine: (line: number) => void
      }
  )

const FG: Record<AnsiColor, string | null | undefined> = {
  black: classes.fgBlack,
  red: classes.fgRed,
  green: classes.fgGreen,
  yellow: classes.fgYellow,
  blue: classes.fgBlue,
  magenta: classes.fgMagenta,
  cyan: classes.fgCyan,
  white: null,
}

const BG: Record<AnsiColor, string | undefined> = {
  black: classes.bgBlack,
  red: classes.bgRed,
  green: classes.bgGreen,
  yellow: classes.bgYellow,
  blue: classes.bgBlue,
  magenta: classes.bgMagenta,
  cyan: classes.bgCyan,
  white: classes.bgWhite,
}

const join = (...names: Array<string | false | null | undefined>) => names.filter(Boolean).join(' ')

export const styleClassNames = (styles: readonly LogStyle[]): string[] =>
  styles.map((style) =>
    join(
      style.fg && FG[style.fg],
      style.bg && BG[style.bg],
      style.bold && classes.bold,
      style.dim && classes.dim,
      style.italic && classes.italic,
      style.underline && classes.underline,
      style.strike && classes.strike
    )
  )

const timeFormat = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  fractionalSecondDigits: 3,
  hour12: false,
})

const KIND_CLASS = {
  error: classes.error,
  warning: classes.warning,
  debug: classes.debug,
  command: classes.command,
} as const

function StepHeader({
  section,
  open,
  onToggle,
}: {
  section: LogSection
  open: boolean
  onToggle: (sectionId: number) => void
}): ReactNode {
  const Chevron = open ? IconChevronDown : IconChevronRight

  return (
    <div className={classes.stepWrap}>
      <UnstyledButton
        className={join(classes.step, 'interactive-surface')}
        aria-expanded={open}
        onClick={() => onToggle(section.id)}
      >
        <Chevron size={14} aria-hidden />
        {section.status && <StatusIcon status={section.status} />}
        <span className={classes.stepTitle}>{section.title}</span>
        {section.error_count > 0 && (
          <Text component="span" size="xs" c="dimmed" className={classes.stepMeta}>
            {section.error_count} {section.error_count === 1 ? 'error' : 'errors'}
          </Text>
        )}
        {section.duration_ms != null && (
          <span className={classes.stepMeta}>{formatDuration(section.duration_ms / 1000)}</span>
        )}
      </UnstyledButton>
    </div>
  )
}

function LineText({
  text,
  spans,
  styleClasses,
  matches,
  current,
}: {
  text: string
  spans: Parameters<typeof segmentsFor>[1]
  styleClasses: readonly string[]
  matches: readonly MatchTriple[] | undefined
  current: MatchTriple | null
}) {
  const segments = useMemo(() => {
    const highlights: Highlight[] = (matches ?? []).map((match) => [match[1], match[2], match === current])

    return segmentsFor(text, spans, highlights)
  }, [text, spans, matches, current])

  let position = 0

  return segments.map((segment) => {
    const key = position

    position += segment.text.length

    const className = join(
      segment.style !== null && styleClasses[segment.style],
      segment.match === 1 && classes.match,
      segment.match === 2 && classes.matchCurrent
    )
    const { href } = segment

    if (href) {
      return (
        <Anchor
          key={key}
          inherit
          href={href}
          target="_blank"
          rel="noopener noreferrer"
          className={className || undefined}
          onClick={(event) => {
            event.preventDefault()
            void openExternal(href)
          }}
        >
          {segment.text}
        </Anchor>
      )
    }

    return className ? (
      <span key={key} className={className}>
        {segment.text}
      </span>
    ) : (
      segment.text
    )
  })
}

export const LogRow = memo(function LogRow(props: LogRowProps): ReactNode {
  if (props.kind === 'step') {
    return <StepHeader section={props.section} open={props.open} onToggle={props.onToggle} />
  }

  const { line, data, section, open, selected, gutterCh, timestamps, onToggle, onSelect, onFullLine } = props
  const loaded = typeof data === 'object' ? data : null
  const lineKind = loaded?.kind
  const Glyph = lineKind === 'error' ? IconCircleX : lineKind === 'warning' ? IconAlertTriangle : null
  const kindClass = lineKind && isOneOf(typedKeys(KIND_CLASS), lineKind) ? KIND_CLASS[lineKind] : null
  return (
    <div
      className={join(classes.row, kindClass, selected && classes.selected)}
      style={{ '--log-gutter': gutterCh }}
      data-line={line}
      data-selectable
    >
      <button
        type="button"
        className={classes.gutter}
        aria-label={`Select line ${line + 1}`}
        onClick={(event) => onSelect(line, event.shiftKey)}
      >
        {Glyph && (
          <span className={join(classes.glyph, lineKind === 'error' ? classes.glyphError : classes.glyphWarning)}>
            <Glyph size={14} aria-hidden />
          </span>
        )}
        <span className={classes.num} aria-hidden>
          {line + 1}
        </span>
      </button>
      {timestamps && <span className={classes.ts}>{loaded?.ts != null ? timeFormat.format(loaded.ts) : ''}</span>}
      <span className={classes.text}>
        {section && (
          <button
            type="button"
            className={classes.chevron}
            aria-expanded={open}
            aria-label={`${open ? 'Collapse' : 'Expand'} ${section.title}`}
            onClick={() => onToggle(section.id)}
          >
            {open ? <IconChevronDown size={14} aria-hidden /> : <IconChevronRight size={14} aria-hidden />}
          </button>
        )}
        {data === 'loading' && <span className={classes.dim}>Loading…</span>}
        {data === 'error' && <span className={classes.fgRed}>Couldn't load this line</span>}
        {loaded && (
          <LineText
            text={loaded.text}
            spans={loaded.spans}
            styleClasses={props.styleClasses}
            matches={props.matches}
            current={props.current}
          />
        )}
        {loaded && loaded.omitted > 0 && (
          <button type="button" className={classes.omitted} onClick={() => onFullLine(line)}>
            … {loaded.omitted} more characters
          </button>
        )}
      </span>
    </div>
  )
})
