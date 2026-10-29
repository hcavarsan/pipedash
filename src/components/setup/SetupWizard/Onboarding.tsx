import type { ReactNode } from 'react'

import { AppLogo } from '../../atoms/AppLogo'

import classes from './Onboarding.module.css'

interface OnboardingProps {
  title?: string | undefined
  footer?: ReactNode
  children: ReactNode
}

export function Onboarding({ title, footer, children }: OnboardingProps) {
  return (
    <div className={classes.screen}>
      <div className={classes.scroll}>
        {title && (
          <div className={classes.hero}>
            <AppLogo size={72} alt="Pipedash" />
            <h1 className={classes.title}>{title}</h1>
          </div>
        )}
        {children}
      </div>
      {footer && <div className={classes.footer}>{footer}</div>}
    </div>
  )
}

interface StepHeaderProps {
  step: number
  labels: string[]
}

export function StepHeader({ step, labels }: StepHeaderProps) {
  return (
    <div className={classes.stepHeader}>
      <div className={classes.dots} role="img" aria-label={`Step ${step + 1} of ${labels.length}`}>
        {labels.map((label, index) => (
          <span
            key={label}
            className={classes.dot}
            data-active={index === step || undefined}
            data-done={index < step || undefined}
          />
        ))}
      </div>
      <div className={classes.stepCount}>
        Step {step + 1} of {labels.length}
      </div>
      <h2 className={classes.stepTitle} tabIndex={-1} data-step-heading>
        {labels[step]}
      </h2>
    </div>
  )
}
