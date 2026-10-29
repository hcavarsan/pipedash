import type { ReactNode } from 'react'

import classes from './ContentUnavailable.module.css'

interface ContentUnavailableProps {
  icon?: ReactNode
  title?: string
  description?: ReactNode
  hint?: ReactNode
  actions?: ReactNode
  role?: 'alert' | 'status'
  tone?: 'default' | 'error'
  busy?: boolean
  fill?: boolean
}

export function ContentUnavailable({
  icon,
  title,
  description,
  hint,
  actions,
  role,
  tone = 'default',
  busy,
  fill,
}: ContentUnavailableProps): ReactNode {
  return (
    <div className={classes.root} role={role} aria-busy={busy} data-tone={tone} data-fill={fill ? '' : undefined}>
      {icon && (
        <span className={classes.icon} aria-hidden>
          {icon}
        </span>
      )}
      {title && <h2 className={classes.title}>{title}</h2>}
      {description && <p className={classes.description}>{description}</p>}
      {hint && <p className={classes.description}>{hint}</p>}
      {actions && <div className={classes.actions}>{actions}</div>}
    </div>
  )
}
