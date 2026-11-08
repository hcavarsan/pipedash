import type { ReactNode } from 'react'
import { relativeLuminance, slideColors } from '@/lib/contrast'
import type { Scene, Slide, Theme } from '@/lib/types'
import { SceneBackdrop } from './scene-backdrop'

// ---------- Decorative blob ----------

export function Blob({
  cW,
  color,
  x,
  y,
  size,
  opacity = 0.4,
}: {
  cW: number
  color: string
  x: number
  y: number
  size: number
  opacity?: number
}): ReactNode {
  return (
    <div
      style={{
        position: 'absolute',
        left: `${x}%`,
        top: `${y}%`,
        width: `${size}%`,
        aspectRatio: '1 / 1',
        background: color,
        borderRadius: '50%',
        filter: `blur(${cW * 0.06}px)`,
        opacity,
        pointerEvents: 'none',
      }}
    />
  )
}

export function SlideBackground({
  slide,
  cW,
  cH,
  theme,
  scene,
  index,
  count,
}: {
  slide: Slide
  cW: number
  cH: number
  theme: Theme
  scene: Scene
  index: number
  count: number
}): ReactNode {
  const inverted = !!slide.inverted
  const colors = slideColors(theme, slide)
  const base = slide.backgroundColor || (inverted ? theme.bgAlt : theme.bg)
  if (theme.id === 'midnight-glow-pro') {
    return (
      <div aria-hidden style={{ position: 'absolute', inset: 0, background: base }}>
        {index < 2 && (
          <div
            style={{
              position: 'absolute',
              top: cH * 0.97,
              width: '100%',
              borderTop: `1px solid ${theme.muted}`,
              opacity: 0.2,
            }}
          />
        )}
      </div>
    )
  }
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        overflow: 'hidden',
        color: colors.fg,
      }}
    >
      <SceneBackdrop
        scene={scene}
        theme={theme}
        cW={cW}
        cH={cH}
        index={index}
        count={count}
        base={base}
        fg={colors.fg}
        accent={theme.accent}
        dark={(relativeLuminance(base) ?? 1) < 0.18}
        inverted={inverted && !slide.backgroundColor}
        blobsInverted={inverted}
      />
    </div>
  )
}
