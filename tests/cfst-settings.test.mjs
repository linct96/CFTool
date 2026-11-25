import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  cfstSettingsSchema,
  defaultCfstSettings,
  toProbeSettings,
} from '../src/cfst-settings-schema.ts'

function invalidFields(patch) {
  const result = cfstSettingsSchema.safeParse({ ...defaultCfstSettings, ...patch })
  return result.success ? [] : result.error.issues.map((issue) => issue.path[0])
}

test('validates active CFST URLs without requiring a custom URL', () => {
  for (const url of [
    'ftp://example.com',
    'example.com',
    'https://',
    'https:///path',
    'https://user:pass@example.com',
    'https://example.com/a b',
  ]) {
    assert.deepEqual(invalidFields({ mode: 'http', url }), ['url'], url)
    assert.deepEqual(invalidFields({ download: true, url }), ['url'], url)
    assert.deepEqual(invalidFields({ url }), [], url)
  }
  for (const url of ['', '  ', 'http://example.com', ' https://[2001:db8::1]:8443/a%20b?x=1 ']) {
    assert.deepEqual(invalidFields({ mode: 'http', url }), [], url)
  }
})

test('reports numeric drafts, integer/range rules and related fields by name', () => {
  assert.deepEqual(invalidFields({ n: NaN, t: 1.5, tlr: 1.01, tll: 10000, tl: null }), [
    'n',
    't',
    'tlr',
    'tl',
  ])
  assert.deepEqual(invalidFields({ mode: 'http', cfcolo: 'HKG, NRT', 'httping-code': 99 }), [
    'httping-code',
    'cfcolo',
  ])
  assert.deepEqual(invalidFields({ 'httping-code': NaN, dn: 0, tlr: 0.025 }), [])
})

test('serializes enabled settings in the string format expected by Rust', () => {
  assert.deepEqual(toProbeSettings(defaultCfstSettings), {
    mode: 'tcp',
    download: 'no',
    n: '200',
    t: '4',
    tll: '0',
    tl: '9999',
    tlr: '0',
  })
  assert.deepEqual(
    toProbeSettings({
      ...defaultCfstSettings,
      mode: 'http',
      download: true,
      url: ' https://example.com/test ',
      cfcolo: ' HKG,NRT ',
      'httping-code': 204,
      tlr: 0.025,
      sl: 1.25,
    }),
    {
      mode: 'http',
      download: 'yes',
      n: '200',
      t: '4',
      'httping-code': '204',
      cfcolo: 'HKG,NRT',
      url: 'https://example.com/test',
      tll: '0',
      tl: '9999',
      tlr: '0.025',
      dn: '10',
      dt: '10',
      sl: '1.25',
    },
  )
  assert.equal(
    Object.hasOwn(toProbeSettings({ ...defaultCfstSettings, mode: 'http', url: ' ' }), 'url'),
    false,
  )
})
