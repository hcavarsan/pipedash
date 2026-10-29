import { IconChevronRight } from '@tabler/icons-react'
import { type MouseEventHandler, type ReactNode, useId } from 'react'
import { Link, useLocation } from 'react-router-dom'

import classes from './IosList.module.css'
import { type SwipeAction, SwipeActions } from './SwipeActions'
import { useRowMenuLongPress } from './useLongPress'

interface IosGroupProps {
  header?: ReactNode
  headerAction?: ReactNode
  headingLevel?: 2 | 3
  footer?: ReactNode
  children: ReactNode
}

export function IosGroup({ header, headerAction, headingLevel = 2, footer, children }: IosGroupProps) {
  const id = useId()
  const headingId = `${id}-heading`
  const footerId = `${id}-footer`
  const Heading = headingLevel === 3 ? 'h3' : 'h2'
  return (
    <section
      className={classes.section}
      aria-labelledby={header ? headingId : undefined}
      aria-describedby={footer ? footerId : undefined}
    >
      {(header || headerAction) && (
        <div className={classes.headerRow}>
          {header && (
            <Heading id={headingId} className={classes.header}>
              {header}
            </Heading>
          )}
          {headerAction}
        </div>
      )}
      <div className={classes.group}>{children}</div>
      {footer && (
        <div id={footerId} className={classes.footer}>
          {footer}
        </div>
      )}
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
  onClick?: MouseEventHandler<HTMLButtonElement>
  chevron?: boolean
  destructive?: boolean
  disabled?: boolean
  selected?: boolean
  longPressMenu?: boolean
  ariaLabel?: string
  invalid?: boolean
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
  selected,
  longPressMenu,
  ariaLabel,
  invalid,
  swipeActions,
}: IosRowProps) {
  const location = useLocation()
  const pressRef = useRowMenuLongPress(Boolean(longPressMenu))
  const tappable = Boolean(to || onClick)
  const shared = {
    className: classes.row,
    'data-icon': icon ? '' : undefined,
    'data-tappable': tappable ? '' : undefined,
    'data-destructive': destructive ? '' : undefined,
    'data-disabled': disabled ? '' : undefined,
    'data-selected': selected ? '' : undefined,
    'aria-label': ariaLabel,
    'aria-invalid': invalid || undefined,
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

  const render = (className: string | undefined, ref?: (element: HTMLElement | null) => void) =>
    to ? (
      <Link
        to={to}
        state={{ from: `${location.pathname}${location.search}` }}
        {...shared}
        aria-current={selected ? 'true' : undefined}
        className={className}
        ref={ref}
      >
        {content}
      </Link>
    ) : onClick ? (
      <button
        type="button"
        onClick={onClick}
        disabled={disabled}
        {...shared}
        aria-pressed={selected}
        className={className}
        ref={ref}
      >
        {content}
      </button>
    ) : (
      <div {...shared} className={className} ref={ref}>
        {content}
      </div>
    )

  if (swipeActions?.length) {
    return (
      <div
        ref={pressRef}
        className={`${classes.swipeRow} ${classes.item}`}
        data-icon={icon ? '' : undefined}
        data-selected={selected ? '' : undefined}
      >
        <SwipeActions actions={swipeActions}>{render(classes.row)}</SwipeActions>
      </div>
    )
  }

  return render(`${classes.row} ${classes.item}`, pressRef)
}
