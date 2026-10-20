export const formatCount = (count: number, singular: string, plural = `${singular}s`): string =>
  `${count.toLocaleString()} ${count === 1 ? singular : plural}`
