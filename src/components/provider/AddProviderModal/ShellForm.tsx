import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { IosGroup } from '../../ios/IosList'

import classes from './ShellForm.module.css'

interface StepHeadingProps {
  title: string
  subtitle?: string
}

export function StepHeading({ title, subtitle }: StepHeadingProps) {
  return (
    <div className={classes.heading}>
      <h2 className={classes.headingTitle}>{title}</h2>
      {subtitle && <p className={classes.headingSubtitle}>{subtitle}</p>}
    </div>
  )
}

interface ShellFieldProps {
  label?: string
  required?: boolean
  description?: string | null
  error?: string | null
  children: ReactNode
}

export function ShellField({ label, required, description, error, children }: ShellFieldProps) {
  return (
    <IosGroup
      header={label && required ? `${label} *` : label}
      footer={
        error || description ? (
          <>
            {error && <div className={classes.error}>{error}</div>}
            {description && <div>{description}</div>}
          </>
        ) : undefined
      }
    >
      {label ? <div className={classes.control}>{children}</div> : children}
    </IosGroup>
  )
}

export function CheckCircle({ checked }: { checked: boolean }) {
  return (
    <span className={classes.check} data-checked={checked ? '' : undefined} aria-hidden>
      {checked && <IconCheck size={14} stroke={3} />}
    </span>
  )
}
