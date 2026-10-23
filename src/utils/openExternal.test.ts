import assert from 'node:assert/strict'
import { test } from 'node:test'

const opened: Array<[string, string | undefined, string | undefined]> = []

Object.assign(globalThis, {
  document: { baseURI: 'http://localhost/' },
  localStorage: { getItem: () => null, setItem: () => undefined, removeItem: () => undefined },
  window: {
    location: { protocol: 'http:', host: 'localhost' },
    open: (url: string, target?: string, features?: string) => opened.push([url, target, features]),
  },
})

const { isHttpUrl, openExternal } = await import('./openExternal')

test('only http and https urls are external links', () => {
  assert.equal(isHttpUrl('https://www.githubstatus.com/incidents/1'), true)
  assert.equal(isHttpUrl('HTTP://example.com'), true)
  assert.equal(isHttpUrl('file:///etc/passwd'), false)
  assert.equal(isHttpUrl('javascript:alert(1)'), false)
  assert.equal(isHttpUrl('//example.com'), false)
  assert.equal(isHttpUrl(''), false)
})

test('rejects non-http(s) urls without opening anything', async () => {
  assert.equal(await openExternal('file:///etc/passwd'), false)
  assert.equal(await openExternal('javascript:alert(1)'), false)
  assert.deepEqual(opened, [])
})

test('opens http(s) urls in a new tab with noopener on web', async () => {
  assert.equal(await openExternal('https://status.example.com'), true)
  assert.deepEqual(opened, [['https://status.example.com', '_blank', 'noopener,noreferrer']])
})
