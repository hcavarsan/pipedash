'use client'
import { useEffect, useMemo, useReducer, useState } from 'react'
import { toast } from 'sonner'
import { IMPORTED_FONT_FAMILY } from '@/lib/fonts'
import { type FrameFile, frameAssetPaths, type MeasuredFrame, setFrameAssets } from '@/lib/frame-assets'
import { preloadImages } from '@/lib/image-cache'
import { resolveScreenshot } from '@/lib/locale'
import type { ImportedFont, ProjectState, Slide } from '@/lib/types'

type FramesResponse = {
  ok: boolean
  frames?: Partial<Record<FrameFile, MeasuredFrame>>
  errors?: Partial<Record<FrameFile, string>>
}

type AssetSource = {
  slidesByDevice: ProjectState['slidesByDevice']
  appIcon: string | undefined
  locales: string[]
  device: ProjectState['device']
}

// Registers the imported font in <head>, not inside the canvases: html-to-image
// embeds @font-face rules it finds in document.styleSheets, while a <style>
// cloned into the export SVG would point at a URL the SVG image can't load.
function useImportedFontFace(font: ImportedFont | undefined): void {
  const src = font?.src
  const format = font?.format
  useEffect(() => {
    if (!src || !format) {
      return
    }
    const style = document.createElement('style')
    style.dataset.importedScreenshotFont = ''
    style.textContent = `@font-face { font-family: "${IMPORTED_FONT_FAMILY}"; src: url("${src}") format("${format}"); font-display: block; }`
    document.head.appendChild(style)
    return () => style.remove()
  }, [src, format])
}

// Measures any Apple bezels the user added to public/frames/ before the
// first Duo frame renders. Missing files are normal; the drawn frame is used.
function useFrameAssets(): boolean {
  const [loaded, setLoaded] = useState(false)
  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const resp = await fetch('/api/frames', { cache: 'no-store', signal: AbortSignal.timeout(15000) })
        const json: FramesResponse = await resp.json()
        if (cancelled) {
          return
        }
        setFrameAssets(json.frames ?? {})
        const errors = Object.values(json.errors ?? {})
        if (errors.length) {
          toast.warning("A device bezel couldn't be used", {
            description: `${errors.join('\n')}\nThe drawn frame is shown instead.`,
            duration: 12000,
          })
        }
      } catch {
        // No frames route (e.g. a static build): the drawn frames still work.
      }
      if (!cancelled) {
        setLoaded(true)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])
  return loaded
}

function slideAssetPaths(slide: Slide, locales: string[]): string[] {
  const paths: string[] = []
  for (const raw of [slide.screenshot, slide.screenshotSecondary]) {
    if (!raw) {
      continue
    }
    // Inline (data URI) captures are decoded too, so their size is known
    // after a reload and a mismatched Duo capture is still flagged.
    if (raw.startsWith('data:') || !raw.includes('{locale}')) {
      paths.push(raw)
      continue
    }
    for (const loc of locales) {
      paths.push(resolveScreenshot(raw, loc))
    }
  }
  for (const imageElement of slide.imageElements || []) {
    if (imageElement.src && !imageElement.src.startsWith('data:')) {
      paths.push(imageElement.src)
    }
  }
  return paths
}

function collectAssetPaths(source: AssetSource, framesLoaded: boolean): string[] {
  const paths = new Set<string>()
  paths.add('/mockup.png')
  // Only the open deck's bezel: each is a large PNG, and export preloads its
  // own device's bezel before capturing.
  for (const path of framesLoaded ? frameAssetPaths(source.device) : []) {
    paths.add(path)
  }
  if (source.appIcon) {
    paths.add(source.appIcon)
  }
  // Preload every locale variant so bulk export doesn't race image loads.
  for (const slide of Object.values(source.slidesByDevice).flat()) {
    for (const path of slideAssetPaths(slide, source.locales)) {
      paths.add(path)
    }
  }
  return Array.from(paths).sort()
}

// Preloads every image the deck can show. The effect depends on the exact path
// list (serialized), so edits that don't change any path never restart it.
function useAssetPreload(source: ProjectState, hydrated: boolean, framesLoaded: boolean): boolean {
  const [ready, setReady] = useState(false)
  const [, refreshAssets] = useReducer((version: number) => version + 1, 0)
  const { slidesByDevice, appIcon, locales, device } = source
  const pathsKey = useMemo(
    () => JSON.stringify(collectAssetPaths({ slidesByDevice, appIcon, locales, device }, framesLoaded)),
    [slidesByDevice, appIcon, locales, device, framesLoaded]
  )

  useEffect(() => {
    // Wait for the bezel measurements, so the first paint already has the
    // open deck's bezel cached instead of swapping it in under the user.
    if (!hydrated || !framesLoaded) {
      return
    }
    let cancelled = false
    const paths: string[] = JSON.parse(pathsKey)
    void preloadImages(paths).finally(() => {
      if (!cancelled) {
        setReady(true)
        refreshAssets()
      }
    })
    return () => {
      cancelled = true
    }
  }, [hydrated, framesLoaded, pathsKey])

  return ready
}

/** True once the project is hydrated, bezels are measured and the deck's images are cached. */
export function useEditorAssets(state: ProjectState, hydrated: boolean): boolean {
  const framesLoaded = useFrameAssets()
  useImportedFontFace(state.importedFont)
  const preloaded = useAssetPreload(state, hydrated, framesLoaded)
  return hydrated && framesLoaded && preloaded
}
