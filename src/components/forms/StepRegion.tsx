import { VisuallyHidden } from '@mantine/core'
import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from 'react'

import { LiveStatus } from './LiveStatus'

interface StepRegionProps {
  stepKey: string | number
  position: string
  title: string
  hiddenHeading?: boolean
  className?: string
  style?: CSSProperties
  children: ReactNode
}

export function StepRegion({
  stepKey,
  position,
  title,
  hiddenHeading,
  className,
  style,
  children,
}: StepRegionProps): ReactNode {
  const regionRef = useRef<HTMLDivElement>(null)
  const previous = useRef(stepKey)
  const [announcement, setAnnouncement] = useState('')

  useEffect(() => {
    if (previous.current === stepKey) {
      return
    }

    previous.current = stepKey
    regionRef.current?.querySelector<HTMLElement>('[data-step-heading]')?.focus()
    setAnnouncement(`${position}: ${title}`)
  }, [stepKey, position, title])

  return (
    <div ref={regionRef} className={className} style={style}>
      {hiddenHeading && (
        <VisuallyHidden>
          <h2 tabIndex={-1} data-step-heading>
            {title}
          </h2>
        </VisuallyHidden>
      )}
      {children}
      <LiveStatus message={announcement} />
    </div>
  )
}
