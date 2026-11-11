import type {
  BuiltInElementId,
  Device,
  ElementId,
  ElementTransform,
  ImageElementId,
  ProjectState,
  Slide,
  TextElementId,
} from './types'

export const BUILT_IN_ELEMENT_IDS: BuiltInElementId[] = ['caption', 'device', 'deviceSecondary', 'callout']

export const TEXT_ELEMENT_PREFIX = 'text:'
export const IMAGE_ELEMENT_PREFIX = 'image:'

export function isBuiltInElementId(id: ElementId | string): id is BuiltInElementId {
  return BUILT_IN_ELEMENT_IDS.some((builtIn) => builtIn === id)
}

export function isTextElementId(id: ElementId | string | null | undefined): id is TextElementId {
  return typeof id === 'string' && id.startsWith(TEXT_ELEMENT_PREFIX)
}

export function isImageElementId(id: ElementId | string | null | undefined): id is ImageElementId {
  return typeof id === 'string' && id.startsWith(IMAGE_ELEMENT_PREFIX)
}

export function toTextElementId(id: string): TextElementId {
  return `${TEXT_ELEMENT_PREFIX}${id}`
}

export function toImageElementId(id: string): ImageElementId {
  return `${IMAGE_ELEMENT_PREFIX}${id}`
}

export function textElementKey(id: TextElementId | ElementId): string {
  return isTextElementId(id) ? id.slice(TEXT_ELEMENT_PREFIX.length) : id
}

export function imageElementKey(id: ImageElementId | ElementId): string {
  return isImageElementId(id) ? id.slice(IMAGE_ELEMENT_PREFIX.length) : id
}

export function updateSlideInProject(
  project: ProjectState,
  device: Device,
  slideId: string,
  update: (slide: Slide) => Slide
): ProjectState {
  return {
    ...project,
    slidesByDevice: {
      ...project.slidesByDevice,
      [device]: (project.slidesByDevice[device] || []).map((slide) => (slide.id === slideId ? update(slide) : slide)),
    },
  }
}

export function updateElementTransform(slide: Slide, id: ElementId, transform: ElementTransform): Slide {
  if (isTextElementId(id)) {
    const textId = textElementKey(id)
    return {
      ...slide,
      textElements: (slide.textElements || []).map((element) =>
        element.id === textId ? { ...element, transform } : element
      ),
    }
  }
  if (isImageElementId(id)) {
    const imageId = imageElementKey(id)
    return {
      ...slide,
      imageElements: (slide.imageElements || []).map((element) =>
        element.id === imageId ? { ...element, transform } : element
      ),
    }
  }
  return { ...slide, transforms: { ...slide.transforms, [id]: transform } }
}
