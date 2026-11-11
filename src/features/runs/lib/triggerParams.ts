export type ParamValues = Record<string, unknown>

export const comparable = (value: unknown): string => (value === undefined || value === null ? '' : String(value))
