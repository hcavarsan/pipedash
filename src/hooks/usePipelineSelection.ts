import { useCallback, useMemo, useState } from 'react'

import type { PipelineViewPage, ViewPipeline } from '../types'

interface PipelineSelectionResult {
  selecting: boolean
  selectedPipelines: ViewPipeline[]
  toggle: (pipeline: ViewPipeline) => void
  select: (visibleIds: string[], chosenIds: string[]) => void
  start: () => void
  finish: () => void
}

export function usePipelineSelection(view: string, page: PipelineViewPage | undefined): PipelineSelectionResult {
  const pipelines = useMemo(
    () =>
      new Map(
        [
          ...(page?.pinned ?? []),
          ...(page?.pinned_blocks.flatMap((block) => block.pipelines) ?? []),
          ...(page?.sections.flatMap((section) => section.pipelines) ?? []),
        ].map((p) => [p.id, p])
      ),
    [page]
  )
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [selecting, setSelecting] = useState(false)
  const [scope, setScope] = useState({ view, pipelines })

  if (scope.view !== view || scope.pipelines !== pipelines) {
    setScope({ view, pipelines })
    setSelectedIds((ids) => (scope.view === view ? ids.filter((id) => pipelines.has(id)) : []))
    if (scope.view !== view) {
      setSelecting(false)
    }
  }

  const selectedPipelines = useMemo(
    () => selectedIds.flatMap((id) => pipelines.get(id) ?? []),
    [selectedIds, pipelines]
  )

  const toggle = useCallback(
    (pipeline: ViewPipeline) =>
      setSelectedIds((ids) =>
        ids.includes(pipeline.id) ? ids.filter((id) => id !== pipeline.id) : [...ids, pipeline.id]
      ),
    []
  )

  const select = useCallback((visibleIds: string[], chosenIds: string[]) => {
    const visible = new Set(visibleIds)

    setSelectedIds((ids) => [...new Set([...ids.filter((id) => !visible.has(id)), ...chosenIds])])
  }, [])

  const start = useCallback(() => setSelecting(true), [])

  const finish = useCallback(() => {
    setSelectedIds([])
    setSelecting(false)
  }, [])

  return { selecting, selectedPipelines, toggle, select, start, finish }
}
