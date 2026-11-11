import {
  isCancelledError,
  type QueryClient,
  queryOptions,
  type UseMutationResult,
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
import { type CenterSelection, useNotificationCenterStore } from '../../../stores/notificationCenterStore'
import type { CenterNotification } from '../../../types/notifications'
import { displayErrorNotification, displaySuccessNotification } from '../../../utils/errorDisplay'
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

function bulkPending(action: { type: 'read'; key: string | null } | { type: 'clear' }): 'markAll' | 'clear' | null {
  if (action.type === 'clear') {
    return 'clear'
  }

  return action.key === null ? 'markAll' : null
}

interface NotificationItems {
  items: CenterNotification[]
  status: 'pending' | 'error' | 'success'
  error: Error | null
  refetch: () => void
}

export function useNotificationItems(): NotificationItems {
  const { data, status, error, refetch } = useQuery(snapshotOptions)
  const localItems = useNotificationCenterStore((state) => state.items)

  if (!isDesktopTauri()) {
    return { items: localItems, status: 'success', error: null, refetch: () => undefined }
  }

  return { items: data?.items ?? emptyItems, status, error, refetch: () => void refetch() }
}

export function useNotificationActions(): {
  open: (selection: CenterSelection) => void
  markRead: (key: string) => void
  markAllRead: () => void
  clear: () => void
  pending: 'markAll' | 'clear' | null
} {
  const client = useQueryClient()
  const { mutate, isPending, variables } = useMutation({
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
  const storePending = useNotificationCenterStore((state) => state.pending)
  const desktopPending = isPending && variables ? bulkPending(variables) : null
  const pending = isDesktopTauri() ? desktopPending : storePending
  const markRead = useCallback(
    (key: string) => {
      if (isDesktopTauri()) {
        mutate({ type: 'read', key })
      } else {
        void useNotificationCenterStore.getState().markRead(key)
      }
    },
    [mutate]
  )
  const markAllRead = useCallback(() => {
    if (pending) {
      return
    }

    if (isDesktopTauri()) {
      mutate({ type: 'read', key: null })
    } else {
      void useNotificationCenterStore.getState().markAllRead()
    }
  }, [mutate, pending])
  const clear = useCallback(() => {
    if (pending) {
      return
    }

    if (isDesktopTauri()) {
      mutate({ type: 'clear' })
    } else {
      void useNotificationCenterStore.getState().clear()
    }
  }, [mutate, pending])
  const open = useCallback(
    (selection: CenterSelection) => {
      useNotificationCenterStore.getState().open(selection)
      if (isDesktopTauri() && selection.type === 'notification') {
        markRead(selection.key)
      }
    },
    [markRead]
  )

  return { open, markRead, markAllRead, clear, pending }
}

export function useSendTestDesktopNotification(): UseMutationResult<void, Error, void> {
  return useMutation<void, Error, void>({
    mutationFn: () => desktopService.testNotification(),
    onSuccess: () =>
      displaySuccessNotification('Your operating system decides whether to display it.', 'Notification submitted'),
  })
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
        if (isCancelledError(error)) {
          return
        }

        logger.warn('Desktop', "Couldn't load desktop notifications", error)

        if (active && !remove) {
          refreshDesktopNotifications(client).catch(() => undefined)
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
