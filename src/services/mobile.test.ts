import assert from 'node:assert/strict'
import { test } from 'node:test'

import { mobileService } from './mobile'

const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = []
let shared = false

Object.assign(globalThis, {
  window: {
    __TAURI_INTERNALS__: {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        calls.push({ command, args })
        return { path: '/tmp/pipedash-exports/file', shared }
      },
    },
  },
})

test('native text exports preserve UTF-8 and return cancellation without success', async () => {
  const result = await mobileService.saveExportFile('providers.toml', 'name = "ação"')

  assert.equal(result.shared, false)
  assert.deepEqual(calls.at(-1), {
    command: 'save_export_file',
    args: { fileName: 'providers.toml', contents: Array.from(new TextEncoder().encode('name = "ação"')) },
  })
})

test('native image exports preserve binary bytes and return completion', async () => {
  shared = true
  const contents = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 0xff])
  const result = await mobileService.saveExportFile('chart.png', contents)

  assert.equal(result.shared, true)
  assert.deepEqual(calls.at(-1), {
    command: 'save_export_file',
    args: { fileName: 'chart.png', contents: [0x89, 0x50, 0x4e, 0x47, 0, 0xff] },
  })
})
