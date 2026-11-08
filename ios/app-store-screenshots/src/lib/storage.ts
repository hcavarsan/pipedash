'use client'
import { useCallback, useEffect, useRef, useState } from 'react'
import { DEFAULT_PROJECT } from './defaults'
import { normalizeOrientation } from './project-migration'
import { loadFromFile, loadFromLocalStorage, saveToFile, saveToLocalStorage } from './project-persistence'
import type { Device, ProjectState } from './types'

const HISTORY_LIMIT = 50
// Coalesce rapid edits (typing, slider drags) into a single undo step.
const COALESCE_MS = 500
// Debounce file/localStorage writes — frequent enough to feel instant, infrequent enough not to thrash disk.
const SAVE_DEBOUNCE_MS = 600

type Updater = ProjectState | ((prev: ProjectState) => ProjectState)
type SetStateOptions = { history?: boolean; coalesce?: boolean }

export type UseProject = {
  state: ProjectState
  setState: (updater: Updater, options?: SetStateOptions) => void
  hydrated: boolean
  savedAt: number | null
  saveError: string | null
  retrySave: (() => void) | undefined
  reset: () => void
  resetDevice: (device: Device) => void
  undo: () => void
  redo: () => void
  canUndo: boolean
  canRedo: boolean
}

function applyUpdater(updater: Updater, prev: ProjectState): ProjectState {
  return typeof updater === 'function' ? updater(prev) : updater
}

export function useProject(): UseProject {
  const [state, _setState] = useState<ProjectState>(DEFAULT_PROJECT)
  const [hydrated, setHydrated] = useState(false)
  const [fileReady, setFileReady] = useState(false)
  const [savedAt, setSavedAt] = useState<number | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const stateRef = useRef(state)
  const saveQueue = useRef(Promise.resolve())
  const saveRevision = useRef(0)
  const fileRevision = useRef<string | null>(null)
  const unsaved = useRef(false)
  const mounted = useRef(false)

  // Run updates once, outside React render/updater replay. Async callbacks also
  // see the latest state before React has committed the next render. Every
  // path (edit, undo, redo, hydrate) funnels through here, so orientation is
  // normalised once, before the state is published.
  const commit = useCallback((next: ProjectState) => {
    const normalized = normalizeOrientation(next)
    stateRef.current = normalized
    _setState(normalized)
  }, [])

  // History stacks live in refs — they don't drive any rendered UI, so
  // mutating them never needs to re-render.
  const pastRef = useRef<ProjectState[]>([])
  const futureRef = useRef<ProjectState[]>([])
  const lastPushAt = useRef(0)

  // Async save completions only touch React state while mounted.
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      if (timer.current) {
        clearTimeout(timer.current)
      }
    }
  }, [])

  // Hydrate: prefer file (git-tracked) → localStorage (cache) → defaults.
  // localStorage is consulted first for instant paint, then file overwrites if present.
  useEffect(() => {
    let cancelled = false
    const cached = loadFromLocalStorage()
    if (cached) {
      commit(cached)
    }

    void (async () => {
      const fromFile = await loadFromFile()
      if (cancelled) {
        return
      }
      if (fromFile.ok) {
        fileRevision.current = fromFile.revision
        commit(fromFile.state ?? DEFAULT_PROJECT)
        setFileReady(true)
      } else {
        setFileReady(false)
        setSaveError(fromFile.error)
      }
      pastRef.current = []
      futureRef.current = []
      lastPushAt.current = 0
      setHydrated(true)
    })()

    return () => {
      cancelled = true
    }
  }, [commit])

  // A refresh inside the debounce window must not silently discard edits.
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent): void => {
      if (!unsaved.current) {
        return
      }
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [])

  // Debounced save to BOTH localStorage (fast, offline) and file (git-trackable).
  const scheduleSave = useCallback((snapshot: ProjectState) => {
    if (timer.current) {
      clearTimeout(timer.current)
    }
    const revision = ++saveRevision.current
    setSavedAt(null)
    timer.current = setTimeout(() => {
      const localResult = saveToLocalStorage(snapshot)
      // Serialize requests so a slow older write cannot overwrite a newer edit.
      saveQueue.current = saveQueue.current.then(async () => {
        if (revision !== saveRevision.current) {
          return
        }
        const fileResult = await saveToFile(snapshot, fileRevision.current)
        // Advance even if another edit arrived while this request was in flight.
        if (fileResult.ok) {
          fileRevision.current = fileResult.revision
        }
        if (revision !== saveRevision.current || !mounted.current) {
          return
        }
        if (!fileResult.ok) {
          setSaveError(`File save failed: ${fileResult.error}`)
          return
        }
        unsaved.current = false
        setSavedAt(Date.now())
        setSaveError(localResult.ok ? null : localResult.error)
      })
    }, SAVE_DEBOUNCE_MS)
  }, [])

  useEffect(() => {
    if (hydrated && fileReady) {
      scheduleSave(state)
    }
  }, [state, hydrated, fileReady, scheduleSave])

  const retrySave = useCallback(() => scheduleSave(stateRef.current), [scheduleSave])

  // `history: false` is for navigation (device, orientation, locale): it isn't
  // an edit, so it neither takes an undo step nor clears redo. Undo still
  // restores the snapshot's device, which takes you to the deck the undone
  // edit was made on.
  // Structural actions form their own step and end the typing/slider group.
  const setState = useCallback(
    (updater: Updater, options?: SetStateOptions) => {
      const prev = stateRef.current
      const next = applyUpdater(updater, prev)
      if (next === prev) {
        return
      }
      unsaved.current = true
      if (options?.history === false) {
        // Navigation is a boundary: edits to another deck must not coalesce.
        lastPushAt.current = 0
      } else {
        const now = Date.now()
        if (options?.coalesce === false || now - lastPushAt.current > COALESCE_MS) {
          pastRef.current.push(prev)
          if (pastRef.current.length > HISTORY_LIMIT) {
            pastRef.current.shift()
          }
        }
        futureRef.current.length = 0
        lastPushAt.current = options?.coalesce === false ? 0 : now
      }
      commit(next)
    },
    [commit]
  )

  const undo = useCallback(() => {
    const prev = pastRef.current.pop()
    if (prev === undefined) {
      return
    }
    futureRef.current.push(stateRef.current)
    unsaved.current = true
    lastPushAt.current = 0
    commit(prev)
  }, [commit])

  const redo = useCallback(() => {
    const next = futureRef.current.pop()
    if (next === undefined) {
      return
    }
    pastRef.current.push(stateRef.current)
    unsaved.current = true
    lastPushAt.current = 0
    commit(next)
  }, [commit])

  // "Reset all devices" resets the decks only. App name, theme, font, icon and
  // especially `locales` (which has no editor UI) are project settings.
  const reset = useCallback(() => {
    setState((prev) => ({ ...prev, slidesByDevice: DEFAULT_PROJECT.slidesByDevice }), { coalesce: false })
  }, [setState])

  const resetDevice = useCallback(
    (device: Device) => {
      setState(
        (prev) => ({
          ...prev,
          slidesByDevice: {
            ...prev.slidesByDevice,
            [device]: DEFAULT_PROJECT.slidesByDevice[device],
          },
        }),
        { coalesce: false }
      )
    },
    [setState]
  )

  return {
    state,
    setState,
    hydrated,
    savedAt,
    saveError,
    retrySave: fileReady ? retrySave : undefined,
    reset,
    resetDevice,
    undo,
    redo,
    canUndo: pastRef.current.length > 0,
    canRedo: futureRef.current.length > 0,
  }
}
