import { listen } from '@tauri-apps/api/event'
import { type ReactNode, useEffect, useEffectEvent } from 'react'
import { type NavigateFunction, useNavigate } from 'react-router-dom'

import { usePipelineViewStore } from '../features/pipelines/stores/pipelineViewStore'
import { logger } from '../lib/logger'
import { isTauri } from '../services/isTauri'
import { mobileService, NATIVE_INTENT_EVENT } from '../services/mobile'
import { useModalStore } from '../stores/modalStore'
import { useNotificationCenterStore } from '../stores/notificationCenterStore'
import type { NativeIntent } from '../types/domain'

const NOTIFICATIONS = '/notifications'

async function handleIntent(navigate: NavigateFunction, intent: NativeIntent, active: () => boolean): Promise<void> {
  const openPipeline = async (pipelineId: string, from: string, runNumber?: number | null) => {
    await navigate(`/pipelines/${encodeURIComponent(pipelineId)}`, { state: { from } })
    if (active() && runNumber) {
      useModalStore.getState().openLogsModal(pipelineId, runNumber)
    }
  }

  if (intent.type === 'ui') {
    await navigate(intent.action === 'managePins' ? '/pipelines?view=pinned' : '/pipelines')
    if (active()) {
      if (intent.action === 'managePins') {
        usePipelineViewStore.getState().openEditor('pinned')
      } else {
        usePipelineViewStore.getState().openManage()
      }
    }
    return
  }

  if (intent.type === 'link') {
    const url = new URL(intent.path, 'https://pipedash.invalid')
    const detail = /^\/pipelines\/([^/]+)\/?$/.exec(url.pathname)
    await navigate(`${url.pathname}${url.search}${url.hash}`)
    if (active() && detail?.[1] && intent.runNumber) {
      useModalStore.getState().openLogsModal(decodeURIComponent(detail[1]), intent.runNumber)
    }
    return
  }

  const known = useNotificationCenterStore.getState().items.find((item) => item.key === intent.key)
  const pipelineId = intent.pipelineId ?? known?.pipelineId
  const runNumber = intent.runNumber ?? known?.runNumber

  if (pipelineId && runNumber) {
    await openPipeline(pipelineId, NOTIFICATIONS, runNumber)
  } else if (known) {
    await navigate(NOTIFICATIONS)
    if (active()) {
      useNotificationCenterStore.getState().open({ type: 'notification', key: known.key })
    }
  } else if (pipelineId) {
    await openPipeline(pipelineId, NOTIFICATIONS)
  } else {
    await navigate(intent.providerId !== undefined ? '/settings/providers' : NOTIFICATIONS)
  }
}

export function NativeIntentBridge(): ReactNode {
  const navigate = useNavigate()
  const applyIntent = useEffectEvent((intent: NativeIntent, active: () => boolean) =>
    handleIntent(navigate, intent, active)
  )

  useEffect(() => {
    if (!isTauri()) {
      return
    }
    let active = true
    let listening = false
    let draining = false
    let requested = false
    let remove: (() => void) | undefined

    const drain = async (): Promise<void> => {
      requested = true
      if (!active || !listening || draining) {
        return
      }
      draining = true
      try {
        do {
          requested = false
          while (active) {
            const pending = await mobileService.peekPendingIntent()
            if (!active || !pending) {
              break
            }
            await applyIntent(pending.intent, () => active)
            if (!active) {
              return
            }
            await mobileService.ackPendingIntent(pending.id)
          }
        } while (active && requested)
      } catch (error) {
        logger.warn('NativeIntentBridge', "Couldn't handle the pending native action", error)
      } finally {
        draining = false
      }
    }
    const retry = () => {
      if (document.visibilityState !== 'hidden') {
        void drain()
      }
    }

    listen(NATIVE_INTENT_EVENT, () => void drain())
      .then((unlisten) => {
        if (!active) {
          unlisten()
          return
        }
        remove = unlisten
        listening = true
        void drain()
      })
      .catch((error: unknown) => logger.warn('NativeIntentBridge', "Couldn't listen for native actions", error))
    window.addEventListener('focus', retry)
    document.addEventListener('visibilitychange', retry)

    return () => {
      active = false
      remove?.()
      window.removeEventListener('focus', retry)
      document.removeEventListener('visibilitychange', retry)
    }
  }, [])

  return null
}
