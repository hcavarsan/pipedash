import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

import type { AlertKind, NotificationPreferences, PreferenceKind } from '../types/domain'
import { isRecord } from '../utils/isRecord'

interface NotificationStoreState {
  notificationsEnabled: boolean
  preferences: NotificationPreferences
  pendingRemap?: Record<number, number> | undefined
}

interface NotificationStoreActions {
  setNotificationsEnabled: (enabled: boolean) => void
  setKindEnabled: (kind: PreferenceKind, enabled: boolean) => void
  setProviderMuted: (providerId: number, muted: boolean) => void
  setPipelineMuted: (pipelineId: string, muted: boolean) => void
  remapProviders: (mapping: Record<number, number>) => void
  remapProvidersOnRestart: (mapping: Record<number, number>) => void
  applyPendingRemap: () => void
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
}

const initialState: NotificationStoreState = {
  notificationsEnabled: false,
  preferences: DEFAULT_NOTIFICATION_PREFERENCES,
}

const toggled = <T>(list: T[], item: T, present: boolean): T[] => {
  const rest = list.filter((existing) => existing !== item)

  return present ? [...rest, item] : rest
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
    muted_pipelines: list('muted_pipelines', (item): item is string => typeof item === 'string'),
  }
}

export function migrateNotificationSettings(persisted: unknown, version: number): NotificationStoreState {
  const saved: Record<string, unknown> = isRecord(persisted) ? persisted : {}
  const preferences = readPreferences(saved.preferences)

  if (version >= 1) {
    return { notificationsEnabled: saved.notificationsEnabled === true, preferences }
  }

  return {
    notificationsEnabled: saved.failureNotificationsEnabled === true || saved.providerAlertsEnabled === true,
    preferences,
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
  }
}

export function notificationAllowed(
  preferences: NotificationPreferences,
  kind: AlertKind,
  providerIds: number[],
  pipelineId: string | null
): boolean {
  if (kind === 'test') {
    return true
  }

  const providerMuted = providerIds.length > 0 && providerIds.every((id) => preferences.muted_providers.includes(id))
  const pipelineMuted = pipelineId !== null && preferences.muted_pipelines.includes(pipelineId)

  return preferences[kind] && !providerMuted && !pipelineMuted
}

export const shouldNotify = (kind: AlertKind, providerIds: number[], pipelineId: string | null): boolean =>
  notificationAllowed(useNotificationStore.getState().preferences, kind, providerIds, pipelineId)

export const useNotificationStore = create<NotificationStore>()(
  devtools(
    persist(
      (set, get) => ({
        ...initialState,

        setNotificationsEnabled: (enabled) => set({ notificationsEnabled: enabled }),

        setKindEnabled: (kind, enabled) => set((state) => ({ preferences: { ...state.preferences, [kind]: enabled } })),

        setProviderMuted: (providerId, muted) =>
          set((state) => ({
            preferences: {
              ...state.preferences,
              muted_providers: toggled(state.preferences.muted_providers, providerId, muted),
            },
          })),

        setPipelineMuted: (pipelineId, muted) =>
          set((state) => ({
            preferences: {
              ...state.preferences,
              muted_pipelines: toggled(state.preferences.muted_pipelines, pipelineId, muted),
            },
          })),

        remapProviders: (mapping) =>
          set((state) => ({ preferences: remapPreferences(state.preferences, mapping), pendingRemap: undefined })),

        remapProvidersOnRestart: (mapping) => set({ pendingRemap: mapping }),

        applyPendingRemap: () => {
          const { pendingRemap, remapProviders } = get()

          if (pendingRemap) {
            remapProviders(pendingRemap)
          }
        },
      }),
      {
        name: 'pipedash-notifications',
        version: 2,
        migrate: migrateNotificationSettings,
        partialize: (state) => ({
          notificationsEnabled: state.notificationsEnabled,
          preferences: state.preferences,
          pendingRemap: state.pendingRemap,
        }),
        onRehydrateStorage: () => (state) => state?.applyPendingRemap(),
      }
    ),
    { name: 'NotificationStore' }
  )
)
