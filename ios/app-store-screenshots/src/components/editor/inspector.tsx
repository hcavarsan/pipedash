'use client'
import type { ReactNode } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { calloutAvailable } from '@/lib/canvas-geometry'
import { slideColors } from '@/lib/contrast'
import { DEVICE_LABEL, isCreative } from '@/lib/device-specs'
import { DUO_COMPANION, isDuoDevice } from '@/lib/frame-assets'
import { layoutHint, layoutLabel } from '@/lib/layout-labels'
import { pickText, resolveScreenshot, writeLocalized } from '@/lib/locale'
import type { Device, ElementId, Orientation, Patch, Slide, SlideLayout, Theme } from '@/lib/types'
import { BackgroundControls } from './background-controls'
import { CalloutControls } from './callout-controls'
import { CaptureNote } from './capture-note'
import { CopyIdeasMenu } from './copy-ideas-menu'
import { ElementTransformControls } from './element-transform-controls'
import { LayoutSelect } from './layout-select'
import { ScreenshotPicker } from './screenshot-picker'
import { TypographySection } from './typography-section'

type Props = {
  slide: Slide
  device: Device
  orientation: Orientation
  theme: Theme
  locale: string
  appIcon?: string | undefined
  onAppIconChange: (src: string) => void
  selectedElementId: ElementId | null
  onChange: (patch: Patch<Slide>) => void
  /** Patch computed from the slide's latest state (safe after async work). */
  onUpdate: (update: (slide: Slide) => Patch<Slide>) => void
  onSelectElement: (id: ElementId | null) => void
}

const LAYOUTS: readonly SlideLayout[] = [
  'hero',
  'device-bottom',
  'device-top',
  'two-devices',
  'no-device',
  'split-landscape',
  'feature-graphic',
]

const isLayout = (value: string): value is SlideLayout => LAYOUTS.some((layout) => layout === value)

export function Inspector({
  slide,
  device,
  orientation,
  theme,
  locale,
  appIcon,
  onAppIconChange,
  selectedElementId,
  onChange,
  onUpdate,
  onSelectElement,
}: Props): ReactNode {
  const isFeatureGraphic = device === 'feature-graphic' || slide.layout === 'feature-graphic'
  const isNoDevice = slide.layout === 'no-device'
  const layoutValue = device === 'feature-graphic' ? 'feature-graphic' : slide.layout
  const layoutOptions = LAYOUTS.filter((layout) =>
    device === 'feature-graphic' ? layout === 'feature-graphic' : layout !== 'feature-graphic'
  ).map((layout) => [layout, layoutLabel(layout, device)] as const)
  // On iPhone Duo the back device is the other display, so it needs its own capture.
  const companion = isDuoDevice(device) ? DUO_COMPANION[device] : null
  const localeLabel = slide.label?.[locale] ?? ''
  const localeHeadline = slide.headline?.[locale] ?? ''
  // When the active locale is empty, surface the fallback (typically en) as
  // the placeholder so the user sees what they're translating from.
  const headlineDefault = isFeatureGraphic ? 'Your tagline.' : 'One idea\nper slide.'
  const labelPlaceholder = localeLabel ? 'FEATURE 01' : pickText(slide.label, locale) || 'FEATURE 01'
  const headlinePlaceholder = localeHeadline ? headlineDefault : pickText(slide.headline, locale) || headlineDefault

  function setLocaleField(key: 'label' | 'headline', value: string) {
    if (key === 'label') {
      onChange({ label: writeLocalized(slide.label, locale, value) })
    } else {
      onChange({ headline: writeLocalized(slide.headline, locale, value) })
    }
  }

  function changeLayout(layout: string) {
    if (!isLayout(layout)) {
      return
    }
    onChange({
      layout,
      transforms: undefined,
      screenshotSecondary:
        layout === 'two-devices' && !companion
          ? slide.screenshotSecondary || slide.screenshot
          : slide.screenshotSecondary,
    })
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b p-3">
        <div className="flex items-baseline justify-between gap-2">
          <h2 className="text-sm font-semibold">Screen settings</h2>
          <span className="text-[10px] uppercase tracking-wide text-muted-foreground">
            editing · {locale.toUpperCase()}
          </span>
        </div>
        <p className="text-xs text-muted-foreground">{layoutHint(layoutValue, device)}</p>
      </div>

      <div className="flex-1 space-y-4 overflow-y-auto p-3">
        <LayoutSelect value={layoutValue} options={layoutOptions} onChange={changeLayout} />

        {isCreative(device) && (
          <p className="rounded-md border border-green-600/30 bg-green-600/5 p-2 text-[11px] leading-relaxed text-muted-foreground">
            Keep the message and focal art inside the green{' '}
            <span className="font-medium text-foreground">art safe area</span>
            {device === 'creative-universal'
              ? "; the backdrop fills the rest. One universal asset is used for both the product page header and search results. The dashed crops are estimates. Check the real result with App Store Connect's Preview tool."
              : '; the backdrop fills the rest.'}{' '}
            Write a short brand line here; don&apos;t reuse a long screenshot headline.
          </p>
        )}

        <BackgroundControls
          slide={isFeatureGraphic ? { ...slide, inverted: slide.inverted ?? true } : slide}
          theme={theme}
          onChange={onChange}
        />

        {!isFeatureGraphic && (
          <div className="space-y-1.5">
            <Label className="text-xs">Label</Label>
            <Input
              aria-label="Label"
              value={localeLabel}
              onChange={(e) => setLocaleField('label', e.target.value)}
              placeholder={labelPlaceholder}
            />
          </div>
        )}

        <div className="space-y-1.5">
          <div className="flex items-baseline justify-between">
            <Label className="text-xs">{isFeatureGraphic ? 'Tagline' : 'Headline'}</Label>
            <div className="flex items-center gap-2">
              <span className="text-[10px] text-muted-foreground">newline = break</span>
              <CopyIdeasMenu onPick={(formula) => setLocaleField('headline', formula)} />
            </div>
          </div>
          <Textarea
            aria-label={isFeatureGraphic ? 'Tagline' : 'Headline'}
            value={localeHeadline}
            onChange={(e) => setLocaleField('headline', e.target.value)}
            rows={3}
            placeholder={headlinePlaceholder}
          />
        </div>

        <TypographySection slide={slide} isFeatureGraphic={isFeatureGraphic} onChange={onChange} />

        {!isFeatureGraphic && !isNoDevice && (
          <div className="space-y-1.5">
            <Label className="text-xs">
              {slide.layout === 'two-devices' ? 'Front device screenshot' : 'Screenshot'}
            </Label>
            <ScreenshotPicker
              label="Primary"
              value={slide.screenshot}
              locale={locale}
              onChange={(v) => onChange({ screenshot: v })}
            />
            <CaptureNote device={device} src={resolveScreenshot(slide.screenshot, locale)} />
          </div>
        )}

        {slide.layout === 'two-devices' && (
          <div className="space-y-1.5">
            <Label className="text-xs">
              {companion ? `${DEVICE_LABEL[companion]} screenshot` : 'Back device screenshot'}
            </Label>
            <ScreenshotPicker
              label={companion ? DEVICE_LABEL[companion] : 'Secondary (back layer)'}
              value={slide.screenshotSecondary || ''}
              locale={locale}
              onChange={(v) => onChange({ screenshotSecondary: v })}
            />
            {companion && <CaptureNote device={companion} src={resolveScreenshot(slide.screenshotSecondary, locale)} />}
          </div>
        )}

        <CalloutControls
          slide={slide}
          device={device}
          orientation={orientation}
          locale={locale}
          available={calloutAvailable(slide, device)}
          onChange={onChange}
          onSelectElement={onSelectElement}
        />

        {!isFeatureGraphic && (
          <ElementTransformControls
            slide={slide}
            defaultTextColor={slideColors(theme, slide).fg}
            device={device}
            orientation={orientation}
            locale={locale}
            selectedElementId={selectedElementId}
            onChange={onChange}
            onUpdate={onUpdate}
            onSelectElement={onSelectElement}
          />
        )}

        {isFeatureGraphic && (
          <div className="space-y-1.5">
            <Label className="text-xs">App icon</Label>
            <ScreenshotPicker label="Icon (shared by every banner)" value={appIcon || ''} onChange={onAppIconChange} />
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              Shows app icon + name + tagline. Leave the icon blank to use the app&apos;s initial. Name is set in the
              toolbar.
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
