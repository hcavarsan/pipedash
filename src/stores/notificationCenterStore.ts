import { create } from 'zustand'

import type { HealthLevel } from '../types'
import type { EventPayloadMap } from '../types/events'

export const MAX_CENTER_NOTIFICATIONS = 50

type AlertLink = EventPayloadMap['provider-health-alert']['link']

export type CenterNotificationTone = 'danger' | 'warning' | 'success'

export interface CenterNotification {
  id: string
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

export type NotificationInput = Omit<CenterNotification, 'id' | 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; id: string }

interface NotificationCenterState {
  items: CenterNotification[]
  selection: CenterSelection | null
  push: (input: NotificationInput) => string
  markAllRead: () => void
  clear: () => void
  open: (selection: CenterSelection) => void
  close: () => void
}

let sequence = 0

export const useNotificationCenterStore = create<NotificationCenterState>()((set) => ({
  items: [],
  selection: null,

  push: (input) => {
    sequence += 1
    const item: CenterNotification = { ...input, id: `${input.key}#${sequence}`, read: false }

    set((state) => ({
      items: [item, ...state.items.filter((existing) => existing.key !== input.key)].slice(0, MAX_CENTER_NOTIFICATIONS),
    }))

    return item.id
  },

  markAllRead: () =>
    set((state) =>
      state.items.every((item) => item.read) ? state : { items: state.items.map((item) => ({ ...item, read: true })) }
    ),

  clear: () => set({ items: [] }),

  open: (selection) =>
    set((state) => ({
      selection,
      items:
        selection.type === 'notification'
          ? state.items.map((item) => (item.id === selection.id ? { ...item, read: true } : item))
          : state.items,
    })),

  close: () => set({ selection: null }),
}))
