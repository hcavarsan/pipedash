import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

test('foreground notification scenarios use an isolated iOS bridge', () => {
  const scenarios = fileURLToPath(new URL('./fixtures/notificationScenarios.ts', import.meta.url))
  const result = spawnSync(process.execPath, ['test', scenarios], { encoding: 'utf8', timeout: 30000 })
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
})

test('desktop notification scenarios mirror native history without frontend delivery', () => {
  const scenarios = fileURLToPath(new URL('./fixtures/notificationDesktopScenarios.ts', import.meta.url))
  const result = spawnSync(process.execPath, ['test', scenarios], {
    encoding: 'utf8',
    timeout: 30000,
    env: { ...process.env, PIPEDASH_NOTIFICATION_PLATFORM: 'macos' },
  })
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
})

test('web notification scenarios recover missed events after reconnect', () => {
  const scenarios = fileURLToPath(new URL('./fixtures/notificationWebScenarios.ts', import.meta.url))
  const result = spawnSync(process.execPath, ['test', scenarios], { encoding: 'utf8', timeout: 30000 })
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
})
