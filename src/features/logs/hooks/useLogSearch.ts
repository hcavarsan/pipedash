import { useMemo, useState } from 'react'
import { type RunLogSearchIdentity, useRunLogSearch } from '../../../queries/useRunLogQueries'
import { getErrorCode } from '../../../types/errors'
import type { LogSearchResult } from '../../../types/logs'
import { errorMessage } from '../../../utils/errorDisplay'
import type { MatchTriple } from '../components/LogRow'
import { newSessionId } from '../lib/logActions'

const SEARCH_ERROR = 'Invalid search pattern'
const NO_MATCHES: LogSearchResult['matches'] = []

interface LogSearchOptions {
  pipelineId: string
  runNumber: number
  nodeId: string
  revision: number
}

export interface LogSearchState {
  queryText: string
  regex: boolean
  caseSensitive: boolean
  setQueryText: (value: string) => void
  setRegex: (value: boolean) => void
  setCaseSensitive: (value: boolean) => void
  result: LogSearchResult | undefined
  matches: LogSearchResult['matches']
  matchIndex: ReadonlyMap<number, MatchTriple[]>
  matchLines: readonly number[]
  failure: string | null
  settled: boolean
}

const sameIdentity = (a: RunLogSearchIdentity, b: RunLogSearchIdentity): boolean =>
  a.text === b.text && a.regex === b.regex && a.case_sensitive === b.case_sensitive

export function useLogSearch({ pipelineId, runNumber, nodeId, revision }: LogSearchOptions): LogSearchState {
  const [sessionId] = useState(newSessionId)
  const [queryText, setQueryText] = useState('')
  const [regex, setRegex] = useState(false)
  const [caseSensitive, setCaseSensitive] = useState(false)
  const search = useRunLogSearch(pipelineId, runNumber, nodeId, revision, sessionId, {
    text: queryText,
    regex,
    case_sensitive: caseSensitive,
  })
  const result = queryText ? search.data : undefined

  const matchIndex = useMemo(() => {
    const index = new Map<number, MatchTriple[]>()

    for (const match of result?.matches ?? []) {
      const list = index.get(match[0])

      if (list) {
        list.push(match)
      } else {
        index.set(match[0], [match])
      }
    }

    return index
  }, [result])
  const matchLines = useMemo(() => [...matchIndex.keys()], [matchIndex])
  const failure = search.error
    ? getErrorCode(search.error) === 'BAD_REQUEST'
      ? SEARCH_ERROR
      : errorMessage(search.error, "Couldn't search the log")
    : null

  return {
    queryText,
    regex,
    caseSensitive,
    setQueryText,
    setRegex,
    setCaseSensitive,
    result,
    matches: result?.matches ?? NO_MATCHES,
    matchIndex,
    matchLines,
    failure,
    settled: !search.isPlaceholderData && sameIdentity(search.requestedIdentity, search.effectiveIdentity),
  }
}
