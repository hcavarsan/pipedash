'use client'
import type { ReactNode } from 'react'
import { useState } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { cleanHexColor } from '@/lib/clean-hex-color'
import type { Patch, Slide, Theme } from '@/lib/types'

type Props = {
  slide: Slide
  theme: Theme
  onChange: (patch: Patch<Slide>) => void
}

function backgroundMode(slide: Slide): 'custom' | 'alternate' | 'theme' {
  if (slide.backgroundColor) {
    return 'custom'
  }
  return slide.inverted ? 'alternate' : 'theme'
}

export function BackgroundControls({ slide, theme, onChange }: Props): ReactNode {
  const mode = backgroundMode(slide)
  const customColor = cleanHexColor(slide.backgroundColor) || cleanHexColor(theme.bg) || '#FFFFFF'
  // Keep partial hex input local so typing "#0B" or clearing the field doesn't
  // write an invalid color into the slide (or drop it out of custom mode).
  const [draft, setDraft] = useState<{ source: string; text: string } | null>(null)
  const hexText = draft?.source === customColor ? draft.text : customColor

  function changeHex(value: string) {
    const next = value.toUpperCase()
    setDraft({ source: customColor, text: next })
    const color = cleanHexColor(next)
    if (color) {
      onChange({ backgroundColor: color })
    }
  }
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">Background</Label>
      <Select
        value={mode}
        onValueChange={(nextMode) => {
          if (nextMode === 'theme') {
            onChange({ inverted: false, backgroundColor: undefined })
          }
          if (nextMode === 'alternate') {
            onChange({ inverted: true, backgroundColor: undefined })
          }
          if (nextMode === 'custom') {
            onChange({ inverted: false, backgroundColor: customColor })
          }
        }}
      >
        <SelectTrigger aria-label="Background">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="theme">Theme background</SelectItem>
          <SelectItem value="alternate">Theme alternate</SelectItem>
          <SelectItem value="custom">Custom color</SelectItem>
        </SelectContent>
      </Select>
      {mode === 'custom' && (
        <div className="flex items-center gap-2">
          <input
            type="color"
            value={customColor}
            onChange={(event) => onChange({ backgroundColor: event.target.value.toUpperCase() })}
            className="h-8 w-10 cursor-pointer rounded border bg-transparent p-0.5"
            aria-label="Custom background color"
          />
          <Input
            value={hexText}
            onChange={(event) => changeHex(event.target.value)}
            onBlur={() => setDraft(null)}
            placeholder="#0B0908"
            className="h-8 font-mono text-xs uppercase"
            aria-label="Custom background hex color"
          />
        </div>
      )}
      {mode === 'custom' && (
        <p className="text-[11px] text-muted-foreground">
          Caption text switches to a readable colour when the theme&apos;s colours would be too faint.
        </p>
      )}
    </div>
  )
}
