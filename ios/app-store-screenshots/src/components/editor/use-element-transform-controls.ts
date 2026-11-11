import { useEffect, useRef } from 'react'
import { getCanvas } from '@/lib/canvas-device'
import { calloutAvailable, getElementTransform } from '@/lib/canvas-geometry'
import { nid } from '@/lib/defaults'
import { defaultZ, fitToAspect, naturalSize, type ReorderDirection, reorderTarget } from '@/lib/element-geometry'
import {
  imageElementKey,
  isImageElementId,
  isTextElementId,
  textElementKey,
  toImageElementId,
  toTextElementId,
  updateElementTransform,
} from '@/lib/elements'
import { img } from '@/lib/image-cache'
import { writeLocalized } from '@/lib/locale'
import type {
  Device,
  ElementId,
  ElementTransform,
  ImageElement,
  Orientation,
  Patch,
  Slide,
  TextElement,
} from '@/lib/types'

type Params = {
  slide: Slide
  device: Device
  orientation: Orientation
  locale: string
  selectedElementId: ElementId | null
  onChange: (patch: Patch<Slide>) => void
  /** Patch computed from the slide's latest state (safe after async work). */
  onUpdate: (update: (slide: Slide) => Patch<Slide>) => void
  onSelectElement: (id: ElementId | null) => void
}

type ElementTransformControls = {
  activeId: ElementId | null
  transform: ElementTransform | undefined
  textElement: TextElement | undefined
  imageElement: ImageElement | undefined
  canvas: { cW: number; cH: number }
  addTextElement: () => void
  addImageElement: () => void
  rotate: (rotation: number) => void
  reorder: (dir: ReorderDirection) => void
  setText: (value: string) => void
  patchText: (patch: Patch<TextElement>) => void
  deleteText: () => void
  setImageSource: (src: string) => void
  patchImage: (patch: Patch<ImageElement>) => void
  deleteImage: () => void
}

function presentElementIds(slide: Slide, device: Device): ElementId[] {
  const present: ElementId[] = ['caption']
  if (slide.layout !== 'no-device') {
    present.push('device')
  }
  if (slide.layout === 'two-devices') {
    present.push('deviceSecondary')
  }
  if (slide.callout && calloutAvailable(slide, device)) {
    present.push('callout')
  }
  for (const element of slide.textElements || []) {
    present.push(toTextElementId(element.id))
  }
  for (const element of slide.imageElements || []) {
    present.push(toImageElementId(element.id))
  }
  return present
}

function patchById<T extends { id: string }>(items: T[] | undefined, id: string, patch: Patch<T>): T[] {
  return (items || []).map((item) => (item.id === id ? Object.assign({}, item, patch) : item))
}

function withImageSource(
  element: ImageElement,
  src: string,
  size: { w: number; h: number } | null,
  maxHeight: number
): ImageElement {
  if (!size || element.src) {
    return { ...element, src }
  }
  return { ...element, src, transform: fitToAspect(element.transform, size.w / size.h, maxHeight) }
}

export function useElementTransformControls({
  slide,
  device,
  orientation,
  locale,
  selectedElementId,
  onChange,
  onUpdate,
  onSelectElement,
}: Params): ElementTransformControls {
  const present = presentElementIds(slide, device)
  const activeId = selectedElementId && present.includes(selectedElementId) ? selectedElementId : null
  const transform = activeId ? getElementTransform(slide, device, orientation, activeId) : undefined
  const { cW, cH } = getCanvas(device, orientation)
  const textElement =
    activeId && isTextElementId(activeId)
      ? slide.textElements?.find((element) => element.id === textElementKey(activeId))
      : undefined
  const imageElement =
    activeId && isImageElementId(activeId)
      ? slide.imageElements?.find((element) => element.id === imageElementKey(activeId))
      : undefined

  const imageRequests = useRef(new Map<string, number>())
  useEffect(() => () => imageRequests.current.clear(), [])

  function getTransform(id: ElementId) {
    return getElementTransform(slide, device, orientation, id)
  }

  function topZIndex(): number {
    return Math.max(5, ...present.map((elementId) => getTransform(elementId)?.zIndex ?? defaultZ(elementId))) + 1
  }

  function rotate(rotation: number) {
    if (!activeId) {
      return
    }
    onUpdate((latest) => {
      const current = getElementTransform(latest, device, orientation, activeId)
      return current ? updateElementTransform(latest, activeId, { ...current, rotation }) : {}
    })
  }

  // Z-order: re-rank zIndex among present elements so they remain contiguous.
  function reorder(dir: ReorderDirection) {
    if (!activeId) {
      return
    }
    const ranked = [...present].sort(
      (a, b) => (getTransform(a)?.zIndex ?? defaultZ(a)) - (getTransform(b)?.zIndex ?? defaultZ(b))
    )
    const index = ranked.indexOf(activeId)
    const target = reorderTarget(index, ranked.length - 1, dir)
    if (index === -1 || target === index) {
      return
    }
    ranked.splice(index, 1)
    ranked.splice(target, 0, activeId)
    onUpdate((latest) =>
      ranked.reduce((acc, id, rank) => {
        const current = getElementTransform(acc, device, orientation, id)
        return current ? updateElementTransform(acc, id, { ...current, zIndex: rank + 1 }) : acc
      }, latest)
    )
  }

  function addTextElement() {
    const id = nid()
    const element: TextElement = {
      id,
      text: writeLocalized({}, locale, 'New text'),
      transform: {
        x: cW * 0.18,
        y: cH * 0.42,
        width: cW * 0.64,
        height: cH * 0.12,
        rotation: 0,
        zIndex: topZIndex(),
      },
      fontWeight: 800,
      align: 'center',
    }
    onChange({ textElements: [...(slide.textElements || []), element] })
    onSelectElement(toTextElementId(id))
  }

  function addImageElement() {
    const id = nid()
    const element: ImageElement = {
      id,
      src: '',
      transform: {
        x: cW * 0.25,
        y: cH * 0.36,
        width: cW * 0.5,
        height: cW * 0.5,
        rotation: 0,
        zIndex: topZIndex(),
      },
      fit: 'cover',
    }
    onChange({ imageElements: [...(slide.imageElements || []), element] })
    onSelectElement(toImageElementId(id))
  }

  function patchText(patch: Patch<TextElement>) {
    if (textElement) {
      onChange({ textElements: patchById(slide.textElements, textElement.id, patch) })
    }
  }

  function setText(value: string) {
    if (textElement) {
      patchText({ text: writeLocalized(textElement.text, locale, value) })
    }
  }

  function deleteText() {
    if (!textElement) {
      return
    }
    const remaining = (slide.textElements || []).filter((item) => item.id !== textElement.id)
    onChange({ textElements: remaining.length > 0 ? remaining : undefined })
    onSelectElement(null)
  }

  // Functional updates: an image upload can finish after the user has moved or
  // resized the overlay, and a patch built from the render-time slide would
  // silently revert that move.
  function patchImage(patch: Patch<ImageElement>) {
    if (imageElement) {
      onUpdate((latest) => ({ imageElements: patchById(latest.imageElements, imageElement.id, patch) }))
    }
  }

  // The first image picked for an overlay reshapes its frame to the image's
  // aspect ratio (centred on the old frame), so "Fill frame" doesn't crop a
  // wide logo into a square. Replacing an image keeps the frame as placed.
  async function applyImageSource(id: string, src: string) {
    const request = (imageRequests.current.get(id) || 0) + 1
    imageRequests.current.set(id, request)
    const size = src ? await naturalSize(img(src)) : null
    if (imageRequests.current.get(id) !== request) {
      return
    }
    onUpdate((latest) => ({
      imageElements: (latest.imageElements || []).map((element) =>
        element.id === id ? withImageSource(element, src, size, cH * 0.6) : element
      ),
    }))
  }

  function setImageSource(src: string) {
    if (imageElement) {
      void applyImageSource(imageElement.id, src)
    }
  }

  function deleteImage() {
    if (!imageElement) {
      return
    }
    onUpdate((latest) => {
      const remaining = (latest.imageElements || []).filter((item) => item.id !== imageElement.id)
      return { imageElements: remaining.length > 0 ? remaining : undefined }
    })
    onSelectElement(null)
  }

  return {
    activeId,
    transform,
    textElement,
    imageElement,
    canvas: { cW, cH },
    addTextElement,
    addImageElement,
    rotate,
    reorder,
    setText,
    patchText,
    deleteText,
    setImageSource,
    patchImage,
    deleteImage,
  }
}
