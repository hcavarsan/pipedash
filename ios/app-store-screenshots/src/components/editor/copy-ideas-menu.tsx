import { Lightbulb } from 'lucide-react'
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { COPY_IDEA_SLOTS, type CopyIdeaSlot } from '@/lib/copy-ideas'

function SlotItems({
  slot,
  first,
  onPick,
}: {
  slot: CopyIdeaSlot
  first: boolean
  onPick: (formula: string) => void
}): ReactNode {
  return (
    <>
      {!first && <DropdownMenuSeparator />}
      <DropdownMenuLabel className="text-[10px] uppercase tracking-wide text-muted-foreground">
        {slot.name}
      </DropdownMenuLabel>
      {slot.ideas.map((idea) => (
        <DropdownMenuItem
          key={idea.formula}
          onSelect={() => onPick(idea.formula)}
          className="flex-col items-start gap-0.5"
        >
          <span className="text-xs font-medium">{idea.formula.replace(/\n/g, ' / ')}</span>
          <span className="text-[11px] text-muted-foreground">{idea.example}</span>
        </DropdownMenuItem>
      ))}
    </>
  )
}

export function CopyIdeasMenu({ onPick }: { onPick: (formula: string) => void }): ReactNode {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-6 gap-1 px-1.5 text-[11px] text-muted-foreground"
          title="Insert a headline formula, then replace the [bracketed] words"
        >
          <Lightbulb className="h-3 w-3" />
          Copy ideas
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-[420px] w-72 overflow-y-auto">
        {COPY_IDEA_SLOTS.map((slot, i) => (
          <SlotItems key={slot.id} slot={slot} first={i === 0} onPick={onPick} />
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
