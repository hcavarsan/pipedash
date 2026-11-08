'use client'
import { type RefObject, useRef, useState } from 'react'
import { toast } from 'sonner'
import { generateBundle } from '@/lib/export-bundle'
import type { ProjectState, ScreenshotFontId, Slide } from '@/lib/types'

export type ScreenshotExport = {
  exporting: string | null
  exportProject: ProjectState | null
  exportLocaleOverride: string | null
  exportSlideIndex: number
  exportRef: RefObject<HTMLDivElement | null>
  exportInProgress: RefObject<boolean>
  exportAll: () => void
}

export function useScreenshotExport(
  state: ProjectState,
  slides: Slide[],
  fontId: ScreenshotFontId,
  fontFamily: string
): ScreenshotExport {
  const [exporting, setExporting] = useState<string | null>(null)
  const [exportLocaleOverride, setExportLocaleOverride] = useState<string | null>(null)
  const [exportSlideIndex, setExportSlideIndex] = useState(0)
  const [exportProject, setExportProject] = useState<ProjectState | null>(null)
  const exportInProgress = useRef(false)
  const exportRef = useRef<HTMLDivElement | null>(null)

  async function runExport(): Promise<void> {
    if (exportInProgress.current) {
      return
    }
    exportInProgress.current = true
    setExporting('Preparing…')
    setExportProject(state)
    try {
      await generateBundle({
        state,
        slides,
        fontId,
        fontFamily,
        target: exportRef,
        setProgress: setExporting,
        setLocale: setExportLocaleOverride,
        setSlideIndex: setExportSlideIndex,
      })
    } catch (error) {
      toast.error('Export failed', {
        description: error instanceof Error ? error.message : String(error),
      })
    } finally {
      setExportLocaleOverride(null)
      setExportProject(null)
      setExporting(null)
      exportInProgress.current = false
    }
  }

  return {
    exporting,
    exportProject,
    exportLocaleOverride,
    exportSlideIndex,
    exportRef,
    exportInProgress,
    exportAll: () => {
      void runExport()
    },
  }
}
