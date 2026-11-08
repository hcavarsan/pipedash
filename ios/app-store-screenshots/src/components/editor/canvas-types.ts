import type { ComponentType, CSSProperties } from 'react'
import type {
  Device,
  ElementId,
  ElementTransform,
  Orientation,
  Scene,
  SelectedElement,
  Slide,
  Theme,
} from '@/lib/types'
import type { FrameProps } from './frame-types'

export type FrameComp = ComponentType<FrameProps>

/** Inline style that also carries CSS custom properties (`--name`). */
export type CssVars = CSSProperties & Record<`--${string}`, string>

export type EditHandlers = {
  onLabelChange?: ((v: string) => void) | undefined
  onHeadlineChange?: ((v: string) => void) | undefined
  onTextElementTextChange?: ((id: string, v: string) => void) | undefined
  onElementChange?: ((id: ElementId, t: ElementTransform) => void) | undefined
  onSelectElement?: ((id: ElementId | null) => void) | undefined
}

export type SlideCanvasProps = {
  slide: Slide
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appName?: string | undefined
  appIcon?: string | undefined
  fontFamily?: string | undefined
  editable?: boolean | undefined
  edit?: EditHandlers | undefined
  selectedElementId?: ElementId | null | undefined
  // Preview scale (1.0 = full size). Used so react-rnd maps drag deltas correctly
  // when the canvas is rendered inside a CSS-transformed container.
  previewScale?: number | undefined
  /** When true, suppress the "Drop a screenshot here" placeholder. Used for export. */
  hideEmpty?: boolean | undefined
  scene?: Scene | undefined
  /** Where this screen sits in the full deck, for backdrops that span the strip. */
  stripIndex?: number | undefined
  stripCount?: number | undefined
}

export type DeckEditHandlers = {
  onLabelChange?: ((slideId: string, v: string) => void) | undefined
  onHeadlineChange?: ((slideId: string, v: string) => void) | undefined
  onTextElementTextChange?: ((slideId: string, id: string, v: string) => void) | undefined
  onElementChange?: ((slideId: string, id: ElementId, t: ElementTransform) => void) | undefined
  onSelectElement?: ((element: SelectedElement | null) => void) | undefined
  onSelectScreen?: ((slideId: string) => void) | undefined
}

export type DeckCanvasProps = {
  slides: Slide[]
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appName?: string | undefined
  appIcon?: string | undefined
  fontFamily?: string | undefined
  connectedCanvas?: boolean | undefined
  editable?: boolean | undefined
  edit?: DeckEditHandlers | undefined
  selectedElement?: SelectedElement | null | undefined
  activeSlideId?: string | null | undefined
  previewScale?: number | undefined
  hideEmpty?: boolean | undefined
  showGuides?: boolean | undefined
  scene?: Scene | undefined
  /** When `slides` is a slice of a longer deck: its first index and the deck length. */
  stripStart?: number | undefined
  stripCount?: number | undefined
}

export type Rect = { x: number; y: number; width: number; height: number }
export type CaptionRect = Rect & { align?: 'center' | 'left' | undefined }
export type LayoutRects = {
  caption?: CaptionRect
  device?: Rect
  deviceSecondary?: Rect
  callout?: Rect
}

/** What every movable layer of a screen shares: where the screen sits and how edits flow out. */
export type ElementLayer = {
  editable?: boolean | undefined
  edit?: EditHandlers | undefined
  selectedElementId: ElementId | null
  previewScale: number
  screenX: number
  boundsW: number
  boundsH: number
  allowCrossScreen: boolean
}
