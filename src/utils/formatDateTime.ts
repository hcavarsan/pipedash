const DATE_TIME_FORMAT = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })
const TIME_FORMAT = new Intl.DateTimeFormat(undefined, { timeStyle: 'short' })

export const formatDateTime = (value: string | number | Date): string => DATE_TIME_FORMAT.format(new Date(value))

export const formatTime = (value: string | number | Date): string => TIME_FORMAT.format(new Date(value))
