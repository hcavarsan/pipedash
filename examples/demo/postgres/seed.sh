#!/bin/sh
set -eu

providers=$(psql -tAc 'SELECT COUNT(*) FROM providers' 2> /dev/null || echo 0)
until [ "$providers" -gt 0 ] &&
  [ "$(psql -tAc 'SELECT COUNT(DISTINCT provider_id) FROM pipelines_cache')" -ge "$providers" ]; do
  sleep 5
  providers=$(psql -tAc 'SELECT COUNT(*) FROM providers' 2> /dev/null || echo 0)
done

psql -v ON_ERROR_STOP=1 -q -f /seed/views.sql
