import type { ShownNotification } from '../../types/notifications'

export const failedNotification: ShownNotification = {
  id: 1,
  at: '2026-10-26T12:01:00Z',
  tone: 'danger',
  key: 'pipeline:github__1__owner/repo/build.yml',
  title: 'Build failed',
  body: 'owner/repo · github',
  kind: 'failed',
  pipeline_id: 'github__1__owner/repo/build.yml',
  provider_id: 1,
  provider_ids: [1],
  fired: null,
}

export const outageNotification: ShownNotification = {
  id: 2,
  at: '2026-10-26T12:02:00Z',
  tone: 'warning',
  key: 'status-page:github',
  title: 'GitHub Actions outage',
  body: 'Workflow runs are delayed.',
  kind: 'outage',
  pipeline_id: null,
  provider_id: null,
  provider_ids: [1],
  level: 'major_outage',
  link: { url: 'https://www.githubstatus.com/', kind: 'status_page' },
  fired: null,
}
