'use client'
import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'

const MIN_ZOOM = 0.25
const MAX_ZOOM = 2

type Options = {
  deckKey: string
  cW: number
  cH: number
  activeIndex: number
  activeId: string | undefined
}

type PreviewViewport = {
  containerRef: RefObject<HTMLDivElement | null>
  scrollerRef: RefObject<HTMLDivElement | null>
  scale: number
  zoom: number
  stepZoom: (delta: number) => void
  fitActiveScreen: () => void
  suppressNextPan: () => void
}

// Zoom is stored with the deck it was set on, so another device/orientation reads 1.
export function usePreviewViewport({ deckKey, cW, cH, activeIndex, activeId }: Options): PreviewViewport {
  const containerRef = useRef<HTMLDivElement>(null)
  const scrollerRef = useRef<HTMLDivElement>(null)
  const suppressNextActiveScreenPanRef = useRef(false)
  const [fitScale, setFitScale] = useState(0.2)
  const [zoomState, setZoomState] = useState({ deckKey, zoom: 1 })
  const zoom = zoomState.deckKey === deckKey ? zoomState.zoom : 1
  const scale = fitScale * zoom

  useEffect(() => {
    const el = containerRef.current
    if (!el) {
      return
    }
    const update = () => {
      const rect = el.getBoundingClientRect()
      const sx = (rect.width - 96) / cW
      const sy = (rect.height - 96) / cH
      setFitScale(Math.max(0.05, Math.min(sx, sy)))
    }
    update()
    const ro = new ResizeObserver(update)
    ro.observe(el)
    return () => ro.disconnect()
  }, [cW, cH])

  const stepZoom = useCallback(
    (delta: number) => {
      setZoomState((prev) => {
        const current = prev.deckKey === deckKey ? prev.zoom : 1
        return { deckKey, zoom: Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number((current + delta).toFixed(2)))) }
      })
    },
    [deckKey]
  )

  const panToActiveScreen = useCallback(
    (nextScale: number) => {
      const scroller = scrollerRef.current
      if (!scroller || !activeId) {
        return
      }
      const screenLeft = activeIndex * cW * nextScale
      const screenWidth = cW * nextScale
      const targetLeft = Math.max(0, screenLeft - (scroller.clientWidth - screenWidth) / 2)
      scroller.scrollTo({ left: targetLeft, behavior: 'smooth' })
    },
    [activeIndex, activeId, cW]
  )

  useEffect(() => {
    if (suppressNextActiveScreenPanRef.current) {
      suppressNextActiveScreenPanRef.current = false
      return
    }
    panToActiveScreen(scale)
  }, [panToActiveScreen, scale])

  const fitActiveScreen = useCallback(() => {
    setZoomState({ deckKey, zoom: 1 })
    panToActiveScreen(fitScale)
  }, [deckKey, fitScale, panToActiveScreen])

  const suppressNextPan = useCallback(() => {
    suppressNextActiveScreenPanRef.current = true
  }, [])

  return { containerRef, scrollerRef, scale, zoom, stepZoom, fitActiveScreen, suppressNextPan }
}
