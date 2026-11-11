import type { NotificationPreferences, PreferenceKind } from './notifications'

export type PreferenceChange =
  | { type: 'kind'; kind: PreferenceKind; enabled: boolean }
  | { type: 'provider_muted'; provider_id: number; muted: boolean }
  | { type: 'pipeline_muted'; pipeline_id: string; muted: boolean }
  | { type: 'pipeline_watched'; pipeline_id: string; watched: boolean }
  | { type: 'watch_next_finish'; pipeline_id: string }
  | { type: 'cancel_next_finish'; pipeline_id: string }

export type PipelineNotifyState = 'muted' | 'watching' | 'pending' | 'default'

export interface NotificationPreferenceSnapshot {
  preferences: NotificationPreferences
  pipeline_states: Record<string, PipelineNotifyState>
}
