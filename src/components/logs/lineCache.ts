import { useEffect, useState, useSyncExternalStore } from 'react'

import { service } from '../../services'
import { getErrorCode } from '../../types/errors'
import { type LogLine, normalizeLine } from '../../types/logs'

export const LOG_LINE_UNITS = 4096

export interface DocRef {
  pipelineId: string
  runNumber: number
  nodeId: string
  revision: number
  lineCount: number
}

export type LineState = LogLine | 'loading' | 'error' | undefined

interface Entry {
  line: LogLine
  bytes: number
}

interface Request {
  doc: DocRef
  start: number
  count: number
}

interface Need {
  doc: DocRef
  lines: readonly number[]
}

const ALIGN_LINES = 64
const MAX_REQUEST_LINES = 500
const RETRY_MS = 5000
const NOT_FOUND_CODE = 'NOT_FOUND'

const lineKey = (doc: DocRef, line: number): string =>
  `${doc.pipelineId}\u0000${doc.runNumber}\u0000${doc.nodeId}#${doc.revision}#${line}`

export class LogLineCache {
  private readonly budgetBytes: number
  private readonly maxConcurrent: number
  private readonly entries = new Map<string, Entry>()
  private readonly pending = new Set<string>()
  private readonly failed = new Set<string>()
  private readonly needed = new Map<string, number>()
  private readonly owners = new Map<symbol, Need>()
  private readonly queue: Request[] = []
  private readonly listeners = new Set<() => void>()
  private readonly goneHandlers = new Set<(doc: DocRef) => void>()
  private readonly timers = new Set<() => void>()
  private bytes = 0
  private active = 0
  private generation = 0
  private changes = 0

  constructor(budgetBytes = 16 * 1024 * 1024, maxConcurrent = 2) {
    this.budgetBytes = budgetBytes
    this.maxConcurrent = maxConcurrent
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)

    return () => {
      this.listeners.delete(listener)
    }
  }

  readonly version = (): number => this.changes

  line(doc: DocRef, line: number): LineState {
    const key = lineKey(doc, line)
    const entry = this.entries.get(key)

    if (entry) {
      return entry.line
    }

    if (this.failed.has(key)) {
      return 'error'
    }

    return this.pending.has(key) ? 'loading' : undefined
  }

  need(owner: symbol, doc: DocRef, lines: readonly number[]): void {
    this.setNeeded(owner, { doc, lines })
    for (const line of lines) {
      const key = lineKey(doc, line)
      const entry = this.entries.get(key)

      if (entry) {
        this.entries.delete(key)
        this.entries.set(key, entry)
      }
    }
    this.plan(doc, lines)
  }

  release(owner: symbol): void {
    if (this.owners.has(owner)) {
      this.setNeeded(owner, null)
      this.evict()
    }
  }

  onRevisionGone(handler: (doc: DocRef) => void): () => void {
    this.goneHandlers.add(handler)

    return () => {
      this.goneHandlers.delete(handler)
    }
  }

  clear(): void {
    this.generation += 1
    this.entries.clear()
    this.pending.clear()
    this.failed.clear()
    this.queue.length = 0
    this.bytes = 0
    for (const cancel of this.timers) {
      cancel()
    }
    this.timers.clear()
    this.changed()
    for (const owner of [...this.owners.values()]) {
      this.plan(owner.doc, owner.lines)
    }
  }

  private setNeeded(owner: symbol, next: Need | null): void {
    const previous = this.owners.get(owner)

    if (previous) {
      for (const line of previous.lines) {
        const key = lineKey(previous.doc, line)
        const count = (this.needed.get(key) ?? 1) - 1

        if (count > 0) {
          this.needed.set(key, count)
        } else {
          this.needed.delete(key)
        }
      }
    }

    if (next) {
      this.owners.set(owner, next)
      for (const line of next.lines) {
        const key = lineKey(next.doc, line)

        this.needed.set(key, (this.needed.get(key) ?? 0) + 1)
      }
    } else {
      this.owners.delete(owner)
    }
  }

  private settled(key: string): boolean {
    return this.entries.has(key) || this.pending.has(key) || this.failed.has(key)
  }

  private plan(doc: DocRef, lines: readonly number[]): void {
    const missing = [...new Set(lines)]
      .filter((line) => Number.isInteger(line) && line >= 0 && line < doc.lineCount)
      .filter((line) => !this.settled(lineKey(doc, line)))
      .sort((a, b) => a - b)
    const runs: Array<[number, number]> = []

    for (const line of missing) {
      const last = runs.at(-1)

      if (last && last[1] === line) {
        last[1] = line + 1
      } else {
        runs.push([line, line + 1])
      }
    }

    for (const [index, run] of runs.entries()) {
      const floor = Math.max(Math.floor(run[0] / ALIGN_LINES) * ALIGN_LINES, runs[index - 1]?.[1] ?? 0)
      const ceil = Math.min(
        Math.ceil(run[1] / ALIGN_LINES) * ALIGN_LINES,
        doc.lineCount,
        runs[index + 1]?.[0] ?? Infinity
      )

      while (run[0] > floor && !this.settled(lineKey(doc, run[0] - 1))) {
        run[0] -= 1
      }
      while (run[1] < ceil && !this.settled(lineKey(doc, run[1]))) {
        run[1] += 1
      }

      for (let start = run[0]; start < run[1]; start += MAX_REQUEST_LINES) {
        const count = Math.min(MAX_REQUEST_LINES, run[1] - start)

        for (let line = start; line < start + count; line += 1) {
          this.pending.add(lineKey(doc, line))
        }
        this.queue.push({ doc, start, count })
      }
    }

    if (runs.length > 0) {
      this.changed()
    }
    this.pump()
  }

  private pump(): void {
    while (this.active < this.maxConcurrent) {
      const request = this.queue.shift()

      if (!request) {
        return
      }

      if (!this.wanted(request)) {
        this.unmark(request)
        this.changed()
        continue
      }

      this.active += 1
      void this.fetch(request).finally(() => {
        this.active -= 1
        this.pump()
      })
    }
  }

  private wanted({ doc, start, count }: Request): boolean {
    for (let line = start; line < start + count; line += 1) {
      if (this.needed.has(lineKey(doc, line))) {
        return true
      }
    }

    return false
  }

  private unmark({ doc, start, count }: Request): void {
    for (let line = start; line < start + count; line += 1) {
      this.pending.delete(lineKey(doc, line))
    }
  }

  private async fetch(request: Request): Promise<void> {
    const { doc, start, count } = request
    const generation = this.generation
    let next = start

    try {
      while (next < start + count) {
        const page = await service.readRunLogLines(
          doc.pipelineId,
          doc.runNumber,
          doc.nodeId,
          doc.revision,
          next,
          start + count - next,
          LOG_LINE_UNITS
        )

        if (generation !== this.generation) {
          return
        }

        if (page.lines.length === 0) {
          throw new Error('The log ended before the requested lines were read')
        }

        for (const [index, wire] of page.lines.entries()) {
          this.insert(lineKey(doc, next + index), normalizeLine(wire))
        }
        next += page.lines.length
        this.evict()
        this.changed()
      }

      this.unmark(request)
      this.changed()
    } catch (error) {
      if (generation === this.generation) {
        this.fail(request, error)
      }
    }
  }

  private insert(key: string, line: LogLine): void {
    const previous = this.entries.get(key)
    const bytes = line.text.length * 2 + line.spans.length * 24 + 64

    if (previous) {
      this.bytes -= previous.bytes
      this.entries.delete(key)
    }
    this.entries.set(key, { line, bytes })
    this.bytes += bytes
    this.pending.delete(key)
    this.failed.delete(key)
  }

  private evict(): void {
    for (const [key, entry] of this.entries) {
      if (this.bytes <= this.budgetBytes) {
        return
      }

      if (!this.needed.has(key)) {
        this.entries.delete(key)
        this.bytes -= entry.bytes
      }
    }
  }

  private fail(request: Request, error: unknown): void {
    const { doc, start, count } = request
    const marked: string[] = []

    for (let line = start; line < start + count; line += 1) {
      const key = lineKey(doc, line)

      this.pending.delete(key)
      if (!this.entries.has(key)) {
        this.failed.add(key)
        marked.push(key)
      }
    }
    this.changed()

    if (getErrorCode(error) === NOT_FOUND_CODE) {
      for (const handler of [...this.goneHandlers]) {
        handler(doc)
      }
    }

    const timer = setTimeout(() => {
      this.timers.delete(cancel)
      for (const key of marked) {
        this.failed.delete(key)
      }
      this.changed()
      for (const owner of [...this.owners.values()]) {
        this.plan(owner.doc, owner.lines)
      }
    }, RETRY_MS)
    const cancel = () => clearTimeout(timer)

    this.timers.add(cancel)
  }

  private changed(): void {
    this.changes += 1
    for (const listener of [...this.listeners]) {
      listener()
    }
  }
}

export const logLineCache = new LogLineCache()

export function useLogLines(doc: DocRef | null, lines: readonly number[]): (line: number) => LineState {
  const [owner] = useState(() => Symbol('log-lines'))

  useSyncExternalStore(logLineCache.subscribe, logLineCache.version)

  const pipelineId = doc?.pipelineId
  const runNumber = doc?.runNumber
  const nodeId = doc?.nodeId
  const revision = doc?.revision
  const lineCount = doc?.lineCount

  useEffect(() => {
    if (
      pipelineId === undefined ||
      runNumber === undefined ||
      nodeId === undefined ||
      revision === undefined ||
      lineCount === undefined
    ) {
      logLineCache.release(owner)

      return
    }

    logLineCache.need(owner, { pipelineId, runNumber, nodeId, revision, lineCount }, lines)
  }, [owner, pipelineId, runNumber, nodeId, revision, lineCount, lines])

  useEffect(() => () => logLineCache.release(owner), [owner])

  return (line) => (doc ? logLineCache.line(doc, line) : undefined)
}
