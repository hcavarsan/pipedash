import assert from 'node:assert/strict'
import { test } from 'node:test'

import { buildVaultExportCommand } from './vaultExportCommand'

test('a plain password is single-quoted', () => {
  assert.equal(
    buildVaultExportCommand('PIPEDASH_VAULT_PASSWORD', 'hunter2hunter2'),
    "export PIPEDASH_VAULT_PASSWORD='hunter2hunter2'"
  )
})

test('spaces stay inside the quotes', () => {
  assert.equal(buildVaultExportCommand('VAULT', 'a b  c'), "export VAULT='a b  c'")
})

test('single quotes are escaped so the shell reads them literally', () => {
  assert.equal(buildVaultExportCommand('VAULT', "it's"), "export VAULT='it'\\''s'")
  assert.equal(buildVaultExportCommand('VAULT', "'"), "export VAULT=''\\'''")
})

test('double quotes, dollar signs, backticks and backslashes are not expanded', () => {
  assert.equal(buildVaultExportCommand('VAULT', '"$HOME`id`\\n'), "export VAULT='\"$HOME`id`\\n'")
})

test('newlines are kept inside the quotes', () => {
  assert.equal(buildVaultExportCommand('VAULT', 'a\nb'), "export VAULT='a\nb'")
})

test('valid names are accepted', () => {
  assert.notEqual(buildVaultExportCommand('_x1', 'pw'), null)
  assert.notEqual(buildVaultExportCommand('a', 'pw'), null)
})

test('invalid names produce no command', () => {
  for (const name of ['', '1VAR', 'MY-VAR', 'A B', 'A;rm -rf ~', 'A=B', 'A\nB', '$(id)']) {
    assert.equal(buildVaultExportCommand(name, 'pw'), null, name)
  }
})
