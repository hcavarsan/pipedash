'use client'
import { type RefObject, useRef, useState } from 'react'
import { toast } from 'sonner'
import { slugify } from '@/lib/export-filenames'
import { renderSlide } from '@/lib/export-render'

type ComparisonExport = {
  exporting: boolean
  boardRef: RefObject<HTMLDivElement | null>
  exportComparison: () => Promise<void>
}

/** Captures the offscreen comparison board as a PNG download. */
export function useComparisonExport(appName: string): ComparisonExport {
  const [exporting, setExporting] = useState(false)
  const boardRef = useRef<HTMLDivElement>(null)

  async function exportComparison(): Promise<void> {
    if (exporting) {
      return
    }
    setExporting(true)
    try {
      // Let the board mount and lay out before capturing it.
      const frames = Promise.withResolvers<void>()
      requestAnimationFrame(() => requestAnimationFrame(() => frames.resolve()))
      await frames.promise
      await document.fonts?.ready
      const el = boardRef.current
      if (!el) {
        throw new Error('Comparison board did not render')
      }
      const w = el.offsetWidth
      const h = el.offsetHeight
      const rendered = await renderSlide(el, w, h, '#F4F3EF')
      const png = await rendered.toPng(w, h)
      const url = URL.createObjectURL(new Blob([png.slice()], { type: 'image/png' }))
      const a = document.createElement('a')
      a.href = url
      a.download = `${slugify(appName)}-style-lab.png`
      a.click()
      setTimeout(() => URL.revokeObjectURL(url), 5000)
      toast.success('Comparison image exported')
    } catch (error) {
      toast.error("Couldn't export the comparison", {
        description: error instanceof Error ? error.message : String(error),
      })
    } finally {
      setExporting(false)
    }
  }

  return { exporting, boardRef, exportComparison }
}
