import type { ApplicationListQuery } from '../../../types/applications'

export const ALL_APPLICATIONS_QUERY: ApplicationListQuery = {
  search: null,
  health: [],
  behind_only: false,
  provider_ids: [],
  with_suggestions: false,
  include_hidden: true,
  sort: 'name',
  direction: 'asc',
}
