import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

import type { NotificationPreferenceSnapshot, PipelineNotifyState } from '../types/notification_preferences'
import type { NotificationPreferences, PreferenceKind, WatchOnce } from '../types/notifications'
import { isRecord } from '../utils/isRecord'

type SourceLists = Pick<
  NotificationPreferences,
  'muted_providers' | 'muted_pipelines' | 'watched_pipelines' | 'watch_once'
>

interface NotificationStoreState {
  notificationsEnabled: boolean
  notifyTriggeredRuns: boolean
  preferences: NotificationPreferences
  pipelineStates: Record<string, PipelineNotifyState>
  source?: string | undefined
  savedLists: Record<string, SourceLists>
}

interface NotificationStoreActions {
  setNotificationsEnabled: (enabled: boolean) => void
  setNotifyTriggeredRuns: (notify: boolean) => void
  applySnapshot: (snapshot: NotificationPreferenceSnapshot) => void
  markFired: (watches: WatchOnce[]) => void
  adoptPreferences: (preferences: NotificationPreferences) => void
  selectSource: (source: string) => void
}

type NotificationStore = NotificationStoreState & NotificationStoreActions

export const PREFERENCE_KINDS = ['failed', 'degraded', 'long_run', 'outage', 'recovered'] as const

export const DEFAULT_NOTIFICATION_PREFERENCES: NotificationPreferences = {
  failed: true,
  degraded: true,
  long_run: true,
  outage: true,
  recovered: true,
  muted_providers: [],
  muted_pipelines: [],
  watched_pipelines: [],
  watch_once: [],
}

const NO_LISTS: SourceLists = { muted_providers: [], muted_pipelines: [], watched_pipelines: [], watch_once: [] }

const initialState: NotificationStoreState = {
  notificationsEnabled: false,
  notifyTriggeredRuns: true,
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
  pipelineStates: {},
  savedLists: {},
}

const isString = (item: unknown): item is string => typeof item === 'string'

const isDate = (item: unknown): item is string => isString(item) && Number.isFinite(Date.parse(item))

const isWatchOnce = (item: unknown): item is WatchOnce =>
  isRecord(item) && isString(item.pipeline_id) && isDate(item.since)

function readPreferences(value: unknown): NotificationPreferences {
  const saved: Record<string, unknown> = isRecord(value) ? value : {}
  const flag = (kind: PreferenceKind) => saved[kind] !== false
  const list = <T>(key: string, keep: (item: unknown) => item is T): T[] => {
    const items = saved[key]

    return Array.isArray(items) ? items.filter(keep) : []
  }

  return {
    failed: flag('failed'),
    degraded: flag('degraded'),
    long_run: flag('long_run'),
    outage: flag('outage'),
    recovered: flag('recovered'),
    muted_providers: list('muted_providers', (item): item is number => Number.isInteger(item)),
    muted_pipelines: list('muted_pipelines', isString),
    watched_pipelines: list('watched_pipelines', isString),
    watch_once: list('watch_once', isWatchOnce).map(({ pipeline_id, since, fired_at }) =>
      isDate(fired_at) ? { pipeline_id, since, fired_at } : { pipeline_id, since }
    ),
  }
}

export function migrateNotificationSettings(persisted: unknown, version: number): NotificationStoreState {
  const saved: Record<string, unknown> = isRecord(persisted) ? persisted : {}
  const preferences = readPreferences(saved.preferences)

  if (version >= 1) {
    return {
      notificationsEnabled: saved.notificationsEnabled === true,
      notifyTriggeredRuns: true,
      preferences,
      pipelineStates: {},
      savedLists: {},
    }
  }

  return {
    notificationsEnabled: saved.failureNotificationsEnabled === true || saved.providerAlertsEnabled === true,
    notifyTriggeredRuns: true,
    preferences,
    pipelineStates: {},
    savedLists: {},
  }
}

const sameWatch = (a: WatchOnce, b: WatchOnce): boolean =>
  a.pipeline_id === b.pipeline_id && Date.parse(a.since) === Date.parse(b.since)

function markWatchFired(watch: WatchOnce, reported: WatchOnce[]): WatchOnce {
  if (watch.fired_at) {
    return watch
  }

  const fired = reported.find((item) => sameWatch(item, watch))

  return fired ? { ...watch, fired_at: fired.fired_at ?? new Date().toISOString() } : watch
}

function switchSource(state: NotificationStoreState, source: string): Partial<NotificationStoreState> {
  if (state.source === undefined) {
    return { source }
  }
  if (state.source === source) {
    return {}
  }

  const { [source]: restored = NO_LISTS, ...savedLists } = state.savedLists
  const { muted_providers, muted_pipelines, watched_pipelines, watch_once } = state.preferences

  return {
    source,
    pipelineStates: {},
    savedLists: { ...savedLists, [state.source]: { muted_providers, muted_pipelines, watched_pipelines, watch_once } },
    preferences: { ...state.preferences, ...restored },
  }
}

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setNotificationsEnabled: (enabled) => set({ notificationsEnabled: enabled }),

        setNotifyTriggeredRuns: (notify) => set({ notifyTriggeredRuns: notify }),

        applySnapshot: ({ preferences, pipeline_states }) =>
          set({ preferences: readPreferences(preferences), pipelineStates: pipeline_states }),

        markFired: (watches) =>
          set(({ preferences }) => {
            const watch_once = preferences.watch_once.map((watch) => markWatchFired(watch, watches))

            return watch_once.some((watch, index) => watch !== preferences.watch_once[index])
              ? { preferences: { ...preferences, watch_once } }
              : {}
          }),

        adoptPreferences: (preferences) => set({ preferences: readPreferences(preferences) }),

        selectSource: (source) => set((state) => switchSource(state, source)),
      }),
      {
        name: 'pipedash-notifications',
        version: 2,
        migrate: migrateNotificationSettings,
        merge: (persisted, current) => {
          const saved = isRecord(persisted) ? persisted : {}

          return { ...current, ...saved, preferences: readPreferences(saved.preferences) }
        },
        partialize: (state) => ({
          notificationsEnabled: state.notificationsEnabled,
          notifyTriggeredRuns: state.notifyTriggeredRuns,
          preferences: state.preferences,
          pipelineStates: state.pipelineStates,
          source: state.source,
          savedLists: state.savedLists,
        }),
      }
    ),
    { name: 'NotificationStore' }
  )
)
