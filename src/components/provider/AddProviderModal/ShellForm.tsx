import { IconCheck } from '@tabler/icons-react'
import type { ReactNode } from 'react'

import { errorId, helpId } from '../../../utils/formFields'
import { IosGroup } from '../../ios/IosList'

import classes from './ShellForm.module.css'

interface StepHeadingProps {
  title: string
  subtitle?: string
}

export function StepHeading({ title, subtitle }: StepHeadingProps): ReactNode {
  return (
    <div className={classes.heading}>
      <h2 className={classes.headingTitle} tabIndex={-1} data-step-heading>
        {title}
      </h2>
      {subtitle && <p className={classes.headingSubtitle}>{subtitle}</p>}
    </div>
  )
}

interface ShellFieldProps {
  id?: string | undefined
  label?: string | undefined
  required?: boolean | undefined
  description?: string | null | undefined
  error?: string | null | undefined
  children: ReactNode
}

export function ShellField({ id, label, required, description, error, children }: ShellFieldProps): ReactNode {
  return (
    <IosGroup
      header={
        label && required ? (
          <>
            {label}
            <span aria-hidden> *</span>
          </>
        ) : (
          label
        )
      }
      footer={
        error || description ? (
          <>
            {error && (
              <div id={id ? errorId(id) : undefined} className={classes.error}>
                {error}
              </div>
            )}
            {description && <div id={id ? helpId(id) : undefined}>{description}</div>}
          </>
        ) : undefined
      }
    >
      {label ? <div className={classes.control}>{children}</div> : children}
    </IosGroup>
  )
}

export function CheckCircle({ checked }: { checked: boolean }): ReactNode {
  return (
    <span className={classes.check} data-checked={checked ? '' : undefined} aria-hidden>
      {checked && <IconCheck size={14} stroke={3} />}
    </span>
  )
}
