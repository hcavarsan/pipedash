import { IconChevronRight } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import classes from './IosList.module.css'
import { type SwipeAction, SwipeActions } from './SwipeActions'

interface IosGroupProps {
  header?: ReactNode
  footer?: ReactNode
  children: ReactNode
}

export function IosGroup({ header, footer, children }: IosGroupProps) {
  return (
    <section className={classes.section}>
      {header && <div className={classes.header}>{header}</div>}
      <div className={classes.group}>{children}</div>
      {footer && <div className={classes.footer}>{footer}</div>}
    </section>
  )
}

interface IosRowProps {
  title: ReactNode
  subtitle?: ReactNode
  icon?: ReactNode
  value?: ReactNode
  accessory?: ReactNode
  to?: string
  onClick?: () => void
  chevron?: boolean
  destructive?: boolean
  disabled?: boolean
  ariaLabel?: string
  swipeActions?: SwipeAction[]
}

export function IosRow({
  title,
  subtitle,
  icon,
  value,
  accessory,
  to,
  onClick,
  chevron = Boolean(to),
  destructive,
  disabled,
  ariaLabel,
  swipeActions,
}: IosRowProps) {
  const tappable = Boolean(to || onClick)
  const shared = {
    className: classes.row,
    'data-icon': icon ? '' : undefined,
    'data-tappable': tappable ? '' : undefined,
    'data-destructive': destructive ? '' : undefined,
    'data-disabled': disabled ? '' : undefined,
    'aria-label': ariaLabel,
  }
  const content = (
    <>
      {icon && (
        <span className={classes.icon} aria-hidden>
          {icon}
        </span>
      )}
      <span className={classes.text}>
        <span className={classes.title}>{title}</span>
        {subtitle && <span className={classes.subtitle}>{subtitle}</span>}
      </span>
      {value && <span className={classes.value}>{value}</span>}
      {accessory && <span className={classes.accessory}>{accessory}</span>}
      {chevron && (
        <span className={classes.chevron} aria-hidden>
          <IconChevronRight size={18} />
        </span>
      )}
    </>
  )

  const render = (className: string) =>
    to ? (
      <Link to={to} {...shared} className={className}>
        {content}
      </Link>
    ) : onClick ? (
      <button type="button" onClick={onClick} disabled={disabled} {...shared} className={className}>
        {content}
      </button>
    ) : (
      <div {...shared} className={className}>
        {content}
      </div>
    )

  if (swipeActions?.length) {
    return (
      <div className={`${classes.swipeRow} ${classes.item}`} data-icon={icon ? '' : undefined}>
        <SwipeActions actions={swipeActions}>{render(classes.row)}</SwipeActions>
      </div>
    )
  }

  return render(`${classes.row} ${classes.item}`)
}
