'use client'
import { Dices, Layers, RotateCcw } from 'lucide-react'
import { type ReactNode, useRef } from 'react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { BACKDROPS, DECORATIONS, DEFAULT_SCENE, sameScene, sceneOf, TILT_LIMIT } from '@/lib/scene'
import { DIRECTIONS } from '@/lib/style-directions'
import { generateLook } from '@/lib/style-lab'
import type { Scene, Theme } from '@/lib/types'
import { Section, Segmented, Slider, SpanToggle, Swatch, Tile, TileGroup } from './scene-controls'

const CASE_OPTIONS: { id: Scene['headlineCase']; name: string }[] = [
  { id: 'as-typed', name: 'As typed' },
  { id: 'upper', name: 'UPPER' },
]

const ALIGN_OPTIONS: { id: Scene['captionAlign']; name: string }[] = [
  { id: 'auto', name: 'Auto' },
  { id: 'left', name: 'Left' },
  { id: 'center', name: 'Center' },
]

/** Toolbar popover that edits the deck-wide scene live on the canvas. */
export function ScenePlayground({
  scene: rawScene,
  theme,
  disabled,
  straightDevices,
  onChange,
}: {
  scene: Scene | undefined
  theme: Theme
  disabled?: boolean
  /** The open deck's devices ignore tilt (iPhone Duo). */
  straightDevices?: boolean
  onChange: (scene: Scene | undefined) => void
}): ReactNode {
  const scene = sceneOf(rawScene)
  const classic = sameScene(scene, DEFAULT_SCENE)
  const surprises = useRef(0)

  function patch(next: Partial<Scene>) {
    const merged = { ...scene, ...next }
    onChange(sameScene(merged, DEFAULT_SCENE) ? undefined : merged)
  }

  function surprise() {
    surprises.current += 1
    const seed = Math.floor(Math.random() * 1e6) + surprises.current
    const direction = DIRECTIONS[seed % DIRECTIONS.length]
    if (!direction) {
      return
    }
    const next = generateLook(direction.id, seed).scene
    // Surprise the stage, not the copy: headline styling stays as set.
    patch({
      backdrop: next.backdrop,
      span: next.span,
      decoration: next.decoration,
      shadow: next.shadow,
      glow: next.glow,
      tilt: next.tilt,
    })
  }

  const backdropTiles = BACKDROPS.map((b) => (
    <Tile
      key={b.id}
      group="Backdrop"
      name={b.name}
      checked={scene.backdrop === b.id}
      onSelect={() => patch({ backdrop: b.id })}
    >
      <Swatch theme={theme} scene={{ ...scene, backdrop: b.id, decoration: 'none', span: false }} />
    </Tile>
  ))
  const decorationTiles = DECORATIONS.map((d) => (
    <Tile
      key={d.id}
      group="Decoration"
      name={d.name}
      checked={scene.decoration === d.id}
      onSelect={() => patch({ decoration: d.id })}
    >
      <Swatch theme={theme} scene={{ ...scene, backdrop: 'solid', decoration: d.id, span: false }} />
    </Tile>
  ))

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant={classic ? 'outline' : 'secondary'}
          size="sm"
          className="h-8 gap-1.5 px-2 text-xs"
          disabled={disabled}
          title="Scene: backdrop, decoration, depth and headline style for every screen"
        >
          <Layers className="h-3.5 w-3.5" />
          Scene
        </Button>
      </PopoverTrigger>
      <PopoverContent className="max-h-[min(80vh,640px)] w-88 overflow-y-auto" aria-label="Scene">
        <div className="mb-3 flex items-center justify-between gap-2">
          <div>
            <h2 className="text-sm font-semibold">Scene</h2>
            <p className="text-[11px] text-muted-foreground">Applies to every screen in this project.</p>
          </div>
          <div className="flex items-center gap-1">
            <Button type="button" variant="outline" size="sm" className="h-7 gap-1 px-2 text-xs" onClick={surprise}>
              <Dices className="h-3.5 w-3.5" />
              Surprise me
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => onChange(undefined)}
              disabled={classic}
              title="Back to the classic scene"
              aria-label="Reset scene"
            >
              <RotateCcw className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        <Section title="Backdrop">
          <TileGroup label="Backdrop">{backdropTiles}</TileGroup>
          <SpanToggle checked={scene.span} onChange={(span) => patch({ span })} />
        </Section>

        <Section title="Decoration">
          <TileGroup label="Decoration">{decorationTiles}</TileGroup>
        </Section>

        <Section title="Device depth">
          <Slider
            label="Shadow"
            value={scene.shadow}
            min={0}
            max={100}
            unit="%"
            onChange={(shadow) => patch({ shadow })}
          />
          <Slider label="Glow" value={scene.glow} min={0} max={100} unit="%" onChange={(glow) => patch({ glow })} />
          <Slider
            label="Tilt"
            value={scene.tilt}
            min={-TILT_LIMIT}
            max={TILT_LIMIT}
            unit="°"
            onChange={(tilt) => patch({ tilt })}
          />
          {straightDevices && (
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              iPhone Duo frames stay straight: Apple's guidelines ask for straight-on product images.
            </p>
          )}
        </Section>

        <Section title="Headline">
          <Slider
            label="Weight"
            value={scene.headlineWeight}
            min={300}
            max={900}
            step={100}
            onChange={(headlineWeight) => patch({ headlineWeight })}
            reset={DEFAULT_SCENE.headlineWeight}
          />
          <Slider
            label="Size"
            value={Math.round(scene.headlineScale * 100)}
            min={50}
            max={200}
            step={5}
            unit="%"
            onChange={(pct) => patch({ headlineScale: pct / 100 })}
            reset={100}
          />
          <div className="grid grid-cols-2 gap-2">
            <Segmented
              label="Case"
              value={scene.headlineCase}
              options={CASE_OPTIONS}
              onChange={(headlineCase) => patch({ headlineCase })}
            />
            <Segmented
              label="Align"
              value={scene.captionAlign}
              options={ALIGN_OPTIONS}
              onChange={(captionAlign) => patch({ captionAlign })}
            />
          </div>
        </Section>
      </PopoverContent>
    </Popover>
  )
}
