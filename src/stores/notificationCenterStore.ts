import { create } from 'zustand'

import { type DesktopNotificationSnapshot, desktopService } from '../services/desktop'
import type { HealthLevel } from '../types/domain'
import type { EventPayloadMap } from '../types/events'
import { displayErrorNotification } from '../utils/errorDisplay'
import { isDesktopTauri } from '../utils/platform'

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
  runNumber?: number | null
}

export type NotificationInput = Omit<CenterNotification, 'read'>

export type CenterSelection = { type: 'provider'; providerId: number } | { type: 'notification'; key: string }

interface NotificationCenterState {
  items: CenterNotification[]
  desktopRevision: number
  applyDesktopSnapshot: (snapshot: DesktopNotificationSnapshot) => void
  selection: CenterSelection | null
  push: (input: NotificationInput) => void
  markAllRead: () => void
  markRead: (key: string) => void
  clear: () => void
  open: (selection: CenterSelection) => void
  close: () => void
}

const nativeChange = (request: Promise<DesktopNotificationSnapshot>): void => {
  request
    .then((snapshot) => useNotificationCenterStore.getState().applyDesktopSnapshot(snapshot))
    .catch((error: unknown) => displayErrorNotification(error, "Couldn't update notifications"))
}

export const useNotificationCenterStore = create<NotificationCenterState>()((set, get) => ({
  items: [],
  selection: null,
  desktopRevision: -1,
  applyDesktopSnapshot: (snapshot) =>
    set((state) =>
      snapshot.revision < state.desktopRevision ? state : { items: snapshot.items, desktopRevision: snapshot.revision }
    ),

  push: (input) =>
    set((state) => ({
      items: [{ ...input, read: false }, ...state.items.filter((existing) => existing.key !== input.key)].slice(
        0,
        MAX_CENTER_NOTIFICATIONS
      ),
    })),

  markAllRead: () => {
    if (isDesktopTauri()) {
      nativeChange(desktopService.markNotificationsRead(null))
      return
    }
    set((state) =>
      state.items.every((item) => item.read) ? state : { items: state.items.map((item) => ({ ...item, read: true })) }
    )
  },

  markRead: (key) => {
    if (isDesktopTauri()) {
      nativeChange(desktopService.markNotificationsRead(key))
      return
    }
    set((state) => ({ items: state.items.map((item) => (item.key === key ? { ...item, read: true } : item)) }))
  },

  clear: () => {
    if (isDesktopTauri()) {
      nativeChange(desktopService.clearNotifications())
      return
    }
    set({ items: [] })
  },

  open: (selection) => {
    set({ selection })
    if (selection.type === 'notification') {
      get().markRead(selection.key)
    }
  },

  close: () => set({ selection: null }),
}))
