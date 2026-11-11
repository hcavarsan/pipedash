'use client'
import { useCallback, useState } from 'react'
import { toast } from 'sonner'
import { nid } from '@/lib/defaults'
import { BUILT_IN_ELEMENT_IDS, updateElementTransform, updateSlideInProject } from '@/lib/elements'
import { writeLocalized } from '@/lib/locale'
import type { UseProject } from '@/lib/storage'
import type { Device, ElementId, ElementTransform, Patch, ProjectState, SelectedElement, Slide } from '@/lib/types'

type Project = Pick<UseProject, 'state' | 'setState'>

type EditorSlideActions = {
  currentSlides: Slide[]
  activeSlide: Slide | null
  selectedElement: SelectedElement | null
  selectSlide: (id: string) => void
  selectElement: (element: SelectedElement | null) => void
  selectActiveElement: (elementId: ElementId | null) => void
  clearSelection: () => void
  resetSelection: () => void
  patchSlide: (id: string, patch: Patch<Slide>) => void
  updateSlide: (id: string, update: (slide: Slide) => Patch<Slide>) => void
  reorderSlides: (next: Slide[]) => void
  deleteSlide: (id: string) => void
  addSlide: (slide: Slide) => void
  duplicateSlide: (id: string) => void
  patchLocalized: (slide: Slide, key: 'label' | 'headline', value: string) => void
  patchElementTransform: (slideId: string, elementId: ElementId, transform: ElementTransform) => void
  patchTextElementText: (slideId: string, textId: string, value: string) => void
}

function withDeck(project: ProjectState, device: Device, slides: Slide[]): ProjectState {
  return { ...project, slidesByDevice: { ...project.slidesByDevice, [device]: slides } }
}

function removeSlide(project: ProjectState, device: Device, id: string): ProjectState {
  return withDeck(
    project,
    device,
    (project.slidesByDevice[device] || []).filter((s) => s.id !== id)
  )
}

function restoreSlide(project: ProjectState, device: Device, snap: Slide, index: number): ProjectState {
  const current = project.slidesByDevice[device] || []
  if (current.some((s) => s.id === snap.id)) {
    return project
  }
  return withDeck(project, device, [...current.slice(0, index), snap, ...current.slice(index)])
}

function insertSlideAfter(project: ProjectState, afterId: string, copy: Slide): ProjectState {
  const slides = project.slidesByDevice[project.device] || []
  const idx = slides.findIndex((s) => s.id === afterId)
  if (idx === -1) {
    return project
  }
  return withDeck(project, project.device, [...slides.slice(0, idx + 1), copy, ...slides.slice(idx + 1)])
}

function cloneSlide(src: Slide): Slide {
  const copy: Slide = { ...src, id: nid(), label: { ...src.label }, headline: { ...src.headline } }
  if (src.transforms) {
    const transforms: NonNullable<Slide['transforms']> = {}
    for (const id of BUILT_IN_ELEMENT_IDS) {
      const transform = src.transforms[id]
      if (transform) {
        transforms[id] = { ...transform }
      }
    }
    copy.transforms = transforms
  }
  if (src.textElements) {
    copy.textElements = src.textElements.map((element) => ({
      ...element,
      id: nid(),
      text: { ...element.text },
      transform: { ...element.transform },
    }))
  }
  if (src.imageElements) {
    copy.imageElements = src.imageElements.map((element) => ({
      ...element,
      id: nid(),
      transform: { ...element.transform },
    }))
  }
  return copy
}

function setTextElementText(slide: Slide, textId: string, locale: string, value: string): Slide {
  return {
    ...slide,
    textElements: (slide.textElements || []).map((element) =>
      element.id === textId ? { ...element, text: writeLocalized(element.text, locale, value) } : element
    ),
  }
}

// Owns which screen and element are selected, and every edit to the open deck.
// Selection is derived, not reconciled by effects: an id that no longer exists
// (deleted, undone, other device) falls back to the first screen, and an element
// selection only counts while its screen is the active one.
export function useEditorSlideActions({ state, setState }: Project): EditorSlideActions {
  const [activeSlideId, setActiveSlideId] = useState<string | null>(null)
  const [selection, setSelection] = useState<SelectedElement | null>(null)

  const currentSlides = state.slidesByDevice[state.device] || []
  const activeSlide = currentSlides.find((s) => s.id === activeSlideId) || currentSlides[0] || null
  const selectedElement = selection && selection.slideId === activeSlide?.id ? selection : null

  const selectSlide = useCallback((id: string) => {
    setActiveSlideId(id)
    setSelection((cur) => (cur && cur.slideId === id ? cur : null))
  }, [])

  const clearSelection = useCallback(() => setSelection(null), [])

  const resetSelection = useCallback(() => {
    setActiveSlideId(null)
    setSelection(null)
  }, [])

  const activeSlideKey = activeSlide?.id
  const selectActiveElement = useCallback(
    (elementId: ElementId | null) => {
      if (activeSlideKey) {
        setSelection(elementId ? { slideId: activeSlideKey, elementId } : null)
      }
    },
    [activeSlideKey]
  )

  const patchSlide = useCallback(
    (id: string, patch: Patch<Slide>) => {
      setState((prev) => updateSlideInProject(prev, prev.device, id, (slide) => Object.assign({}, slide, patch)))
    },
    [setState]
  )

  const updateSlide = useCallback(
    (id: string, update: (slide: Slide) => Patch<Slide>) => {
      setState((prev) =>
        updateSlideInProject(prev, prev.device, id, (slide) => Object.assign({}, slide, update(slide)))
      )
    },
    [setState]
  )

  // Pin the default selection to its ID before reordering can change index 0.
  const reorderSlides = useCallback(
    (next: Slide[]) => {
      setActiveSlideId(activeSlide?.id ?? null)
      setState((prev) => withDeck(prev, prev.device, next), { coalesce: false })
    },
    [setState, activeSlide?.id]
  )

  const deleteSlide = useCallback(
    (id: string) => {
      const dev = state.device
      const slides = state.slidesByDevice[dev] || []
      const idx = slides.findIndex((s) => s.id === id)
      const snap = slides[idx]
      if (!snap) {
        return
      }
      const fallback = slides[idx + 1] || slides[idx - 1] || null

      setState((prev) => removeSlide(prev, dev, id), { coalesce: false })
      setActiveSlideId((cur) => (cur === id ? fallback?.id || null : cur))
      setSelection((cur) => (cur?.slideId === id ? null : cur))

      const undoDelete = () => {
        setState((prev) => restoreSlide(prev, dev, snap, idx), { coalesce: false })
        setActiveSlideId(snap.id)
      }
      toast('Screen deleted', { action: { label: 'Undo', onClick: undoDelete }, duration: 6000 })
    },
    [setState, state.device, state.slidesByDevice]
  )

  const addSlide = useCallback(
    (slide: Slide) => {
      setState((prev) => withDeck(prev, prev.device, [...(prev.slidesByDevice[prev.device] || []), slide]), {
        coalesce: false,
      })
      selectSlide(slide.id)
    },
    [setState, selectSlide]
  )

  const duplicateSlide = useCallback(
    (id: string) => {
      const src = currentSlides.find((s) => s.id === id)
      if (!src) {
        return
      }
      const copy = cloneSlide(src)
      setState((prev) => insertSlideAfter(prev, id, copy), { coalesce: false })
      selectSlide(copy.id)
    },
    [setState, currentSlides, selectSlide]
  )

  const patchLocalized = useCallback(
    (slide: Slide, key: 'label' | 'headline', value: string) => {
      const text = writeLocalized(slide[key], state.locale, value)
      patchSlide(slide.id, key === 'label' ? { label: text } : { headline: text })
    },
    [patchSlide, state.locale]
  )

  const patchElementTransform = useCallback(
    (slideId: string, elementId: ElementId, transform: ElementTransform) => {
      setState((prev) =>
        updateSlideInProject(prev, prev.device, slideId, (slide) => updateElementTransform(slide, elementId, transform))
      )
    },
    [setState]
  )

  const patchTextElementText = useCallback(
    (slideId: string, textId: string, value: string) => {
      setState((prev) =>
        updateSlideInProject(prev, prev.device, slideId, (slide) =>
          setTextElementText(slide, textId, prev.locale, value)
        )
      )
    },
    [setState]
  )

  return {
    currentSlides,
    activeSlide,
    selectedElement,
    selectSlide,
    selectElement: setSelection,
    selectActiveElement,
    clearSelection,
    resetSelection,
    patchSlide,
    updateSlide,
    reorderSlides,
    deleteSlide,
    addSlide,
    duplicateSlide,
    patchLocalized,
    patchElementTransform,
    patchTextElementText,
  }
}
