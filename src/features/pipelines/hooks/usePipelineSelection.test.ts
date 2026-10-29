import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineViewPageFixture, viewPipelineFixture } from '../../../testing/fixtures'
import type { PipelineViewPage } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { usePipelineSelection } = await import('./usePipelineSelection')

const pipeline = (id: string) => viewPipelineFixture({ id })
const pageOf = (...ids: string[]) =>
  pipelineViewPageFixture({
    sections: [
      {
        id: 'section',
        kind: 'view',
        title: 'Section',
        organization: null,
        provider_ids: [],
        status: null,
        status_pipeline_id: null,
        last_run: null,
        pending_fetch: false,
        pipelines: ids.map(pipeline),
      },
    ],
  })

beforeEach(() => cleanup())

const render = (page: PipelineViewPage, view = 'all') =>
  renderHook(({ view, page }) => usePipelineSelection(view, page), { initialProps: { view, page } })

test('a pipeline filtered out of the view does not rejoin the selection when the filter clears', () => {
  const { result, rerender } = render(pageOf('a', 'b'))

  act(() => result.current.toggle(pipeline('a')))
  rerender({ view: 'all', page: pageOf('b') })
  act(() => result.current.toggle(pipeline('b')))
  rerender({ view: 'all', page: pageOf('a', 'b') })

  assert.deepEqual(
    result.current.selectedPipelines.map((p) => p.id),
    ['b']
  )
})

test('selecting visible rows keeps selected pipelines from collapsed sections', () => {
  const { result } = render(pageOf('a', 'b', 'c'))

  act(() => result.current.toggle(pipeline('a')))
  act(() => result.current.select(['b', 'c'], ['b', 'c']))
  act(() => result.current.select(['b', 'c'], ['c']))

  assert.deepEqual(
    result.current.selectedPipelines.map((p) => p.id),
    ['a', 'c']
  )
})

test('a pipeline present only in a pinned block can be selected', () => {
  const page = pipelineViewPageFixture({
    pinned_blocks: [{ kind: 'repository', ref: 'x', name: 'x', pipelines: [pipeline('x')] }],
  })
  const { result } = render(page)

  act(() => result.current.toggle(pipeline('x')))

  assert.deepEqual(
    result.current.selectedPipelines.map((p) => p.id),
    ['x']
  )
})

test('switching view clears the selection', () => {
  const page = pageOf('a')
  const { result, rerender } = render(page)

  act(() => result.current.toggle(pipeline('a')))
  rerender({ view: 'release', page })

  assert.deepEqual(result.current.selectedPipelines, [])
})

test('select mode ends when the view changes and stays off when coming back', () => {
  const page = pageOf('a')
  const { result, rerender } = render(page)

  act(() => result.current.start())
  assert.equal(result.current.selecting, true)
  rerender({ view: 'release', page })
  assert.equal(result.current.selecting, false)
  rerender({ view: 'all', page })
  assert.equal(result.current.selecting, false)
})

test('select mode survives a refreshed page of the same view', () => {
  const { result, rerender } = render(pageOf('a', 'b'))

  act(() => result.current.start())
  rerender({ view: 'all', page: pageOf('a', 'b') })
  assert.equal(result.current.selecting, true)
})

test('an unloaded page selects nothing and a page that disappears drops its selection', () => {
  const unloaded = (): PipelineViewPage | undefined => undefined
  const initialProps = { page: unloaded() }
  const { result, rerender } = renderHook(
    ({ page }: { page: PipelineViewPage | undefined }) => usePipelineSelection('all', page),
    { initialProps }
  )
  assert.equal(result.current.selectedPipelines.length, 0)

  rerender({ page: pageOf('a', 'b') })
  act(() => result.current.toggle(pipeline('a')))
  assert.deepEqual(
    result.current.selectedPipelines.map((p) => p.id),
    ['a']
  )

  rerender({ page: undefined })
  assert.equal(result.current.selectedPipelines.length, 0)

  rerender({ page: pageOf('a', 'b') })
  assert.equal(result.current.selectedPipelines.length, 0)
})
