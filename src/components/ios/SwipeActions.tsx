import type { ReactNode } from 'react'
import { SwipeActionButtons } from './SwipeActionButtons'
import classes from './SwipeActions.module.css'
import type { SwipeAction } from './swipeAction'
import { useSwipeActions } from './useSwipeActions'

interface SwipeActionsProps {
  actions: SwipeAction[]
  children: ReactNode
}

export function SwipeActions({ actions, children }: SwipeActionsProps): ReactNode {
  const { slideRef, primaryRef, isOpen, close } = useSwipeActions(actions)

  return (
    <>
      <div
        ref={slideRef}
        className={classes.slide}
        onClickCapture={(event) => {
          if (isOpen()) {
            event.preventDefault()
            event.stopPropagation()
            close()
          }
        }}
      >
        {children}
      </div>
      <SwipeActionButtons actions={actions} primaryRef={primaryRef} onBeforeSelect={close} />
    </>
  )
}
