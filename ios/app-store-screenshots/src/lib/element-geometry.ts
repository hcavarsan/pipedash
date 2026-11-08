import { isBuiltInElementId, isImageElementId } from './elements'
import type { BuiltInElementId, ElementId, ElementTransform } from './types'

export type ReorderDirection = 'front' | 'back' | 'up' | 'down'

const ELEMENT_LABEL: Record<BuiltInElementId, string> = {
  caption: 'Headline',
  device: 'Device',
  deviceSecondary: 'Back device',
  callout: 'Magnifier',
}

export function naturalSize(src: string): Promise<{ w: number; h: number } | null> {
  if (!src) {
    return Promise.resolve(null)
  }
  return new Promise((resolve) => {
    const image = new Image()
    const finish = (size: { w: number; h: number } | null) => {
      clearTimeout(timeout)
      image.onload = null
      image.onerror = null
      resolve(size)
    }
    const timeout = setTimeout(() => finish(null), 10000)
    image.onload = () =>
      finish(
        image.naturalWidth > 0 && image.naturalHeight > 0 ? { w: image.naturalWidth, h: image.naturalHeight } : null
      )
    image.onerror = () => finish(null)
    image.src = src
  })
}

export function fitToAspect(t: ElementTransform, aspect: number, maxHeight: number): ElementTransform {
  let width = t.width
  let height = width / aspect
  if (height > maxHeight) {
    height = maxHeight
    width = height * aspect
  }
  return {
    ...t,
    x: t.x + (t.width - width) / 2,
    y: t.y + (t.height - height) / 2,
    width,
    height,
  }
}

export function elementLabel(id: ElementId): string {
  if (isBuiltInElementId(id)) {
    return ELEMENT_LABEL[id]
  }
  if (isImageElementId(id)) {
    return 'Image'
  }
  return 'Text'
}

export function defaultZ(id: ElementId): number {
  if (id === 'deviceSecondary') {
    return 2
  }
  if (id === 'device') {
    return 3
  }
  if (id === 'caption') {
    return 4 // caption on top
  }
  return 5
}

export function reorderTarget(index: number, last: number, dir: ReorderDirection): number {
  if (dir === 'front') {
    return last
  }
  if (dir === 'back') {
    return 0
  }
  return dir === 'up' ? Math.min(last, index + 1) : Math.max(0, index - 1)
}
