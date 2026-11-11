import { RotateCcw } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import type { Patch, Slide, SlideTypography } from '@/lib/types'
import { cleanTypography, FONT_SCALE_DEFAULT, FONT_SCALE_MAX, FONT_SCALE_MIN, slideFontScales } from '@/lib/typography'
import { ResetButton } from './reset-button'

export function TypographySection({
  slide,
  isFeatureGraphic,
  onChange,
}: {
  slide: Slide
  isFeatureGraphic: boolean
  onChange: (patch: Patch<Slide>) => void
}): ReactNode {
  const scales = slideFontScales(slide)
  const customized = !!cleanTypography(slide.typography)

  function patchTypography(patch: Partial<SlideTypography>) {
    onChange({
      typography: cleanTypography({ ...slide.typography, ...patch }),
    })
  }

  return (
    <div className="space-y-3 rounded-md border bg-muted/30 p-3">
      <div className="flex items-start justify-between gap-2">
        <div>
          <Label className="text-xs font-semibold">Text size</Label>
          <p className="text-[11px] text-muted-foreground">Relative to the layout default.</p>
        </div>
        {customized && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-6 shrink-0 gap-1 px-1.5 text-[11px] text-muted-foreground"
            onClick={() => onChange({ typography: undefined })}
            title="Reset all text sizes to 100%"
          >
            <RotateCcw className="h-3 w-3" />
            Reset
          </Button>
        )}
      </div>
      {!isFeatureGraphic && (
        <FontScaleSlider
          label="Label"
          value={scales.labelScale}
          onChange={(value) => patchTypography({ labelScale: value })}
        />
      )}
      {isFeatureGraphic && (
        <FontScaleSlider
          label="App name"
          value={scales.appNameScale}
          onChange={(value) => patchTypography({ appNameScale: value })}
        />
      )}
      <FontScaleSlider
        label={isFeatureGraphic ? 'Tagline' : 'Headline'}
        value={scales.headlineScale}
        onChange={(value) => patchTypography({ headlineScale: value })}
      />
    </div>
  )
}

function FontScaleSlider({
  label,
  value,
  onChange,
}: {
  label: string
  value: number
  onChange: (value: number) => void
}): ReactNode {
  const pct = Math.round(value * 100)
  const minPct = Math.round(FONT_SCALE_MIN * 100)
  const maxPct = Math.round(FONT_SCALE_MAX * 100)
  const defaultPct = Math.round(FONT_SCALE_DEFAULT * 100)

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-[11px] text-muted-foreground">{label}</Label>
        <div className="flex items-center gap-1">
          <ResetButton
            visible={pct !== defaultPct}
            label={`Reset ${label.toLowerCase()} to ${defaultPct}%`}
            onClick={() => onChange(FONT_SCALE_DEFAULT)}
          />
          <span className="w-9 text-right text-[11px] tabular-nums text-muted-foreground">{pct}%</span>
        </div>
      </div>
      <input
        type="range"
        min={minPct}
        max={maxPct}
        step={5}
        value={pct}
        onChange={(event) => onChange(Number(event.target.value) / 100)}
        onDoubleClick={() => onChange(FONT_SCALE_DEFAULT)}
        className="w-full"
        aria-label={`${label} size`}
        aria-valuetext={`${pct}%`}
        title="Double-click to reset"
      />
    </div>
  )
}
