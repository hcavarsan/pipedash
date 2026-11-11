import type { ReactNode, RefObject } from 'react'

import classes from './SwipeActions.module.css'
import { ACTION_WIDTH, type SwipeAction } from './swipeAction'

interface SwipeActionButtonsProps {
  actions: SwipeAction[]
  underlayRef: RefObject<HTMLDivElement | null>
  primaryRef: RefObject<HTMLButtonElement | null>
  onBeforeSelect: () => void
}

export function SwipeActionButtons({
  actions,
  underlayRef,
  primaryRef,
  onBeforeSelect,
}: SwipeActionButtonsProps): ReactNode {
  const primary = actions.find((action) => action.primary)
  const rest = actions.filter((action) => !action.primary)

  return (
    <div ref={underlayRef} className={classes.underlay} data-tone={(rest[0] ?? primary)?.tone}>
      <div className={classes.rest} style={{ right: primary ? ACTION_WIDTH : 0 }}>
        {rest.map((action) => (
          <button
            key={action.key}
            type="button"
            className={classes.action}
            data-tone={action.tone}
            disabled={action.disabled}
            onClick={() => {
              onBeforeSelect()
              action.onSelect()
            }}
          >
            {action.label}
          </button>
        ))}
      </div>
      {primary && (
        <button
          ref={primaryRef}
          type="button"
          className={classes.primary}
          data-tone={primary.tone}
          disabled={primary.disabled}
          onClick={() => {
            onBeforeSelect()
            primary.onSelect()
          }}
        >
          <span className={classes.label}>{primary.label}</span>
        </button>
      )}
    </div>
  )
}
