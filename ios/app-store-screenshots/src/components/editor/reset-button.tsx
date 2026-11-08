import { RotateCcw } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

/** Small inline reset affordance. Always occupies its slot so the row doesn't
 * shift when it appears. */
export function ResetButton({
  visible,
  label,
  onClick,
}: {
  visible: boolean
  label: string
  onClick: () => void
}): ReactNode {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className={cn('h-5 w-5 text-muted-foreground [&_svg]:size-3', !visible && 'invisible')}
      onClick={onClick}
      title={label}
      aria-label={label}
      tabIndex={visible ? 0 : -1}
      aria-hidden={!visible}
    >
      <RotateCcw />
    </Button>
  )
}
