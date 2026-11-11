'use client'
// Visual layers driven by the deck-wide Scene (lib/scene.ts): backdrops,
// decorations, device depth and the magnified callout. Everything here is
// plain DOM/CSS or inline SVG so html-to-image exports it exactly as previewed.
import type { ReactNode } from 'react'
import { alpha, hash01, shade } from '@/lib/scene-colors'
import { type BackdropProps, SceneDecor } from './scene-decor'

/**
 * One screen's backdrop. With `scene.span`, the art layer is laid out across
 * the whole strip and this screen shows its slice, so shapes flow over seams.
 * The base colour stays per screen, so inverted and custom-colour screens keep
 * their own background.
 */
export function SceneBackdrop(props: BackdropProps): ReactNode {
  const { scene, cW, cH, index, count, base } = props
  const span = scene.span && count > 1
  const stripW = span ? cW * count : cW
  const offset = span ? -index * cW : 0
  const baseFill =
    scene.backdrop === 'gradient'
      ? `linear-gradient(160deg, ${base} 0%, ${shade(base, props.inverted ? -8 : -6)} 100%)`
      : base
  return (
    <div
      aria-hidden
      style={{ position: 'absolute', inset: 0, overflow: 'hidden', background: baseFill, pointerEvents: 'none' }}
    >
      <div
        data-scene-art={scene.backdrop}
        style={{
          position: 'absolute',
          left: offset,
          top: 0,
          width: stripW,
          height: cH,
          background: backdropArt(props, stripW, span),
        }}
      />
      {scene.backdrop === 'spotlight' && (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            background: `radial-gradient(ellipse ${cW * 0.75}px ${cH * 0.62}px at ${cW / 2}px ${cH * 0.45}px, transparent 55%, rgba(0,0,0,${props.dark ? 0.45 : 0.16}) 100%)`,
          }}
        />
      )}
      <SceneDecor {...props} stripW={stripW} offset={offset} span={span} />
    </div>
  )
}

function backdropArt(p: BackdropProps, stripW: number, span: boolean): string | undefined {
  const { cW, cH, fg, accent, dark, theme } = p
  const second = theme.accentAlt && theme.accentAlt !== accent ? theme.accentAlt : theme.muted
  const unit = Math.min(cW, cH)
  switch (p.scene.backdrop) {
    case 'gradient':
      return span
        ? `linear-gradient(100deg, transparent 0%, ${alpha(accent, 0.16)} 28%, transparent 52%, ${alpha(accent, 0.12)} 78%, transparent 100%)`
        : undefined
    case 'solid':
      return undefined
    case 'aurora': {
      // A row of soft colour pools; spanning decks get one per ~0.7 screens so
      // pools straddle the seams.
      const pools = Math.max(2, Math.round((stripW / cW) * (span ? 1.4 : 2)))
      const palette = [accent, second, fg]
      const poolStrength = dark ? 0.55 : 0.42
      const layers: string[] = []
      for (let i = 0; i < pools; i++) {
        const x = ((i + 0.5) / pools) * stripW + (hash01(i + 3) - 0.5) * cW * 0.25
        const y = (i % 2 === 0 ? 0.18 : 0.78) * cH + (hash01(i + 11) - 0.5) * cH * 0.12
        const color = palette[i % 3] ?? accent
        const strength = i % 3 === 2 ? 0.12 : poolStrength
        const r = cW * (0.75 + hash01(i + 5) * 0.35)
        layers.push(`radial-gradient(circle at ${x}px ${y}px, ${alpha(color, strength)} 0px, transparent ${r}px)`)
      }
      return layers.join(', ')
    }
    case 'spotlight': {
      const lights: string[] = []
      const screens = Math.round(stripW / cW)
      for (let i = 0; i < screens; i++) {
        const x = span ? (i + 0.5 + (i % 2 === 0 ? -0.32 : 0.32)) * cW : cW * 0.5
        const light = dark ? alpha(accent, 0.42) : 'rgba(255,255,255,0.75)'
        lights.push(
          `radial-gradient(ellipse ${cW * 0.7}px ${cH * 0.45}px at ${x}px ${cH * 0.12}px, ${light} 0px, transparent 100%)`
        )
      }
      // The vignette is a separate, unshifted layer (see SceneBackdrop) so
      // every screen gets its own dark corners even when the lights span.
      return lights.join(', ')
    }
    case 'grid': {
      const g = Math.round(unit * 0.08)
      const line = alpha(fg, dark ? 0.1 : 0.08)
      const w = Math.max(1, Math.round(unit * 0.0016))
      return [
        `radial-gradient(ellipse ${cW * 0.8}px ${cH * 0.4}px at ${stripW / 2}px ${cH * 0.55}px, ${alpha(accent, 0.18)} 0px, transparent 100%)`,
        `repeating-linear-gradient(90deg, ${line} 0px, ${line} ${w}px, transparent ${w}px, transparent ${g}px)`,
        `repeating-linear-gradient(0deg, ${line} 0px, ${line} ${w}px, transparent ${w}px, transparent ${g}px)`,
      ].join(', ')
    }
    case 'dots': {
      const g = Math.round(unit * 0.055)
      const dot = alpha(fg, dark ? 0.2 : 0.14)
      const r = Math.max(1.5, unit * 0.0045)
      return `radial-gradient(circle at ${g / 2}px ${g / 2}px, ${dot} 0px, ${dot} ${r}px, transparent ${r + 0.8}px) 0 0 / ${g}px ${g}px`
    }
    case 'lines': {
      const g = Math.round(unit * 0.06)
      const line = alpha(fg, dark ? 0.1 : 0.09)
      const w = Math.max(1, Math.round(unit * 0.0016))
      return `repeating-linear-gradient(180deg, transparent 0px, transparent ${g - w}px, ${line} ${g - w}px, ${line} ${g}px)`
    }
  }
}
