import { Button, Stack, Text } from '@mantine/core'
import { type NotificationData, notifications, notificationsStore } from '@mantine/notifications'
import { IconCircleCheck } from '@tabler/icons-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useRef } from 'react'
import { useNavigate } from 'react-router-dom'

import { logger } from '../lib/logger'
import { events, isTauri } from '../services'
import { useNotificationStore } from '../stores/notificationStore'
import { HEALTH_META } from '../theme/status'
import type { EventPayloadMap } from '../types/events'
import { openExternal } from '../utils/openExternal'
import { showSystemNotification } from '../utils/systemNotifications'

type HealthAlert = EventPayloadMap['provider-health-alert']

const LINK_LABELS = { incident: 'View incident', status_page: 'Open status page' } as const

const OUTAGE_AUTO_CLOSE_MS = 10_000
const RECOVERY_AUTO_CLOSE_MS = 6_000

const isToastVisible = (id: string): boolean => {
  const { notifications: visible, queue } = notificationsStore.getState()

  return [...visible, ...queue].some((toast) => toast.id === id)
}

const showToast = (alert: HealthAlert): void => {
  const color = alert.recovered ? 'green' : HEALTH_META[alert.level].color
  const Icon = alert.recovered ? IconCircleCheck : HEALTH_META[alert.level].icon
  const { link } = alert

  const toast: NotificationData = {
    id: alert.notification_key,
    title: alert.title,
    color,
    icon: <Icon size={16} aria-hidden />,
    autoClose: alert.recovered ? RECOVERY_AUTO_CLOSE_MS : OUTAGE_AUTO_CLOSE_MS,
    message: (
      <Stack gap={6}>
        <Text size="sm">{alert.body}</Text>
        {link && (
          <Button
            size="compact-xs"
            variant="light"
            color={color}
            w="fit-content"
            aria-label={`${LINK_LABELS[link.kind]}: ${alert.title}`}
            onClick={() => {
              void openExternal(link.url)
            }}
          >
            {LINK_LABELS[link.kind]}
          </Button>
        )}
      </Stack>
    ),
  }

  if (isToastVisible(alert.notification_key)) {
    notifications.update(toast)
  } else {
    notifications.show(toast)
  }
}

const focusWindow = (): void => {
  if (!isTauri()) {
    window.focus()

    return
  }

  getCurrentWindow()
    .setFocus()
    .catch((error: unknown) => logger.warn('useProviderHealthAlerts', "Couldn't focus the window", error))
}

export function useProviderHealthAlerts() {
  const navigate = useNavigate()
  const navigateRef = useRef(navigate)

  useEffect(() => {
    navigateRef.current = navigate
  }, [navigate])

  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined

    events
      .listen<HealthAlert>('provider-health-alert', (alert) => {
        if (isTauri() && !navigator.onLine && alert.link === null) {
          return
        }

        showToast(alert)

        if (!useNotificationStore.getState().providerAlertsEnabled) {
          return
        }

        const [providerId] = alert.provider_ids

        showSystemNotification(alert.title, alert.body, () => {
          focusWindow()
          navigateRef.current(providerId === undefined ? '/pipelines' : `/pipelines?provider=${providerId}`)
        })
      })
      .then((fn) => {
        if (active) {
          unlisten = fn
        } else {
          fn()
        }
      })
      .catch((error: unknown) => logger.warn('useProviderHealthAlerts', "Couldn't listen for health alerts", error))

    return () => {
      active = false
      unlisten?.()
    }
  }, [])
}
