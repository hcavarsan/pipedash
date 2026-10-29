import { prefersReducedMotion } from './motion'

export interface SpringConfig {
  damping: number
  response: number
}

export const SPRING_DEFAULT: SpringConfig = { damping: 1, response: 0.35 }
export const SPRING_MOMENTUM: SpringConfig = { damping: 0.8, response: 0.3 }

const REST_DELTA = 0.5
const REST_SPEED = 10
const DECELERATION_RATE = 0.998
const RUBBERBAND_CONSTANT = 0.55

export interface SpringState {
  displacement: number
  velocity: number
}

export function springState(
  displacement: number,
  velocity: number,
  seconds: number,
  config: SpringConfig
): SpringState {
  const omega = (2 * Math.PI) / config.response
  const zeta = config.damping

  if (zeta < 1) {
    const decay = zeta * omega
    const damped = omega * Math.sqrt(1 - zeta * zeta)
    const b = (velocity + decay * displacement) / damped
    const envelope = Math.exp(-decay * seconds)
    const cos = Math.cos(damped * seconds)
    const sin = Math.sin(damped * seconds)
    const x = envelope * (displacement * cos + b * sin)

    return { displacement: x, velocity: -decay * x + envelope * (b * damped * cos - displacement * damped * sin) }
  }

  if (zeta === 1) {
    const b = velocity + omega * displacement
    const envelope = Math.exp(-omega * seconds)

    return {
      displacement: (displacement + b * seconds) * envelope,
      velocity: (velocity - omega * b * seconds) * envelope,
    }
  }

  const root = Math.sqrt(zeta * zeta - 1)
  const r1 = -omega * (zeta - root)
  const r2 = -omega * (zeta + root)
  const c1 = (velocity - r2 * displacement) / (r1 - r2)
  const c2 = displacement - c1
  const e1 = Math.exp(r1 * seconds)
  const e2 = Math.exp(r2 * seconds)

  return { displacement: c1 * e1 + c2 * e2, velocity: c1 * r1 * e1 + c2 * r2 * e2 }
}

export function project(position: number, velocity: number, rate = DECELERATION_RATE): number {
  return position + ((velocity / 1000) * rate) / (1 - rate)
}

export function rubberband(overshoot: number, dimension: number, constant = RUBBERBAND_CONSTANT): number {
  if (dimension <= 0) {
    return 0
  }

  return (overshoot * dimension * constant) / (dimension + constant * Math.abs(overshoot))
}

export interface SpringOptions {
  from: number
  to: number
  velocity?: number
  config?: SpringConfig
  onUpdate: (value: number) => void
}

export interface SpringAnimation {
  readonly value: number
  readonly velocity: number
  readonly finished: Promise<boolean>
  stop: () => void
}

export function animateSpring({
  from,
  to,
  velocity = 0,
  config = SPRING_DEFAULT,
  onUpdate,
}: SpringOptions): SpringAnimation {
  const start = performance.now()
  const displacement = from - to
  let frame = 0
  let done = false
  let rest = { value: to, velocity: 0 }
  const { promise: finished, resolve } = Promise.withResolvers<boolean>()

  const stateAt = (now: number) => springState(displacement, velocity, Math.max(0, now - start) / 1000, config)

  const finish = (settled: boolean) => {
    if (done) {
      return
    }

    if (!settled) {
      rest = { value: to + stateAt(performance.now()).displacement, velocity: 0 }
    }

    done = true
    cancelAnimationFrame(frame)
    resolve(settled)
  }

  const tick = (now: number) => {
    const state = stateAt(now)

    if (Math.abs(state.displacement) < REST_DELTA && Math.abs(state.velocity) < REST_SPEED) {
      onUpdate(to)
      finish(true)

      return
    }

    onUpdate(to + state.displacement)
    frame = requestAnimationFrame(tick)
  }

  if (prefersReducedMotion() || (Math.abs(displacement) < REST_DELTA && Math.abs(velocity) < REST_SPEED)) {
    onUpdate(to)
    finish(true)
  } else {
    frame = requestAnimationFrame(tick)
  }

  return {
    get value() {
      return done ? rest.value : to + stateAt(performance.now()).displacement
    },
    get velocity() {
      return done ? rest.velocity : stateAt(performance.now()).velocity
    },
    finished,
    stop: () => finish(false),
  }
}
