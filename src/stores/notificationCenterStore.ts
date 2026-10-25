import { create } from 'zustand'

import type { HealthLevel } from '../types'
import type { EventPayloadMap } from '../types/events'

export const MAX_CENTER_NOTIFICATIONS = 50

type AlertLink = EventPayloadMap['provider-health-alert']['link']

export type CenterNotificationTone = 'danger' | 'warning' | 'success'

export interface CenterNotification {
  key: string
  at: number
  read: boolean
  tone: CenterNotificationTone
  level: HealthLevel | null
  title: string
  body: string
  providerIds: number[]
  pipelineId: string | null
  link: AlertLink
}

export type NotificationInput = Omit<CenterNotification, 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; key: string }

interface NotificationCenterState {
  items: CenterNotification[]
  selection: CenterSelection | null
  push: (input: NotificationInput) => void
  markAllRead: () => void
  markRead: (key: string) => void
  clear: () => void
  open: (selection: CenterSelection) => void
  close: () => void
}

export const useNotificationCenterStore = create<NotificationCenterState>()((set) => ({
  items: [],
  selection: null,

  push: (input) =>
    set((state) => ({
      items: [{ ...input, read: false }, ...state.items.filter((existing) => existing.key !== input.key)].slice(
        0,
        MAX_CENTER_NOTIFICATIONS
      ),
    })),

  markAllRead: () =>
    set((state) =>
      state.items.every((item) => item.read) ? state : { items: state.items.map((item) => ({ ...item, read: true })) }
    ),

  markRead: (key) =>
    set((state) => ({ items: state.items.map((item) => (item.key === key ? { ...item, read: true } : item)) })),

  clear: () => set({ items: [] }),

  open: (selection) =>
    set((state) => ({
      selection,
      items:
        selection.type === 'notification'
          ? state.items.map((item) => (item.key === selection.key ? { ...item, read: true } : item))
          : state.items,
    })),

  close: () => set({ selection: null }),
}))
