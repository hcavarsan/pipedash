import type { ApplicationListQuery } from '../../../types/applications'

export const DEFAULT_APPLICATION_QUERY: ApplicationListQuery = {
  search: null,
  health: [],
  behind_only: false,
  provider_ids: [],
  with_suggestions: false,
  include_hidden: false,
  sort: 'name',
  direction: 'asc',
}
