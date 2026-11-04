import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

import type { NotificationPreferences, PreferenceKind, WatchOnce } from '../types/domain'
import { isRecord } from '../utils/isRecord'

type SourceLists = Pick<
  NotificationPreferences,
  'muted_providers' | 'muted_pipelines' | 'watched_pipelines' | 'watch_once'
>

const WATCH_ONCE_TTL_MS = 7 * 24 * 60 * 60 * 1000

interface NotificationStoreState {
  notificationsEnabled: boolean
  notifyTriggeredRuns: boolean
  preferences: NotificationPreferences
  pendingRemap?: Record<number, number> | undefined
  source?: string | undefined
  savedLists: Record<string, SourceLists>
}

interface NotificationStoreActions {
  setNotificationsEnabled: (enabled: boolean) => void
  setNotifyTriggeredRuns: (notify: boolean) => void
  setKindEnabled: (kind: PreferenceKind, enabled: boolean) => void
  setProviderMuted: (providerId: number, muted: boolean) => void
  setPipelineMuted: (pipelineId: string, muted: boolean) => void
  setPipelineWatched: (pipelineId: string, watched: boolean) => void
  watchNextFinish: (pipelineId: string) => void
  cancelNextFinish: (pipelineId: string) => void
  markFired: (watches: WatchOnce[]) => void
  adoptPreferences: (preferences: NotificationPreferences) => void
  dropStaleWatches: () => void
  remapProviders: (mapping: Record<number, number>) => void
  remapProvidersOnRestart: (mapping: Record<number, number>) => void
  applyPendingRemap: () => void
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
  savedLists: {},
}

const without = <T>(list: T[], item: T): T[] => list.filter((existing) => existing !== item)

const toggled = <T>(list: T[], item: T, present: boolean): T[] => {
  const rest = without(list, item)

  return present ? [...rest, item] : rest
}

const withoutOnce = (watchOnce: WatchOnce[], pipelineId: string): WatchOnce[] =>
  watchOnce.filter((watch) => watch.pipeline_id !== pipelineId)

const isString = (item: unknown): item is string => typeof item === 'string'

const isDate = (item: unknown): item is string => isString(item) && Number.isFinite(Date.parse(item))

const isWatchOnce = (item: unknown): item is WatchOnce =>
  isRecord(item) && isString(item.pipeline_id) && isDate(item.since)

export const pendingWatches = (preferences: NotificationPreferences): WatchOnce[] =>
  preferences.watch_once.filter((watch) => !watch.fired_at)

const withoutStaleWatches = <T extends Pick<SourceLists, 'watch_once'>>(lists: T, now: number): T => {
  const watch_once = lists.watch_once.filter((watch) => now - Date.parse(watch.since) <= WATCH_ONCE_TTL_MS)

  return watch_once.length === lists.watch_once.length ? lists : { ...lists, watch_once }
}

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
      savedLists: {},
    }
  }

  return {
    notificationsEnabled: saved.failureNotificationsEnabled === true || saved.providerAlertsEnabled === true,
    notifyTriggeredRuns: true,
    preferences,
    savedLists: {},
  }
}

export function remapPreferences(
  preferences: NotificationPreferences,
  mapping: Record<number, number>
): NotificationPreferences {
  const pipeline = (id: string): string[] => {
    const [type, provider, ...rest] = id.split('__')
    const next = mapping[Number(provider)]

    return next === undefined || rest.length === 0 ? [] : [[type, next, ...rest].join('__')]
  }

  return {
    ...preferences,
    muted_providers: preferences.muted_providers.flatMap((id) => (mapping[id] === undefined ? [] : [mapping[id]])),
    muted_pipelines: preferences.muted_pipelines.flatMap(pipeline),
    watched_pipelines: preferences.watched_pipelines.flatMap(pipeline),
    watch_once: preferences.watch_once.flatMap((watch) =>
      pipeline(watch.pipeline_id).map((pipeline_id) => ({ ...watch, pipeline_id }))
    ),
  }
}

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set, get) => ({
        ...initialState,

        setNotificationsEnabled: (enabled) => set({ notificationsEnabled: enabled }),

        setNotifyTriggeredRuns: (notify) => set({ notifyTriggeredRuns: notify }),

        setKindEnabled: (kind, enabled) => set((state) => ({ preferences: { ...state.preferences, [kind]: enabled } })),

        setProviderMuted: (providerId, muted) =>
          set((state) => ({
            preferences: {
              ...state.preferences,
              muted_providers: toggled(state.preferences.muted_providers, providerId, muted),
            },
          })),

        setPipelineMuted: (pipelineId, muted) =>
          set(({ preferences }) => ({
            preferences: {
              ...preferences,
              muted_pipelines: toggled(preferences.muted_pipelines, pipelineId, muted),
              ...(muted && {
                watched_pipelines: without(preferences.watched_pipelines, pipelineId),
                watch_once: withoutOnce(preferences.watch_once, pipelineId),
              }),
            },
          })),

        setPipelineWatched: (pipelineId, watched) =>
          set(({ preferences }) => ({
            preferences: {
              ...preferences,
              watched_pipelines: toggled(preferences.watched_pipelines, pipelineId, watched),
              ...(watched && {
                muted_pipelines: without(preferences.muted_pipelines, pipelineId),
                watch_once: withoutOnce(preferences.watch_once, pipelineId),
              }),
            },
          })),

        watchNextFinish: (pipelineId) =>
          set(({ preferences }) => ({
            preferences: {
              ...preferences,
              muted_pipelines: without(preferences.muted_pipelines, pipelineId),
              watch_once: [
                ...withoutOnce(preferences.watch_once, pipelineId),
                { pipeline_id: pipelineId, since: new Date().toISOString() },
              ],
            },
          })),

        cancelNextFinish: (pipelineId) =>
          set(({ preferences }) => ({
            preferences: { ...preferences, watch_once: withoutOnce(preferences.watch_once, pipelineId) },
          })),

        markFired: (watches) =>
          set(({ preferences }) => {
            const watch_once = preferences.watch_once.map((watch) => {
              const fired = watch.fired_at
                ? undefined
                : watches.find(
                    (item) =>
                      item.pipeline_id === watch.pipeline_id && Date.parse(item.since) === Date.parse(watch.since)
                  )

              return fired ? { ...watch, fired_at: fired.fired_at ?? new Date().toISOString() } : watch
            })

            return watch_once.some((watch, index) => watch !== preferences.watch_once[index])
              ? { preferences: { ...preferences, watch_once } }
              : {}
          }),

        adoptPreferences: (preferences) => set({ preferences: readPreferences(preferences) }),

        dropStaleWatches: () => {
          const now = Date.now()

          set((state) => ({
            preferences: withoutStaleWatches(state.preferences, now),
            savedLists: Object.fromEntries(
              Object.entries(state.savedLists).map(([source, lists]) => [source, withoutStaleWatches(lists, now)])
            ),
          }))
        },

        remapProviders: (mapping) =>
          set((state) => ({ preferences: remapPreferences(state.preferences, mapping), pendingRemap: undefined })),

        remapProvidersOnRestart: (mapping) => set({ pendingRemap: mapping }),

        applyPendingRemap: () => {
          const { pendingRemap, remapProviders } = get()

          if (pendingRemap) {
            remapProviders(pendingRemap)
          }
        },

        selectSource: (source) =>
          set((state) => {
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
              savedLists: {
                ...savedLists,
                [state.source]: { muted_providers, muted_pipelines, watched_pipelines, watch_once },
              },
              preferences: { ...state.preferences, ...restored },
            }
          }),
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
          pendingRemap: state.pendingRemap,
          source: state.source,
          savedLists: state.savedLists,
        }),
        onRehydrateStorage: () => (state) => {
          state?.dropStaleWatches()
          state?.applyPendingRemap()
        },
      }
    ),
    { name: 'NotificationStore' }
  )
)
