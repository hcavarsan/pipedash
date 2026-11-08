import type { CSSProperties } from 'react'

export type FrameProps = {
  src: string
  alt?: string | undefined
  style?: CSSProperties | undefined
  /** When true, hide EmptySlot placeholder (so it doesn't bake into exports). */
  hideEmpty?: boolean | undefined
}
