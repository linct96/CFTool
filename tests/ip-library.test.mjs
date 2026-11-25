import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import {
  appendIpEntries,
  fetchIpLibrary,
  ipLibraries,
  parseCidrLibrary,
  parseIpv6CidrLibrary,
} from '../src/ip-library.ts'

test('CIDR imports are bounded, repeatable, and cover multiple ranges', () => {
  const text = '203.0.113.19/24\r\n198.51.100.0/24\r\n203.0.113.0/24\r\n1.2.3.4/0\r\ninvalid/24'
  const entries = parseCidrLibrary(text)
  assert.equal(entries.length, 128)
  assert.equal(entries.filter((entry) => entry.startsWith('203.0.113.')).length, 64)
  assert.equal(entries.filter((entry) => entry.startsWith('198.51.100.')).length, 64)
  assert.deepEqual(parseCidrLibrary(text), entries)
  assert.deepEqual(parseCidrLibrary('203.0.113.17/32'), ['203.0.113.17'])
})

test('bundled IPv4 and IPv6 import separately without replacing or duplicating existing IPs', () => {
  const ipv4 = readFileSync(new URL('../src/assets/ip.txt', import.meta.url), 'utf8')
  const ipv6 = readFileSync(new URL('../src/assets/ipv6.txt', import.meta.url), 'utf8')
  const v4 = parseCidrLibrary(ipv4)
  const v6 = parseIpv6CidrLibrary(ipv6)
  assert.equal(v4.length, 128)
  assert.equal(v6.length, ipv6.trim().split('\n').length)
  assert.ok(v6.includes('2606:4700::1'))
  const existing = '203.0.113.1#manual'
  const updated = appendIpEntries(appendIpEntries(existing, v4), v6)
  assert.equal(updated, [existing, ...v4, ...v6].join('\n'))
  assert.equal(appendIpEntries(updated, v6), updated)
})

test('merging keeps existing edits and distinct remarks for the same endpoint', () => {
  const existing = '203.0.113.1:443#manual\n[2001:0DB8::1]:443\n\n'
  const entries = [
    '203.0.113.1:443',
    '[2001:db8::1]:443',
    '203.0.113.1:80',
    '203.0.113.1:443#移动 | HKG#note',
    '203.0.113.1:443#联通 | HKG',
  ]
  const updated = appendIpEntries(existing, [...entries, entries[3]])
  assert.equal(updated, `${existing}${entries.slice(2).join('\n')}`)
  assert.equal(appendIpEntries(updated, entries), updated)
  assert.equal(appendIpEntries('', entries), entries.join('\n'))
})

test('an IP library response preserves UTF-8 remarks across chunk boundaries', async ({ mock }) => {
  const body = '1.1.1.1:443#移动 | HKG\n[2606:4700::1111]:443#联通'
  const encoded = new TextEncoder().encode(body)
  const response = new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(encoded.slice(0, 13))
        controller.enqueue(encoded.slice(13))
        controller.close()
      },
    }),
  )
  mock.method(globalThis, 'fetch', () => Promise.resolve(response))
  assert.equal(await fetchIpLibrary(ipLibraries[0]), body)
})

test('Tauri only allows the maintained library URLs for HTTPS connections', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url)))
  const remotes = config.app.security.csp['connect-src']
    .split(' ')
    .filter((url) => url.startsWith('https:'))
  assert.deepEqual(remotes.sort(), ipLibraries.map(({ url }) => url).sort())
})
