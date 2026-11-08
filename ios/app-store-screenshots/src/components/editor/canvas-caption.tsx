'use client'
import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from 'react'
import { slideColors } from '@/lib/contrast'
import { pickText } from '@/lib/locale'
import type { Scene, Slide, Theme } from '@/lib/types'
import { slideFontScales } from '@/lib/typography'
import type { CssVars, EditHandlers } from './canvas-types'
import { EditableText } from './editable-text'

type Props = {
  cW: number
  cH: number
  slide: Slide
  theme: Theme
  locale: string
  editable?: boolean | undefined
  edit?: EditHandlers | undefined
  align?: 'center' | 'left' | undefined
  inverted: boolean
  scene: Scene
  onFocus?: (() => void) | undefined
  /** Typography unit; defaults to the canvas's shorter side. */
  typeUnit?: number | undefined
}

// Shrinks the headline until its longest line fits the caption's width.
function fitHeadlineSize(el: HTMLElement, headline: string, baseSize: number, cW: number): number | null {
  const context = document.createElement('canvas').getContext('2d')
  if (!context) {
    return null
  }
  const family = getComputedStyle(el).fontFamily
  const lines = headline.replace(/\*/g, '').split('\n')
  const fits = (size: number) => {
    context.font = `600 ${size}px ${family}`
    return lines.every(
      (line) => context.measureText(line).width - Math.max(0, line.length - 1) * size * 0.02 <= el.clientWidth
    )
  }
  let size = baseSize
  while (size > 1 && !fits(size)) {
    size -= Math.min((8 * cW) / 1320, size - 1)
  }
  return size
}

function headlineLetterSpacing(scene: Scene, midnight: boolean, unit: number, headlineScale: number): number | string {
  if (midnight) {
    return '-.02em'
  }
  return (scene.headlineCase === 'upper' ? unit * 0.0015 : -unit * 0.001) * headlineScale
}

export function Caption({
  cW,
  cH,
  slide,
  theme,
  locale,
  editable,
  edit,
  align = 'center',
  inverted,
  scene,
  onFocus,
  typeUnit,
}: Props): ReactNode {
  const { fg, accent } = slideColors(theme, { ...slide, inverted })
  const { labelScale, headlineScale } = slideFontScales(slide)
  // Scale typography off the *shorter* dimension so landscape layouts don't
  // produce headlines so tall they overlap the device frame.
  const unit = typeUnit ?? Math.min(cW, cH)
  const midnight = theme.id === 'midnight-glow-pro'
  const captionRef = useRef<HTMLDivElement>(null)
  const baseSize = unit * (midnight ? 0.114 : 0.092) * headlineScale * (midnight ? 1 : scene.headlineScale)
  const [fittedSize, setFittedSize] = useState(baseSize)
  const headline = pickText(slide.headline, locale)
  useEffect(() => {
    const el = captionRef.current
    if (!midnight || !el) {
      return
    }
    let active = true
    const fit = () => {
      if (!active) {
        return
      }
      const size = fitHeadlineSize(el, headline, baseSize, cW)
      if (size !== null) {
        setFittedSize(size)
      }
    }
    const observer = new ResizeObserver(fit)
    observer.observe(el)
    void document.fonts.ready.then(fit)
    return () => {
      active = false
      observer.disconnect()
    }
  }, [midnight, baseSize, headline, cW])

  const rootStyle: CssVars = {
    '--pipedash-accent': accent,
    textAlign: align === 'left' ? 'start' : align,
    position: 'relative',
    width: '100%',
  }
  const labelStyle: CSSProperties = {
    // Floor keeps the label legible on the 422×514 Apple Watch canvas;
    // the per-slide scale is applied on top of the floored base.
    fontSize: midnight ? unit * 0.022 * labelScale : Math.max(unit * 0.028, 16) * labelScale,
    fontWeight: midnight ? 500 : 600,
    letterSpacing: midnight ? '.02em' : Math.max(unit * 0.0015, 0.8) * labelScale,
    color: midnight ? theme.muted : accent,
    textTransform: 'uppercase',
    marginBottom: unit * (midnight ? 0.04 : 0.018),
    minHeight: unit * 0.03,
  }
  const headlineStyle: CSSProperties = {
    fontSize: midnight ? Math.min(baseSize, fittedSize) : baseSize,
    fontWeight: midnight ? 600 : scene.headlineWeight,
    lineHeight: midnight ? 1.08 : 0.96,
    letterSpacing: headlineLetterSpacing(scene, midnight, unit, headlineScale),
    textTransform: !midnight && scene.headlineCase === 'upper' ? 'uppercase' : undefined,
    color: fg,
  }
  return (
    // "start" rather than "left" so a left-set caption hugs the right edge in RTL.
    <div ref={captionRef} data-midnight={midnight || undefined} style={rootStyle}>
      <EditableText
        value={pickText(slide.label, locale)}
        editable={editable}
        onChange={edit?.onLabelChange}
        onFocus={onFocus}
        placeholder="LABEL"
        emphasis={midnight}
        style={labelStyle}
      />
      <EditableText
        value={headline}
        editable={editable}
        multiline
        onChange={edit?.onHeadlineChange}
        onFocus={onFocus}
        placeholder="Headline goes here"
        emphasis={midnight}
        className={midnight ? 'midnight-headline' : undefined}
        style={headlineStyle}
      />
    </div>
  )
}
