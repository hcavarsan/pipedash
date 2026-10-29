import assert from 'node:assert/strict'
import { after, before, beforeEach, test } from 'node:test'

import { mobileService } from './mobile'

const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = []
let reply: () => unknown = () => ({ path: '/tmp/pipedash-exports/file', shared: false })
const originalWindow = Object.getOwnPropertyDescriptor(globalThis, 'window')

before(() => {
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    writable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async (command: string, args?: Record<string, unknown>) => {
          calls.push({ command, args })

          return reply()
        },
      },
    },
  })
})

after(() => {
  if (originalWindow) {
    Object.defineProperty(globalThis, 'window', originalWindow)
  } else {
    Reflect.deleteProperty(globalThis, 'window')
  }
})

beforeEach(() => {
  calls.length = 0
  reply = () => ({ path: '/tmp/pipedash-exports/file', shared: false })
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
  reply = () => ({ path: '/tmp/pipedash-exports/file', shared: true })
  const contents = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 0xff])
  const result = await mobileService.saveExportFile('chart.png', contents)

  assert.equal(result.shared, true)
  assert.deepEqual(calls.at(-1), {
    command: 'save_export_file',
    args: { fileName: 'chart.png', contents: [0x89, 0x50, 0x4e, 0x47, 0, 0xff] },
  })
})

test('a rejected native export surfaces a normalized error', async () => {
  reply = () => {
    throw new Error('disk full')
  }

  await assert.rejects(mobileService.saveExportFile('a.toml', 'x'), { type: 'unknown', message: 'disk full' })
})

test('empty text and empty byte exports send no contents', async () => {
  await mobileService.saveExportFile('empty.toml', '')
  await mobileService.saveExportFile('empty.png', new Uint8Array())

  assert.deepEqual(
    calls.map((call) => call.args?.contents),
    [[], []]
  )
})
