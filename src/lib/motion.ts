export const PRESENT_EASING = 'cubic-bezier(0.32, 0.72, 0, 1)'
export const MODAL_TRANSITION = {
  transition: {
    in: { opacity: 1, transform: 'scale(1)' },
    out: { opacity: 0, transform: 'scale(0.97)' },
    common: { transformOrigin: 'center' },
    transitionProperty: 'opacity, transform',
  },
  duration: 160,
  timingFunction: PRESENT_EASING,
} as const

export const OVERLAY_TRANSITION = { transition: 'fade', duration: 120, timingFunction: PRESENT_EASING } as const

const REDUCED_MOTION = '(prefers-reduced-motion: reduce)'

export const prefersReducedMotion = (): boolean =>
  typeof window !== 'undefined' && window.matchMedia(REDUCED_MOTION).matches

export const scrollBehavior = (): ScrollBehavior => (prefersReducedMotion() ? 'auto' : 'smooth')
