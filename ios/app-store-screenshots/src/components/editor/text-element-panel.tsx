import { AlignCenter, AlignLeft, AlignRight } from 'lucide-react'
import type { KeyboardEvent, ReactNode } from 'react'
import { useRef, useState } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { pickText } from '@/lib/locale'
import type { Patch, TextElement } from '@/lib/types'
import { defaultTextElementFontSize, textElementFontSizeRange } from '@/lib/typography'
import { LayerButton } from './layer-button'
import { ResetButton } from './reset-button'

type Canvas = { cW: number; cH: number }

const ALIGNS = [
  { value: 'left', Icon: AlignLeft, label: 'Align left' },
  { value: 'center', Icon: AlignCenter, label: 'Align center' },
  { value: 'right', Icon: AlignRight, label: 'Align right' },
] as const

export function TextElementPanel({
  element,
  locale,
  canvas,
  defaultColor,
  onTextChange,
  onTextPatch,
}: {
  element: TextElement
  locale: string
  canvas: Canvas
  defaultColor: string
  onTextChange: (value: string) => void
  onTextPatch: (patch: Patch<TextElement>) => void
}): ReactNode {
  // Like the headline field: the locale's own text, with the fallback shown as
  // a placeholder, so clearing a translation doesn't snap back to English.
  const text = element.text?.[locale] ?? ''
  const textPlaceholder = pickText(element.text, locale) || 'Overlay text'
  const align = element.align ?? 'center'

  return (
    <div className="space-y-2 rounded border bg-muted/30 p-2">
      <div className="space-y-1">
        <Label className="text-[11px] text-muted-foreground">Text</Label>
        <Textarea
          value={text}
          rows={2}
          onChange={(event) => onTextChange(event.target.value)}
          placeholder={textPlaceholder}
          aria-label="Overlay text"
        />
      </div>
      <TextSizeControl element={element} canvas={canvas} onTextPatch={onTextPatch} />
      <div className="grid grid-cols-[76px_1fr] gap-2">
        <div className="space-y-1">
          <Label className="text-[11px] text-muted-foreground">Color</Label>
          <Input
            type="color"
            value={element.color || defaultColor}
            className="h-7 cursor-pointer p-0.5"
            onChange={(event) => onTextPatch({ color: event.target.value })}
            aria-label="Text color"
          />
        </div>
        <div className="space-y-1">
          <Label className="text-[11px] text-muted-foreground">Align</Label>
          <AlignButtons align={align} onTextPatch={onTextPatch} />
        </div>
      </div>
    </div>
  )
}

function AlignButtons({
  align,
  onTextPatch,
}: {
  align: NonNullable<TextElement['align']>
  onTextPatch: (patch: Patch<TextElement>) => void
}): ReactNode {
  return (
    <div className="grid grid-cols-3 gap-1">
      {ALIGNS.map(({ value, Icon, label }) => (
        <LayerButton
          key={value}
          disabled={false}
          active={align === value}
          onClick={() => onTextPatch({ align: value })}
          label={label}
        >
          <Icon className="h-3.5 w-3.5" />
        </LayerButton>
      ))}
    </div>
  )
}

function TextSizeControl({
  element,
  canvas,
  onTextPatch,
}: {
  element: TextElement
  canvas: Canvas
  onTextPatch: (patch: Patch<TextElement>) => void
}): ReactNode {
  const defaultSize = defaultTextElementFontSize(canvas.cW, canvas.cH)
  const range = textElementFontSizeRange(canvas.cW, canvas.cH)
  const customSize =
    typeof element.fontSize === 'number' && Number.isFinite(element.fontSize) ? element.fontSize : undefined
  // Mirror the canvas: an unset size renders at the canvas-relative default.
  const size = Math.round(customSize ?? defaultSize)
  // Keep a value loaded from an older project reachable even if it sits
  // outside the canvas-relative range.
  const sliderMin = Math.min(range.min, size)
  const sliderMax = Math.max(range.max, size)
  // Typing is buffered so intermediate values ("1" on the way to "120") don't
  // get clamped mid-keystroke; the value is committed on blur / Enter.
  const [draft, setDraft] = useState<string | null>(null)
  const cancelDraft = useRef(false)

  function commitDraft() {
    if (cancelDraft.current) {
      cancelDraft.current = false
      return
    }
    if (draft === null) {
      return
    }
    const n = Number(draft)
    if (draft.trim() !== '' && Number.isFinite(n)) {
      onTextPatch({ fontSize: Math.min(range.max, Math.max(range.min, Math.round(n))) })
    }
    setDraft(null)
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'Enter') {
      event.currentTarget.blur()
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      cancelDraft.current = true
      setDraft(null)
      event.currentTarget.blur()
    }
  }

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-[11px] text-muted-foreground">Size</Label>
        <ResetButton
          visible={customSize !== undefined && size !== defaultSize}
          label={`Reset size to ${defaultSize}px`}
          onClick={() => onTextPatch({ fontSize: undefined })}
        />
      </div>
      <div className="flex items-center gap-2">
        <input
          type="range"
          min={sliderMin}
          max={sliderMax}
          step={1}
          value={size}
          onChange={(event) => onTextPatch({ fontSize: Number(event.target.value) })}
          onDoubleClick={() => onTextPatch({ fontSize: undefined })}
          className="min-w-0 flex-1"
          aria-label="Text size"
        />
        <div className="relative w-[76px] shrink-0">
          <Input
            type="number"
            min={range.min}
            max={range.max}
            value={draft ?? String(size)}
            onChange={(event) => setDraft(event.target.value)}
            onBlur={commitDraft}
            onKeyDown={handleKeyDown}
            className="h-8 pr-7 text-xs tabular-nums [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
            aria-label="Text size in pixels"
          />
          <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[11px] text-muted-foreground">
            px
          </span>
        </div>
      </div>
    </div>
  )
}
