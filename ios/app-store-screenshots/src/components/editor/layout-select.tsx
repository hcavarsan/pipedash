import type { ReactNode } from 'react'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { SlideLayout } from '@/lib/types'

export function LayoutSelect({
  value,
  options,
  onChange,
}: {
  value: SlideLayout
  options: readonly (readonly [SlideLayout, string])[]
  onChange: (layout: string) => void
}): ReactNode {
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">Layout</Label>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger aria-label="Layout">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map(([layout, label]) => (
            <SelectItem key={layout} value={layout}>
              {label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
