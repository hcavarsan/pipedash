'use client'
import JSZip from 'jszip'
import type { RefObject } from 'react'
import { toast } from 'sonner'
import { getCanvas } from './canvas-device'
import { detectPlatform } from './defaults'
import { type ExportSize, getExportSizes } from './device-specs'
import { exportAssetPaths } from './export-assets'
import { slugify, stamp } from './export-filenames'
import { exportPreflightWarning } from './export-preflight'
import { captureSlide, ensureFontsReady, waitForPaint } from './export-render'
import { didFail, preloadImages } from './image-cache'
import type { Platform, ProjectState, ScreenshotFontId, Slide } from './types'

export type ExportRun = {
  state: ProjectState
  slides: Slide[]
  fontId: ScreenshotFontId
  fontFamily: string
  target: RefObject<HTMLDivElement | null>
  setProgress: (label: string) => void
  setLocale: (locale: string) => void
  setSlideIndex: (index: number) => void
}

type Job = {
  run: ExportRun
  sizes: ExportSize[]
  zip: JSZip
  platform: Platform
  canvas: { cW: number; cH: number }
  encoding: Promise<void>
  ok: number
  failed: number
  errors: string[]
  incomplete: string[]
}

function addToZip(job: Job, locale: string, filename: string, files: Uint8Array[]): void {
  for (const [index, size] of job.sizes.entries()) {
    const file = files[index]
    if (file) {
      job.zip.file(`${job.platform}/${job.run.state.device}/${size.w}x${size.h}/${locale}/${filename}`, file)
    }
  }
  job.ok += job.sizes.length
}

async function renderScreen(job: Job, locale: string, index: number): Promise<void> {
  const { run, sizes } = job
  const slide = run.slides[index]
  const el = run.target.current
  const label = `${locale} screen ${index + 1}`
  if (!slide || !el) {
    await job.encoding
    job.failed += sizes.length
    job.errors.push(`${label}: render target missing`)
    return
  }
  const filename = `${String(index + 1).padStart(2, '0')}-${slide.layout}.png`
  const fail = (e: unknown) => {
    job.errors.push(`${label}: ${e instanceof Error ? e.message : String(e)}`)
    console.error('Export failed', { slideId: slide.id, locale }, e)
  }
  let pngs: Promise<Uint8Array[]>
  try {
    const rendered = await captureSlide(el, job.canvas.cW, job.canvas.cH)
    if (rendered.missingImages > 0) {
      job.incomplete.push(label)
    }
    pngs = Promise.all(sizes.map((size) => rendered.toPng(size.w, size.h)))
  } catch (e) {
    fail(e)
    await job.encoding
    job.failed += sizes.length
    return
  }
  // Attach rejection handling now: this screen can fail while the
  // previous screen is still encoding.
  const nextEncoding = pngs.then(
    (files) => addToZip(job, locale, filename, files),
    (e) => {
      fail(e)
      job.failed += sizes.length
    }
  )
  await job.encoding
  job.encoding = nextEncoding
}

// Render each slide once per locale at canvas resolution, then scale that
// one render to every export size. The sizes are all scalings of the same
// design, so re-rendering the DOM per size only added time. PNG encoding
// runs in workers, so a screen encodes while the next one renders; waiting
// for the previous screen first keeps at most one screen's pixels queued.
async function renderAll(job: Job): Promise<void> {
  const { run } = job
  const totalRenders = run.state.locales.length * run.slides.length
  let render = 0
  for (const locale of run.state.locales) {
    run.setLocale(locale)
    await waitForPaint()
    for (let i = 0; i < run.slides.length; i++) {
      render += 1
      run.setProgress(`${render}/${totalRenders}`)
      run.setSlideIndex(i)
      await waitForPaint()
      await renderScreen(job, locale, i)
    }
  }
  await job.encoding
}

async function downloadBundle(job: Job): Promise<boolean> {
  const { state } = job.run
  try {
    const blob = await job.zip.generateAsync({ type: 'blob' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${slugify(state.appName)}-${job.platform}-${state.device}-${stamp()}.zip`
    a.click()
    setTimeout(() => URL.revokeObjectURL(url), 5000)
    return true
  } catch (e) {
    toast.error("Couldn't bundle export")
    console.error(e)
    return false
  }
}

function reportResult(job: Job): void {
  const localeCount = job.run.state.locales.length
  const totalUnits = job.sizes.length * localeCount * job.run.slides.length
  if (job.incomplete.length > 0) {
    toast.warning('Some screenshots may be missing from the export', {
      description: `${job.incomplete.slice(0, 3).join(', ')}${job.incomplete.length > 3 ? '…' : ''}: a screenshot never finished rendering. Check those files, or export again.`,
      duration: 12000,
    })
  }
  const summary = `${localeCount} locale${localeCount === 1 ? '' : 's'} × ${job.sizes.length} size${job.sizes.length === 1 ? '' : 's'}`
  const description = job.errors.slice(0, 3).join('\n')
  if (job.failed === 0) {
    toast.success(`Exported ${job.ok} PNGs (${summary})`)
  } else if (job.ok === 0) {
    toast.error(`All ${job.failed} renders failed`, { description })
  } else {
    toast.error(`${job.failed} of ${totalUnits} renders failed`, { description })
  }
}

export async function generateBundle(run: ExportRun): Promise<void> {
  const { state, slides } = run
  if (!slides.length) {
    toast.error('No screens to export')
    return
  }
  const sizes = getExportSizes(state.device, state.orientation)
  if (!sizes.length) {
    toast.error('Nothing to export')
    return
  }
  const exportPaths = exportAssetPaths(state)
  await preloadImages(exportPaths, { retryFailed: true })
  const missingPaths = exportPaths.filter(didFail)
  if (missingPaths.length) {
    throw new Error(`Images could not be loaded: ${missingPaths.slice(0, 3).join(', ')}`)
  }
  await waitForPaint()

  const warning = exportPreflightWarning(state, slides)
  if (warning) {
    toast.warning(warning.title, { description: warning.description, duration: 7000 })
  }
  await ensureFontsReady(run.fontId === 'self-hosted' ? run.fontFamily : null)

  const job: Job = {
    run,
    sizes,
    zip: new JSZip(),
    platform: detectPlatform(state.device),
    canvas: getCanvas(state.device, state.orientation),
    encoding: Promise.resolve(),
    ok: 0,
    failed: 0,
    errors: [],
    incomplete: [],
  }
  await renderAll(job)

  run.setProgress('Bundling…')
  if (job.ok > 0 && !(await downloadBundle(job))) {
    return
  }
  reportResult(job)
}
