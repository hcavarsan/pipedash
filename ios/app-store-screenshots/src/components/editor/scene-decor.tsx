'use client'
import type { ReactNode } from 'react'
import { hash01 } from '@/lib/scene-colors'
import type { Scene, Theme } from '@/lib/types'

export type BackdropProps = {
  scene: Scene
  theme: Theme
  cW: number
  cH: number
  /** Position of this screen in the strip and the strip length. */
  index: number
  count: number
  base: string
  fg: string
  accent: string
  dark: boolean
  /** Theme's inverted background (not a custom colour); matches the classic gradient depth. */
  inverted: boolean
  /** The slide's own `inverted` flag; the classic blobs key their opacity off it. */
  blobsInverted?: boolean | undefined
}

type Anchor = { x: number; y: number; k: number }
type DecorProps = BackdropProps & { stripW: number; offset: number; span: boolean }

const RINGS = [
  { id: 'inner', r: 0.22, step: 0 },
  { id: 'middle', r: 0.32, step: 1 },
  { id: 'outer', r: 0.42, step: 2 },
]

// Spanning decks centre shapes on the seams between screens.
function anchorsFor(span: boolean, count: number, cW: number, cH: number): Anchor[] {
  if (!span) {
    return [
      { x: -0.05 * cW, y: 0.06 * cH, k: 0 },
      { x: 0.98 * cW, y: 0.9 * cH, k: 1 },
    ]
  }
  const anchors: Anchor[] = []
  for (let i = 0; i <= count; i++) {
    anchors.push({ x: i * cW, y: (i % 2 === 0 ? 0.12 : 0.86) * cH, k: i })
  }
  return anchors
}

function blobOpacity(dark: boolean, k: number): number {
  const even = k % 2 === 0
  if (dark) {
    return even ? 0.25 : 0.18
  }
  return even ? 0.32 : 0.25
}

function ClassicBlobs({ cW, cH, accent, lit }: { cW: number; cH: number; accent: string; lit: boolean }): ReactNode {
  // The classic pair, exactly as decks rendered before scenes existed.
  const blobs = [
    { id: 'top', left: -0.15, top: -0.1, size: 0.55, opacity: lit ? 0.25 : 0.32 },
    { id: 'bottom', left: 0.7, top: 0.75, size: 0.45, opacity: lit ? 0.18 : 0.25 },
  ]
  return (
    <>
      {blobs.map((b) => (
        <div
          key={b.id}
          style={{
            position: 'absolute',
            left: b.left * cW,
            top: b.top * cH,
            width: b.size * cW,
            height: b.size * cW,
            background: accent,
            borderRadius: '50%',
            filter: `blur(${cW * 0.06}px)`,
            opacity: b.opacity,
          }}
        />
      ))}
    </>
  )
}

function SeamBlobs({ anchors, props }: { anchors: Anchor[]; props: DecorProps }): ReactNode {
  const { cW, cH, accent, dark, stripW, offset } = props
  return (
    <div style={{ position: 'absolute', left: offset, top: 0, width: stripW, height: cH }}>
      {anchors.map(({ x, y, k }) => {
        const size = cW * (k % 2 === 0 ? 0.55 : 0.45)
        return (
          <div
            key={k}
            style={{
              position: 'absolute',
              left: x - size / 2,
              top: y - size / 2,
              width: size,
              height: size,
              borderRadius: '50%',
              background: accent,
              filter: `blur(${cW * 0.06}px)`,
              opacity: blobOpacity(dark, k),
            }}
          />
        )
      })}
    </div>
  )
}

function Ring({ anchor, ring, props }: { anchor: Anchor; ring: (typeof RINGS)[number]; props: DecorProps }): ReactNode {
  const { cW, accent, dark } = props
  return (
    <circle
      cx={anchor.x}
      cy={anchor.y}
      r={cW * ring.r}
      fill="none"
      stroke={accent}
      strokeWidth={Math.max(2, cW * 0.004)}
      opacity={dark ? 0.32 - ring.step * 0.07 : 0.4 - ring.step * 0.09}
    />
  )
}

function DecorSvg({ props, children }: { props: DecorProps; children: ReactNode }): ReactNode {
  const { stripW, cH, offset } = props
  return (
    <svg
      aria-hidden="true"
      width={stripW}
      height={cH}
      viewBox={`0 0 ${stripW} ${cH}`}
      style={{ position: 'absolute', left: offset, top: 0, overflow: 'visible' }}
    >
      {children}
    </svg>
  )
}

function Rings({ anchors, props }: { anchors: Anchor[]; props: DecorProps }): ReactNode {
  return (
    <DecorSvg props={props}>
      {anchors.flatMap((anchor) =>
        RINGS.map((ring) => <Ring key={`${anchor.k}-${ring.id}`} anchor={anchor} ring={ring} props={props} />)
      )}
    </DecorSvg>
  )
}

type Star = { id: string; x: number; y: number; s: number; o: number }

function starsFor({ stripW, cW, cH, span, index }: DecorProps): Star[] {
  const screens = Math.max(1, Math.round(stripW / cW))
  const stars: Star[] = []
  for (let i = 0; i < screens; i++) {
    for (let j = 0; j < 5; j++) {
      // Isolated screens each get their own scatter rather than a repeat.
      const n = (span ? i : index) * 7 + j
      const yBand = j % 2 === 0 ? 0.04 + hash01(n + 1) * 0.26 : 0.62 + hash01(n + 2) * 0.32
      stars.push({
        id: `${i}-${j}`,
        x: (i + 0.06 + hash01(n + 3) * 0.88) * cW,
        y: yBand * cH,
        s: cW * (0.025 + hash01(n + 4) * 0.045),
        o: 0.45 + hash01(n + 5) * 0.45,
      })
    }
  }
  return stars
}

// Sparkles: four-point stars scattered with a fixed seed per screen.
function Sparkles({ props }: { props: DecorProps }): ReactNode {
  return (
    <DecorSvg props={props}>
      {starsFor(props).map(({ id, x, y, s, o }) => (
        <path
          key={id}
          d={`M ${x} ${y - s} Q ${x} ${y} ${x + s} ${y} Q ${x} ${y} ${x} ${y + s} Q ${x} ${y} ${x - s} ${y} Q ${x} ${y} ${x} ${y - s} Z`}
          fill={props.accent}
          opacity={props.dark ? o : o * 0.85}
        />
      ))}
    </DecorSvg>
  )
}

export function SceneDecor(props: DecorProps): ReactNode {
  const { scene, cW, cH, inverted, blobsInverted, count, span, accent } = props
  const { decoration } = scene
  if (decoration === 'none') {
    return null
  }
  if (decoration === 'sparkles') {
    return <Sparkles props={props} />
  }
  if (decoration === 'blobs' && !span) {
    return <ClassicBlobs cW={cW} cH={cH} accent={accent} lit={blobsInverted ?? inverted} />
  }
  const anchors = anchorsFor(span, count, cW, cH)
  return decoration === 'blobs' ? (
    <SeamBlobs anchors={anchors} props={props} />
  ) : (
    <Rings anchors={anchors} props={props} />
  )
}
