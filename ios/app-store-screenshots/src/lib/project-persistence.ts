import { STORAGE_KEY } from './constants'
import { parseProject } from './project-migration'
import { isRecord } from './project-validation'
import type { ProjectState } from './types'

export type FileLoad = { ok: true; state: ProjectState | null; revision: string | null } | { ok: false; error: string }
type SaveResult = { ok: true } | { ok: false; error: string }
type FileSave = { ok: true; revision: string | null } | { ok: false; error: string }

const REQUEST_TIMEOUT_MS = 15000

export function loadFromLocalStorage(): ProjectState | null {
  if (typeof window === 'undefined') {
    return null
  }
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    return raw ? parseProject(JSON.parse(raw)) : null
  } catch {
    return null
  }
}

export async function loadFromFile(): Promise<FileLoad> {
  if (typeof window === 'undefined') {
    return { ok: false, error: 'Window is not available' }
  }
  try {
    const resp = await fetch('/api/project', { cache: 'no-store', signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
    const json: unknown = await resp.json()
    if (!(resp.ok && isRecord(json) && json.ok === true)) {
      return { ok: false, error: errorOf(json) || `HTTP ${resp.status}` }
    }
    const revision = resp.headers.get('etag')
    if (!json.state) {
      return { ok: true, state: null, revision }
    }
    return { ok: true, state: parseProject(json.state), revision }
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'Project file could not be loaded' }
  }
}

function errorOf(json: unknown): string {
  return isRecord(json) && typeof json.error === 'string' ? json.error : ''
}

export function saveToLocalStorage(state: ProjectState): SaveResult {
  if (typeof window === 'undefined') {
    return { ok: true }
  }
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
    return { ok: true }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) }
  }
}

export async function saveToFile(state: ProjectState, revision: string | null): Promise<FileSave> {
  try {
    const resp = await fetch('/api/project', {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...(revision ? { 'if-match': revision } : {}) },
      body: JSON.stringify(state),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    })
    const json: unknown = await resp.json()
    if (!(resp.ok && isRecord(json) && json.ok === true)) {
      return { ok: false, error: errorOf(json) || `HTTP ${resp.status}` }
    }
    return { ok: true, revision: resp.headers.get('etag') }
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) }
  }
}
