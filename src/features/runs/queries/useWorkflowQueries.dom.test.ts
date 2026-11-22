import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { NotificationData } from '@mantine/notifications'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({
  width: 390,
  height: 844,
  settings: { navigator: { userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile/15E148' } },
})
Object.assign(window, { __TAURI_INTERNALS__: {}, __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('../../../lib/notifications')
const { mobileService } = await import('../../../services/mobile')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { NOTIFICATION_DURATIONS } = await import('../../../utils/errorDisplay')
const { useTriggerWorkflow } = await import('./useWorkflowQueries')

const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

const copy = {
  label: 'Run',
  compact_label: 'Run',
  title: 'Run workflow',
  success: { title: 'Run started', message: 'build started' },
  error_title: "Couldn't run workflow",
  note: null,
}

afterEach(async () => {
  client.clear()
  await act(async () => cleanup())
})
after(async () => {
  await GlobalRegistrator.unregister()
})

function observe(context: TestContext) {
  const shown: NotificationData[] = []
  const haptics: string[] = []
  context.mock.method(notifications, 'show', (data: NotificationData) => {
    shown.push(data)

    return 'id'
  })
  context.mock.method(mobileService, 'haptic', async (kind: string) => {
    haptics.push(kind)
  })

  return { shown, haptics }
}

async function trigger() {
  const { result } = renderHook(() => useTriggerWorkflow(), { wrapper })
  await act(async () => {
    await result.current.mutateAsync({ params: { workflow_id: 'p1' }, copy }).catch(() => undefined)
  })
}

test('a failed trigger on the phone buzzes the error haptic and stays up for the phone error duration', async (context) => {
  const { shown, haptics } = observe(context)
  const failing = async (): Promise<never> => {
    throw new Error('runner offline')
  }
  context.mock.method(apiService, 'triggerPipeline', failing)
  context.mock.method(tauriService, 'triggerPipeline', failing)

  await trigger()

  assert.deepEqual(haptics, ['error'])
  assert.equal(shown.length, 1)
  assert.equal(shown[0]?.title, "Couldn't run workflow")
  assert.equal(shown[0]?.message, 'runner offline')
  assert.equal(shown[0]?.autoClose, NOTIFICATION_DURATIONS.ERROR_MOBILE)
  assert.ok(shown[0]?.icon)
})

test('a trigger that starts a run buzzes the success haptic', async (context) => {
  const { shown, haptics } = observe(context)
  const starting = async () => ({ run_number: 7 })
  context.mock.method(apiService, 'triggerPipeline', starting)
  context.mock.method(tauriService, 'triggerPipeline', starting)

  await trigger()

  assert.deepEqual(haptics, ['success'])
  assert.equal(shown[0]?.title, 'Run started')
  assert.ok(shown[0]?.icon)
})
