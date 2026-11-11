'use client'
import type { ReactNode } from 'react'
import { Label } from '@/components/ui/label'
import { relativeLuminance } from '@/lib/contrast'
import type { Scene, Theme } from '@/lib/types'
import { cn } from '@/lib/utils'
import { SceneBackdrop } from './scene-backdrop'

const TILE_W = 72
const TILE_H = 48

export function Section({ title, children }: { title: string; children: ReactNode }): ReactNode {
  return (
    <section className="space-y-2 border-t pt-3 first-of-type:border-t-0 first-of-type:pt-0 [&+&]:mt-3">
      <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{title}</h3>
      {children}
    </section>
  )
}

export function TileGroup({ label, children }: { label: string; children: ReactNode }): ReactNode {
  return (
    <fieldset aria-label={label} className="m-0 grid min-w-0 grid-cols-4 gap-1.5 border-0 p-0">
      {children}
    </fieldset>
  )
}

export function Tile({
  group,
  name,
  checked,
  onSelect,
  children,
}: {
  group: string
  name: string
  checked: boolean
  onSelect: () => void
  children: ReactNode
}): ReactNode {
  return (
    <label
      className={cn(
        'group relative flex cursor-pointer flex-col items-center gap-1 rounded-md p-1 text-[10px] outline-hidden transition-colors has-focus-visible:ring-2 has-focus-visible:ring-ring hover:bg-accent',
        checked && 'bg-accent'
      )}
    >
      <input
        type="radio"
        name={group}
        checked={checked}
        aria-checked={checked}
        aria-label={name}
        onChange={onSelect}
        className="absolute inset-0 m-0 cursor-pointer opacity-0"
      />
      <span
        className={cn('relative block overflow-hidden rounded ring-1 ring-black/10', checked && 'ring-2 ring-primary')}
        style={{ width: TILE_W, height: TILE_H }}
      >
        {children}
      </span>
      <span className={cn('text-muted-foreground', checked && 'font-medium text-foreground')}>{name}</span>
    </label>
  )
}

// A real backdrop rendered at tile size, in the deck's own theme colours.
export function Swatch({ theme, scene }: { theme: Theme; scene: Scene }): ReactNode {
  return (
    <SceneBackdrop
      scene={scene}
      theme={theme}
      cW={TILE_W}
      cH={TILE_H}
      index={0}
      count={1}
      base={theme.bg}
      fg={theme.fg}
      accent={theme.accent}
      dark={(relativeLuminance(theme.bg) ?? 1) < 0.18}
      inverted={false}
    />
  )
}

export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  unit = '',
  reset = 0,
  onChange,
}: {
  label: string
  value: number
  min: number
  max: number
  step?: number
  unit?: string
  reset?: number
  onChange: (value: number) => void
}): ReactNode {
  return (
    <div className="space-y-0.5">
      <div className="flex items-center justify-between">
        <Label className="text-[11px] text-muted-foreground">{label}</Label>
        <span className="text-[11px] tabular-nums text-muted-foreground">
          {value}
          {unit}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={() => onChange(reset)}
        className="w-full"
        aria-label={label}
        aria-valuetext={`${value}${unit}`}
        title="Double-click to reset"
      />
    </div>
  )
}

function SegmentOption({
  group,
  name,
  checked,
  onSelect,
}: {
  group: string
  name: string
  checked: boolean
  onSelect: () => void
}): ReactNode {
  const tone = checked
    ? 'bg-secondary font-medium text-foreground shadow-xs'
    : 'text-muted-foreground hover:text-foreground'
  return (
    <label
      className={cn(
        'relative flex-1 cursor-pointer rounded px-1.5 py-1 text-center text-[11px] outline-hidden has-focus-visible:ring-2 has-focus-visible:ring-ring',
        tone
      )}
    >
      <input
        type="radio"
        name={group}
        checked={checked}
        aria-checked={checked}
        aria-label={name}
        onChange={onSelect}
        className="absolute inset-0 m-0 cursor-pointer opacity-0"
      />
      {name}
    </label>
  )
}

export function SpanToggle({
  checked,
  onChange,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
}): ReactNode {
  return (
    <label className="mt-2 flex cursor-pointer items-start gap-2 text-xs">
      <input type="checkbox" className="mt-0.5" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        <span className="font-medium">Flow across screens</span>
        <span className="block text-[11px] text-muted-foreground">
          One continuous backdrop over the whole strip, so shapes cross the seams.
        </span>
      </span>
    </label>
  )
}

export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: { id: T; name: string }[]
  onChange: (value: T) => void
}): ReactNode {
  return (
    <div className="space-y-1">
      <Label className="text-[11px] text-muted-foreground">{label}</Label>
      <fieldset aria-label={label} className="m-0 flex min-w-0 rounded-md border p-0.5">
        {options.map((o) => (
          <SegmentOption
            key={o.id}
            group={label}
            name={o.name}
            checked={value === o.id}
            onSelect={() => onChange(o.id)}
          />
        ))}
      </fieldset>
    </div>
  )
}
