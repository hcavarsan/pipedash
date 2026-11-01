import {
  isCancelledError,
  type QueryClient,
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect } from 'react'

import { logger } from '../../../lib/logger'
import {
  DESKTOP_NOTIFICATIONS_EVENT,
  type DesktopNotificationSnapshot,
  desktopService,
} from '../../../services/desktop'
import {
  type CenterNotification,
  type CenterSelection,
  useNotificationCenterStore,
} from '../../../stores/notificationCenterStore'
import { displayErrorNotification } from '../../../utils/errorDisplay'
import { isRecord } from '../../../utils/isRecord'
import { isDesktopTauri } from '../../../utils/platform'

export const desktopNotificationsKey = ['desktop', 'notifications'] as const
const emptyItems: CenterNotification[] = []

function newestSnapshot<T>(current: T | undefined, snapshot: T): T {
  return isRecord(current) &&
    isRecord(snapshot) &&
    typeof current.revision === 'number' &&
    typeof snapshot.revision === 'number' &&
    current.revision > snapshot.revision
    ? current
    : snapshot
}

const snapshotOptions = queryOptions({
  queryKey: desktopNotificationsKey,
  queryFn: () => desktopService.getNotifications(),
  enabled: false,
  staleTime: Number.POSITIVE_INFINITY,
  retry: false,
  structuralSharing: newestSnapshot,
})

export function refreshDesktopNotifications(client: QueryClient): Promise<DesktopNotificationSnapshot> {
  return client.fetchQuery({ ...snapshotOptions, staleTime: 0 })
}

export function useNotificationItems(): CenterNotification[] {
  const { data } = useQuery(snapshotOptions)
  const localItems = useNotificationCenterStore((state) => state.items)

  return isDesktopTauri() ? (data?.items ?? emptyItems) : localItems
}

export function useNotificationActions(): {
  open: (selection: CenterSelection) => void
  markRead: (key: string) => void
  markAllRead: () => void
  clear: () => void
} {
  const client = useQueryClient()
  const { mutate } = useMutation({
    mutationFn: (action: { type: 'read'; key: string | null } | { type: 'clear' }) =>
      action.type === 'read' ? desktopService.markNotificationsRead(action.key) : desktopService.clearNotifications(),
    onMutate: () => client.getQueryCache().find({ queryKey: desktopNotificationsKey, exact: true }),
    onSuccess: (snapshot, _action, query) => {
      if (query && client.getQueryCache().find({ queryKey: desktopNotificationsKey, exact: true }) === query) {
        client.setQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey, (current) =>
          newestSnapshot(current, snapshot)
        )
      }
    },
    onError: (error) => displayErrorNotification(error, "Couldn't update notifications"),
  })
  const markRead = useCallback(
    (key: string) => {
      if (isDesktopTauri()) {
        mutate({ type: 'read', key })
      } else {
        useNotificationCenterStore.getState().markRead(key)
      }
    },
    [mutate]
  )
  const markAllRead = useCallback(() => {
    if (isDesktopTauri()) {
      mutate({ type: 'read', key: null })
    } else {
      useNotificationCenterStore.getState().markAllRead()
    }
  }, [mutate])
  const clear = useCallback(() => {
    if (isDesktopTauri()) {
      mutate({ type: 'clear' })
    } else {
      useNotificationCenterStore.getState().clear()
    }
  }, [mutate])
  const open = useCallback(
    (selection: CenterSelection) => {
      useNotificationCenterStore.getState().open(selection)
      if (isDesktopTauri() && selection.type === 'notification') {
        markRead(selection.key)
      }
    },
    [markRead]
  )

  return { open, markRead, markAllRead, clear }
}

export function useDesktopNotifications(): void {
  const client = useQueryClient()

  useEffect(() => {
    if (!isDesktopTauri()) {
      return
    }
    let active = true
    let remove: (() => void) | undefined
    const apply = (snapshot: DesktopNotificationSnapshot) => {
      if (active) {
        client.setQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey, (current) =>
          newestSnapshot(current, snapshot)
        )
      }
    }

    listen<DesktopNotificationSnapshot>(DESKTOP_NOTIFICATIONS_EVENT, ({ payload }) => apply(payload))
      .then(async (unlisten) => {
        if (!active) {
          unlisten()
          return
        }
        remove = unlisten
        await refreshDesktopNotifications(client)
      })
      .catch((error: unknown) => {
        if (!isCancelledError(error)) {
          logger.warn('Desktop', "Couldn't load desktop notifications", error)
        }
      })

    return () => {
      active = false
      remove?.()
      client.removeQueries({ queryKey: desktopNotificationsKey, exact: true })
      useNotificationCenterStore.getState().close()
    }
  }, [client])
}
