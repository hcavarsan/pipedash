import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { inflateSync } from 'node:zlib'
import JSZip from 'jszip'
import { chromium } from 'playwright'

const root = fileURLToPath(new URL('../', import.meta.url))
const targets = { iphone: [1320, 2868], ipad: [2064, 2752] }
const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])

export function validateProject(project) {
  assert.deepEqual(project.locales, ['en'], 'Only the en locale is supported; Fastlane output is en-US')
  assert.equal(project.locale, 'en', 'The active locale must be en')
  assert(Object.hasOwn(targets, project.device), 'The active device must be iPhone or iPad')
  assert.equal(project.orientation, 'portrait', 'Fastlane screenshots require portrait orientation')
  for (const device of Object.keys(targets)) {
    const slides = project.slidesByDevice?.[device]
    assert(Array.isArray(slides) && slides.length >= 1 && slides.length <= 10, `${device} requires 1–10 slides`)
    for (const slide of slides) assert(/^[a-z0-9-]+$/.test(slide.layout), `Invalid ${device} slide layout`)
  }
}

function validatePng(bytes, width, height, name) {
  assert(bytes.length >= 45 && bytes.subarray(0, 8).equals(pngSignature), `${name}: invalid PNG signature`)
  assert.equal(bytes.readUInt32BE(8), 13, `${name}: invalid IHDR length`)
  assert.equal(bytes.toString('ascii', 12, 16), 'IHDR', `${name}: missing IHDR`)
  assert.equal(bytes.readUInt32BE(16), width, `${name}: wrong width`)
  assert.equal(bytes.readUInt32BE(20), height, `${name}: wrong height`)
  assert.equal(bytes[24], 8, `${name}: expected 8-bit PNG`)
  assert.equal(bytes[25], 2, `${name}: expected opaque RGB PNG (color type 2)`)
  assert.deepEqual([...bytes.subarray(26, 29)], [0, 0, 0], `${name}: unsupported PNG encoding`)
  const compressed = []
  let ended = false
  for (let offset = 8; offset < bytes.length;) {
    assert(offset + 12 <= bytes.length, `${name}: truncated PNG chunk`)
    const length = bytes.readUInt32BE(offset)
    const type = bytes.toString('ascii', offset + 4, offset + 8)
    assert(offset + 12 + length <= bytes.length, `${name}: truncated ${type} chunk`)
    assert(type !== 'tRNS', `${name}: transparent PNG is not allowed`)
    if (type === 'IDAT') compressed.push(bytes.subarray(offset + 8, offset + 8 + length))
    offset += length + 12
    if (type === 'IEND') {
      assert(length === 0 && offset === bytes.length, `${name}: invalid PNG end`)
      ended = true
      break
    }
  }
  assert(ended && compressed.length, `${name}: incomplete PNG`)
  const rowLength = width * 3 + 1
  const pixels = inflateSync(Buffer.concat(compressed), { maxOutputLength: rowLength * height })
  assert.equal(pixels.length, rowLength * height, `${name}: incomplete pixel data`)
  for (let row = 0; row < height; row++) assert(pixels[row * rowLength] <= 4, `${name}: invalid PNG filter`)
}

export async function validateBundle(bytes, device, slides) {
  assert(Object.hasOwn(targets, device), 'Unsupported export device')
  const [width, height] = targets[device]
  const prefix = `ios/${device}/${width}x${height}/en/`
  const zip = await JSZip.loadAsync(bytes, { checkCRC32: true })
  const files = Object.values(zip.files).filter(file => !file.dir)
  for (const file of files) {
    const original = file.unsafeOriginalName ?? file.name
    assert(!original.includes('\\') && !original.startsWith('/') && !original.split('/').some(part => part === '..' || part === '.'), `Unsafe ZIP member: ${original}`)
    assert.equal(original, file.name, `Rewritten ZIP member: ${original}`)
    assert(new RegExp(`^ios/${device}/[0-9]+x[0-9]+/en/[0-9]{2}-[a-z0-9-]+\\.png$`).test(file.name), `Unexpected ZIP member: ${file.name}`)
  }
  const selected = files.filter(file => file.name.startsWith(prefix)).sort((a, b) => a.name.localeCompare(b.name))
  assert.equal(selected.length, slides.length, `${device}: incomplete or duplicate target-size screenshots`)
  const output = []
  for (const [index, file] of selected.entries()) {
    const number = String(index + 1).padStart(2, '0')
    assert.equal(file.name, `${prefix}${number}-${slides[index].layout}.png`, `${device}: wrong slide order or layout`)
    const png = await file.async('nodebuffer')
    validatePng(png, width, height, file.name)
    output.push({ name: `${number}-${device}.png`, bytes: png })
  }
  return output
}

function launchServer() {
  const next = createRequire(import.meta.url).resolve('next/dist/bin/next')
  const child = spawn(process.execPath, [next, 'start', '--hostname', '127.0.0.1', '--port', '0'], {
    cwd: root,
    env: { ...process.env, NEXT_TELEMETRY_DISABLED: '1' },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const ready = new Promise((resolve, reject) => {
    let log = ''
    const timeout = setTimeout(() => reject(new Error(`Next server did not start:\n${log}`)), 60_000)
    function finish(error, url) {
      clearTimeout(timeout)
      if (error) reject(error)
      else resolve(url)
    }
    child.on('error', error => finish(error))
    child.on('exit', code => finish(new Error(`Next server exited (${code}):\n${log}`)))
    for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => {
      log = (log + chunk.toString()).slice(-20_000)
      const match = log.match(/http:\/\/127\.0\.0\.1:([1-9][0-9]*)/)
      if (match) finish(null, match[0])
    })
  })
  return { child, ready }
}

async function stopServer(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return
  const exited = once(child, 'exit')
  child.kill('SIGTERM')
  const timeout = setTimeout(() => child.kill('SIGKILL'), 5000)
  try { await exited } finally { clearTimeout(timeout) }
}

async function exportDevice(browser, url, project, device, archive) {
  const context = await browser.newContext({ acceptDownloads: true, locale: 'en-US', viewport: { width: 1600, height: 1000 } })
  try {
    let memory = structuredClone(project)
    await context.route('**/api/project', async route => {
      if (route.request().method() === 'POST') memory = route.request().postDataJSON()
      await route.fulfill({ json: { ok: true, state: memory }, headers: { etag: '"export-memory"' } })
    })
    await context.addInitScript(() => {
      localStorage.clear()
      window.__exportFailures = []
      new MutationObserver(() => {
        for (const toast of document.querySelectorAll('[data-sonner-toast][data-type="warning"], [data-sonner-toast][data-type="error"]')) {
          if (!window.__exportFailures.includes(toast.textContent)) window.__exportFailures.push(toast.textContent)
        }
      }).observe(document, { childList: true, subtree: true, attributes: true })
    })
    const page = await context.newPage()
    const errors = []
    page.on('pageerror', error => errors.push(error.message))
    page.on('response', response => {
      if (response.status() >= 400) errors.push(`HTTP ${response.status()}: ${response.url()}`)
    })
    await page.goto(url, { waitUntil: 'networkidle', timeout: 60_000 })
    const saved = memory.device !== device ? page.waitForResponse(response => response.url().endsWith('/api/project') && response.request().method() === 'POST' && response.request().postDataJSON().device === device) : null
    await page.getByRole('combobox', { name: 'Device', exact: true }).click()
    await page.getByRole('option', { name: device === 'iphone' ? 'iPhone' : 'iPad', exact: true }).click()
    await saved
    const download = page.waitForEvent('download', { timeout: 240_000 })
    const failure = page.waitForFunction(() => window.__exportFailures.length > 0, { }, { timeout: 240_000 }).then(async () => {
      throw new Error((await page.evaluate(() => window.__exportFailures)).join('\n'))
    })
    const [, result] = await Promise.all([
      page.getByRole('button', { name: 'Export bundle', exact: true }).click(),
      Promise.race([download, failure]),
    ])
    await result.saveAs(archive)
    await page.waitForFunction(() => [...document.querySelectorAll('[data-sonner-toast][data-type="success"]')].some(toast => toast.textContent.startsWith('Exported ')), {}, { timeout: 30_000 })
    errors.push(...await page.evaluate(() => window.__exportFailures))
    assert.deepEqual(errors, [], `${device}: editor export warnings or missing assets`)
    return await validateBundle(await readFile(archive), device, project.slidesByDevice[device])
  } finally {
    await context.close()
  }
}

export async function main() {
  const projectPath = path.join(root, 'app-store-screenshots.json')
  const canonical = await readFile(projectPath)
  const project = JSON.parse(canonical.toString())
  validateProject(project)
  await access(path.join(root, '.next', 'BUILD_ID')).catch(() => { throw new Error('Build the screenshot editor first: bun run build') })
  const output = path.resolve(root, '../fastlane/screenshots')
  await mkdir(output, { recursive: true })
  await mkdir(path.join(root, 'exports'), { recursive: true })
  const stage = await mkdtemp(path.join(output, '.export-'))
  const locale = path.join(output, 'en-US')
  const backup = path.join(stage, 'previous-en-US')
  let server
  let browser
  let interrupted = false
  let preserveStage = false
  const interrupt = () => {
    interrupted = true
    void browser?.close().catch(() => {})
    server?.child.kill('SIGTERM')
  }
  process.on('SIGINT', interrupt)
  process.on('SIGTERM', interrupt)
  try {
    server = launchServer()
    const url = await server.ready
    browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined, headless: true })
    if (interrupted) throw new Error('Screenshot export interrupted')
    const generated = path.join(stage, 'en-US')
    await mkdir(generated)
    for (const device of Object.keys(targets)) {
      const archive = path.join(root, 'exports', `${device}.zip`)
      const files = await exportDevice(browser, url, project, device, archive)
      if (interrupted) throw new Error('Screenshot export interrupted')
      for (const file of files) await writeFile(path.join(generated, file.name), file.bytes)
      console.log(`${device}: ${files.length} validated screenshots; complete bundle: ${archive}`)
    }
    assert((await readFile(projectPath)).equals(canonical), 'Canonical project changed during export; output was not replaced')
    if (interrupted) throw new Error('Screenshot export interrupted')
    let hadPrevious = false
    try { await rename(locale, backup); hadPrevious = true } catch (error) { if (error.code !== 'ENOENT') throw error }
    try { await rename(generated, locale) } catch (error) {
      if (hadPrevious) {
        try { await rename(backup, locale) } catch (cause) {
          preserveStage = true
          throw new Error(`Could not restore previous screenshots; preserved at ${backup}`, { cause })
        }
      }
      throw error
    }
    console.log(`Fastlane screenshots ready: ${locale}`)
  } finally {
    process.off('SIGINT', interrupt)
    process.off('SIGTERM', interrupt)
    try { await browser?.close() } finally {
      try { if (server) await stopServer(server.child) } finally {
        if (!preserveStage) await rm(stage, { recursive: true, force: true })
      }
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch(error => { console.error(error); process.exitCode = 1 })
}
