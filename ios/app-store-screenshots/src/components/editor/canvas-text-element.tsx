'use client'
import type { ReactNode } from 'react'
import { slideColors } from '@/lib/contrast'
import { toTextElementId } from '@/lib/elements'
import { pickText } from '@/lib/locale'
import type { Slide, TextElement, Theme } from '@/lib/types'
import { defaultTextElementFontSize } from '@/lib/typography'
import type { CssVars, ElementLayer } from './canvas-types'
import { EditableText } from './editable-text'
import { LayerMovable } from './layer-movable'

const JUSTIFY = { right: 'flex-end', left: 'flex-start', center: 'center' } as const

function textShadowFor(midnight: boolean, dark: boolean): string {
  if (midnight) {
    return 'none'
  }
  return dark ? '0 2px 18px rgba(0,0,0,0.22)' : '0 2px 18px rgba(255,255,255,0.2)'
}

export function CanvasTextElement({
  layer,
  slide,
  theme,
  locale,
  element,
  index,
  cW,
  cH,
}: {
  layer: ElementLayer
  slide: Slide
  theme: Theme
  locale: string
  element: TextElement
  index: number
  cW: number
  cH: number
}): ReactNode {
  const elementId = toTextElementId(element.id)
  const rect = element.transform
  const colors = slideColors(theme, slide)
  const midnight = theme.id === 'midnight-glow-pro'
  const boxStyle: CssVars = {
    width: '100%',
    height: '100%',
    display: 'flex',
    alignItems: 'center',
    justifyContent: JUSTIFY[element.align ?? 'center'],
    padding: `${Math.min(cW, cH) * 0.012}px`,
    ...(midnight ? { '--pipedash-accent': colors.accent, '--pipedash-border': theme.muted } : {}),
  }
  const featureCard = midnight && slide.layout === 'no-device' && element.id !== 'wordmark'
  return (
    <LayerMovable
      layer={layer}
      elementId={elementId}
      rect={rect}
      rotation={rect.rotation ?? 0}
      zIndex={rect.zIndex ?? 5 + index}
      allowOverflow={layer.allowCrossScreen}
    >
      <div className={featureCard ? 'midnight-feature-card' : undefined} style={boxStyle}>
        <EditableText
          value={pickText(element.text, locale)}
          editable={layer.editable}
          multiline
          onChange={(value) => layer.edit?.onTextElementTextChange?.(element.id, value)}
          onFocus={() => layer.edit?.onSelectElement?.(elementId)}
          placeholder="Text"
          emphasis={midnight}
          className={midnight ? 'midnight-feature-copy' : undefined}
          style={{
            width: '100%',
            color: element.color || colors.fg,
            fontSize: element.fontSize ?? defaultTextElementFontSize(cW, cH),
            fontWeight: element.fontWeight ?? (midnight ? 500 : 700),
            lineHeight: midnight ? 1.32 : 1.05,
            textAlign: element.align ?? 'center',
            textShadow: textShadowFor(midnight, colors.dark),
          }}
        />
      </div>
    </LayerMovable>
  )
}
