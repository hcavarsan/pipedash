import { useRef } from 'react'
import { useBlurValidation } from '../../../hooks/useBlurValidation'
import type { ParamValues } from '../lib/triggerParams'
import { useCheckTrigger } from '../queries/useWorkflowQueries'

interface TriggerValidation {
  errors: Partial<Record<string, string>>
  checkAll: (inputs: ParamValues, names: string[]) => Promise<boolean>
  blur: (name: string, inputs: ParamValues) => void
  change: (name: string, inputs: ParamValues) => void
}

export function useTriggerValidation(pipelineId: string, rerunOf: number | undefined): TriggerValidation {
  const check = useCheckTrigger()
  const checked = useRef<Record<string, string>>({})
  const latest = useRef(0)
  const waiting = useRef(new Map<string, (key: string, value: string) => void>())
  const validation = useBlurValidation<string>((key) => checked.current[key])

  const run = async (inputs: ParamValues): Promise<Record<string, string>> => {
    const draft = await check.mutateAsync({ pipelineId, rerunOf: rerunOf ?? null, inputs })

    return draft.errors
  }

  const checkAll = async (inputs: ParamValues, names: string[]): Promise<boolean> => {
    latest.current += 1
    waiting.current.clear()
    checked.current = await run(inputs)

    return validation.validateAll(Object.fromEntries([...names, ...Object.keys(checked.current)].map((n) => [n, ''])))
  }

  const recheck = (inputs: ParamValues): void => {
    latest.current += 1
    const ticket = latest.current
    run(inputs)
      .then((errors) => {
        if (ticket !== latest.current) {
          return
        }

        const pending = [...waiting.current]

        checked.current = errors
        waiting.current.clear()

        for (const [name, apply] of pending) {
          apply(name, '')
        }
      })
      .catch(() => {
        if (ticket === latest.current) {
          waiting.current.clear()
        }
      })
  }

  return {
    errors: validation.errors,
    checkAll,
    blur: (name, inputs) => {
      waiting.current.set(name, validation.onBlur)
      recheck(inputs)
    },
    change: (name, inputs) => {
      if (validation.errors[name] !== undefined) {
        waiting.current.set(name, validation.onChange)
      }

      if (waiting.current.size > 0) {
        recheck(inputs)
      }
    },
  }
}
