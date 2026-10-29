import {
  animateSpring,
  SPRING_DEFAULT,
  SPRING_MOMENTUM,
  type SpringAnimation,
  type SpringConfig,
} from '../../lib/spring'

export const FLICK_SPEED = 500

export const springFor = (velocity: number): SpringConfig =>
  Math.abs(velocity) > FLICK_SPEED ? SPRING_MOMENTUM : SPRING_DEFAULT

export interface SpringValue {
  readonly value: number
  readonly active: boolean
  set: (value: number) => void
  grab: () => number
  to: (target: number, velocity?: number, config?: SpringConfig) => Promise<boolean>
}

export function createSpringValue(initial: number, apply: (value: number) => void): SpringValue {
  let current = initial
  let animation: SpringAnimation | null = null
  let session = 0

  const grab = (): number => {
    session += 1

    if (animation) {
      current = animation.value
      animation.stop()
      animation = null
    }

    return current
  }

  return {
    get value() {
      return animation ? animation.value : current
    },
    get active() {
      return animation !== null
    },
    set: (value) => {
      grab()
      current = value
      apply(value)
    },
    grab,
    to: (target, velocity, config = SPRING_DEFAULT) => {
      const inherited = animation?.velocity ?? 0
      const from = grab()
      const mine = session
      const run = animateSpring({
        from,
        to: target,
        velocity: velocity ?? inherited,
        config,
        onUpdate: (value) => {
          current = value
          apply(value)
        },
      })

      animation = run

      return run.finished.then((finished) => {
        const live = finished && session === mine

        if (live) {
          animation = null
        }

        return live
      })
    },
  }
}
