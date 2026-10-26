import type { PipelineStatus } from './index'

export type LogNodeKind = 'stage' | 'job' | 'step'

export type LogAvailability = 'ready' | 'partial' | 'pending'

export interface RunLogNode {
  id: string
  parent_id: string | null
  kind: LogNodeKind
  name: string
  status: PipelineStatus
  started_at: string | null
  finished_at: string | null
  duration_seconds: number | null
  log: LogAvailability | null
  web_url: string | null
  depth: number
  navigable: boolean
}

export interface RunLogTree {
  nodes: RunLogNode[]
  default_node_id: string | null
  complete: boolean
}

export type AnsiColor = 'black' | 'red' | 'green' | 'yellow' | 'blue' | 'magenta' | 'cyan' | 'white'

export interface LogStyle {
  fg: AnsiColor | null
  bg: AnsiColor | null
  bold: boolean
  dim: boolean
  italic: boolean
  underline: boolean
  strike: boolean
}

export type LogSectionKind = 'step' | 'group'

export interface LogSection {
  id: number
  parent: number | null
  kind: LogSectionKind
  node_id: string | null
  title: string
  status: PipelineStatus | null
  duration_ms: number | null
  header_line: number | null
  start_line: number
  end_line: number
  error_count: number
  open: boolean
}

export interface LogDocumentMeta {
  node_id: string
  revision: number
  complete: boolean
  truncated: boolean
  line_count: number
  text_bytes: number
  first_timestamp_ms: number | null
  styles: LogStyle[]
  sections: LogSection[]
  error_lines: number[]
  warning_lines: number[]
  error_total: number
  warning_total: number
  focus_line: number | null
}

export type RunLogDocument =
  | ({ state: 'ready' } & LogDocumentMeta)
  | { state: 'loading'; received_bytes: number }
  | { state: 'pending' }
  | { state: 'unavailable'; message: string }

export type LineKind = 'plain' | 'group' | 'error' | 'warning' | 'notice' | 'debug' | 'command'

export type LogSpan = [start: number, end: number, style: number]

export interface LogLineWire {
  text: string
  kind?: LineKind
  ts?: number
  spans?: LogSpan[]
  omitted?: number
}

export interface LogLine {
  text: string
  kind: LineKind
  ts: number | null
  spans: LogSpan[]
  omitted: number
}

export interface LogLinesPage {
  start: number
  lines: LogLineWire[]
}

export interface LogSearchQuery {
  text: string
  regex: boolean
  case_sensitive: boolean
}

export interface LogSearchResult {
  matches: Array<[line: number, start: number, end: number]>
  total: number
  capped: boolean
}

export const normalizeLine = (wire: LogLineWire): LogLine => ({
  text: wire.text,
  kind: wire.kind ?? 'plain',
  ts: wire.ts ?? null,
  spans: wire.spans ?? [],
  omitted: wire.omitted ?? 0,
})
