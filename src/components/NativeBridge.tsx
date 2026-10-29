import { listen } from '@tauri-apps/api/event'
import { type ReactNode, useEffect } from 'react'
import { type NavigateFunction, useNavigate } from 'react-router-dom'

import { isMobileShell } from '../hooks/useIsMobile'
import { logger } from '../lib/logger'
import { isTauri } from '../services'
import { mobileService, NATIVE_INTENT_EVENT, STATUS_BAR_TAP_EVENT } from '../services/mobile'
import { useModalStore } from '../stores/modalStore'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import type { NativeIntent } from '../types'
import { isMobilePlatform } from '../utils/platform'
import { openNotificationDetails } from '../utils/systemNotifications'

import { scrollActiveToTop } from './layout/scrollToTop'

const NOTIFICATIONS = '/notifications'

function openPipeline(navigate: NavigateFunction, pipelineId: string, from: string, runNumber?: number) {
  void navigate(`/pipelines/${encodeURIComponent(pipelineId)}`, { state: { from } })
  if (runNumber) {
    useModalStore.getState().openLogsModal(pipelineId, runNumber)
  }
}

function handleIntent(navigate: NavigateFunction, intent: NativeIntent) {
  if (intent.type === 'link') {
    if (intent.path.startsWith('/pipelines/')) {
      openPipeline(
        navigate,
        decodeURIComponent(intent.path.slice('/pipelines/'.length)),
        '/pipelines',
        intent.runNumber
      )
    } else {
      void navigate(intent.path)
    }

    return
  }

  const known = intent.key && useNotificationCenterStore.getState().items.some((item) => item.key === intent.key)

  if (intent.action !== 'open-run' && intent.key && known) {
    if (isMobileShell()) {
      void navigate(NOTIFICATIONS)
    }
    openNotificationDetails(intent.key)

    return
  }

  if (intent.pipelineId) {
    openPipeline(navigate, intent.pipelineId, NOTIFICATIONS, intent.runNumber)

    return
  }

  if (isMobileShell()) {
    void navigate(NOTIFICATIONS)
  }
}

function listenNative(event: string, onEvent: () => void): () => void {
  let stop: (() => void) | undefined
  let active = true

  listen(event, onEvent)
    .then((unlisten) => {
      if (active) {
        stop = unlisten
      } else {
        unlisten()
      }
    })
    .catch((error: unknown) => logger.warn('NativeBridge', `Couldn't listen for ${event}`, error))

  return () => {
    active = false
    stop?.()
  }
}

export function NativeBridge(): ReactNode {
  const navigate = useNavigate()

  useEffect(() => {
    if (!isTauri()) {
      return
    }

    const drain = () => {
      mobileService
        .takePendingIntent()
        .then((intent) => intent && handleIntent(navigate, intent))
        .catch((error: unknown) => logger.warn('NativeBridge', "Couldn't read the pending intent", error))
    }

    drain()
    const stopIntents = listenNative(NATIVE_INTENT_EVENT, drain)
    const stopTaps = listenNative(STATUS_BAR_TAP_EVENT, scrollActiveToTop)

    return () => {
      stopIntents()
      stopTaps()
    }
  }, [navigate])

  useEffect(() => {
    if (!isMobilePlatform()) {
      return
    }

    const unread = (items: { read: boolean }[]) => items.filter((item) => !item.read).length
    let last = unread(useNotificationCenterStore.getState().items)

    mobileService.setBadge(last).catch(() => undefined)

    return useNotificationCenterStore.subscribe((state) => {
      const count = unread(state.items)
      if (count !== last) {
        last = count
        mobileService.setBadge(count).catch(() => undefined)
      }
    })
  }, [])

  return null
}
