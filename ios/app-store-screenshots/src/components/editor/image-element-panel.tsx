import type { ReactNode } from 'react'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { ImageElement, Patch } from '@/lib/types'
import { ScreenshotPicker } from './screenshot-picker'

type FitValue = NonNullable<ImageElement['fit']>
type FadeEdge = NonNullable<ImageElement['fade']>['edge']

const FITS: readonly FitValue[] = ['cover', 'contain']
const EDGES: readonly FadeEdge[] = ['top', 'bottom', 'left', 'right']

const isFit = (value: string): value is FitValue => FITS.some((fit) => fit === value)
const isEdge = (value: string): value is FadeEdge => EDGES.some((edge) => edge === value)

export function ImageElementPanel({
  element,
  onPatch,
  onSourceChange,
}: {
  element: ImageElement
  onPatch: (patch: Patch<ImageElement>) => void
  onSourceChange: (src: string) => void
}): ReactNode {
  const fade = element.fade

  function changeFit(value: string) {
    if (isFit(value)) {
      onPatch({ fit: value })
    }
  }

  function changeEdge(value: string) {
    if (value === 'none') {
      onPatch({ fade: undefined })
    } else if (isEdge(value)) {
      onPatch({ fade: { edge: value, amount: fade?.amount ?? 35 } })
    }
  }

  return (
    <div className="space-y-2">
      <ScreenshotPicker label="Image" value={element.src} onChange={onSourceChange} />
      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-1">
          <Label className="text-[11px] text-muted-foreground">Fit</Label>
          <Select value={element.fit || 'cover'} onValueChange={changeFit}>
            <SelectTrigger className="h-8 text-xs" aria-label="Image fit">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="cover">Fill frame</SelectItem>
              <SelectItem value="contain">Whole image</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-[11px] text-muted-foreground">Edge fade</Label>
          <Select value={fade?.edge || 'none'} onValueChange={changeEdge}>
            <SelectTrigger className="h-8 text-xs" aria-label="Edge fade">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="none">None</SelectItem>
              <SelectItem value="top">From top</SelectItem>
              <SelectItem value="bottom">From bottom</SelectItem>
              <SelectItem value="left">From left</SelectItem>
              <SelectItem value="right">From right</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      {fade && (
        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <Label className="text-[11px] text-muted-foreground">Fade strength</Label>
            <span className="text-[11px] tabular-nums text-muted-foreground">{Math.round(fade.amount)}%</span>
          </div>
          <input
            type="range"
            min={1}
            max={100}
            value={fade.amount}
            onChange={(event) => onPatch({ fade: { ...fade, amount: Number(event.target.value) } })}
            className="w-full"
            aria-label="Fade strength"
          />
        </div>
      )}
    </div>
  )
}
