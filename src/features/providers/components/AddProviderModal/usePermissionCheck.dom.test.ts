import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { PermissionCheckResult, ProviderConfig } from '../../../../types/domain'

import { pluginFixture } from './pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../../services/api')
const { tauriService } = await import('../../../../services/tauri')
const { usePermissionCheck } = await import('./usePermissionCheck')

notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

const configFor = (token: string): ProviderConfig => ({
  name: 'GitHub',
  provider_type: 'github',
  token,
  config: { token },
  refresh_interval: 300,
})

const granted: PermissionCheckResult = {
  permission_status: { permissions: [], all_granted: true, checked_at: '2026-01-01T00:00:00Z' },
  features: [],
}

beforeEach(() => {
  client.clear()
})
afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('permission details disappear when the credentials change', async (context) => {
  const check = async (): Promise<PermissionCheckResult> => granted

  context.mock.method(tauriService, 'checkProviderPermissions', check)
  context.mock.method(apiService, 'checkProviderPermissions', check)

  const selectedPlugin = pluginFixture()
  const { result, rerender } = renderHook(
    ({ providerConfig }) => usePermissionCheck({ selectedPlugin, providerConfig }),
    { wrapper, initialProps: { providerConfig: configFor('first') } }
  )

  await act(async () => result.current.checkPermissions())
  await waitFor(() => assert.equal(result.current.status?.all_granted, true))

  rerender({ providerConfig: configFor('second') })

  assert.equal(result.current.status, null)
})

test('a failed check surfaces a readable error and no details', async (context) => {
  const check = async (): Promise<PermissionCheckResult> => {
    throw { type: 'network', message: 'offline' }
  }

  context.mock.method(tauriService, 'checkProviderPermissions', check)
  context.mock.method(apiService, 'checkProviderPermissions', check)

  const { result } = renderHook(
    () => usePermissionCheck({ selectedPlugin: pluginFixture(), providerConfig: configFor('x') }),
    { wrapper }
  )

  await act(async () => result.current.checkPermissions())
  await waitFor(() => assert.equal(result.current.error, "Couldn't reach the provider. Check your connection."))
  assert.equal(result.current.status, null)
  assert.equal(result.current.modalOpen, true)
})
