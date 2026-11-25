const bestCF = 'https://bestcf.pages.dev'
const edgeTunnel = 'https://raw.githubusercontent.com/cmliu/cmliu/main'

export const ipLibraries = [
  { name: 'BestCF · WeTest IPv4', url: `${bestCF}/wetest/ipv4.txt`, format: 'ip' },
  { name: 'BestCF · UOUIN 三网', url: `${bestCF}/uouin/all.txt`, format: 'ip' },
  { name: 'BestCF · CFYes IPv4', url: `${bestCF}/cfyes/ipv4.txt`, format: 'ip' },
  { name: 'BestCF · CFYes IPv6', url: `${bestCF}/cfyes/ipv6.txt`, format: 'ip' },
  { name: 'edgetunnel · Cloudflare', url: `${edgeTunnel}/CF-CIDR.txt`, format: 'cidr' },
  { name: 'edgetunnel · 电信', url: `${edgeTunnel}/CF-CIDR/ct.txt`, format: 'cidr' },
  { name: 'edgetunnel · 联通', url: `${edgeTunnel}/CF-CIDR/cu.txt`, format: 'cidr' },
  { name: 'edgetunnel · 移动', url: `${edgeTunnel}/CF-CIDR/cmcc.txt`, format: 'cidr' },
] as const

type Library = (typeof ipLibraries)[number]

function withoutComment(line: string) {
  return line.split('#', 1)[0].trim()
}

function ipv4Octets(ip: string) {
  if (!/^(?:0|[1-9]\d{0,2})(?:\.(?:0|[1-9]\d{0,2})){3}$/.test(ip)) return
  const octets = ip.split('.').map(Number)
  if (octets.every((octet) => octet <= 255)) return octets
}

function ipEndpoint(line: string) {
  const entry = withoutComment(line)
  const match = /^(\d+\.\d+\.\d+\.\d+|\[[\da-f:.]+\])(?::(\d{1,5}))?$/i.exec(entry)
  const host =
    match?.[1] ?? (entry.includes(':') && /^[\da-f:.]+$/i.test(entry) ? `[${entry}]` : '')
  const port = match?.[2]
  if (!host || (port && (Number(port) < 1 || Number(port) > 65535))) return

  if (!host.startsWith('[')) {
    if (!ipv4Octets(host)) return
    return port ? `${host}:${Number(port)}` : host
  }

  try {
    const normalized = new URL(`http://${host}`).hostname
    return port ? `${normalized}:${Number(port)}` : normalized.slice(1, -1)
  } catch {
    return
  }
}

const sampleSize = 128

function sampleCidr(lines: string[]) {
  const ranges = new Map<number, number>()
  for (const line of lines) {
    const match = /^(\S+)\/(\d{1,2})$/.exec(withoutComment(line))
    if (!match) continue
    const octets = ipv4Octets(match[1])
    const prefix = Number(match[2])
    if (!octets || prefix < 1 || prefix > 32) continue
    const size = 2 ** (32 - prefix)
    const ip = octets.reduce((value, octet) => value * 256 + octet, 0)
    const base = Math.floor(ip / size) * size
    ranges.set(base, Math.max(ranges.get(base) ?? 0, size))
    if (ranges.size === sampleSize) break
  }

  return [...ranges].flatMap(([base, size], index) => {
    const share = Math.floor(sampleSize / ranges.size) + Number(index < sampleSize % ranges.size)
    const count = Math.min(size, share)
    return Array.from({ length: count }, (_, i) => {
      const ip = base + Math.floor(((i + 1) * size) / (count + 1))
      return [24, 16, 8, 0].map((shift) => (ip >>> shift) & 255).join('.')
    })
  })
}

export function parseCidrLibrary(text: string) {
  return [...new Set(sampleCidr(text.split('\n')))]
}

export function parseIpv6CidrLibrary(ipv6: string) {
  const candidates = new Set<string>()
  // shortcut: take one host per bundled /32 or /48 IPv6 network; expand if the bundled format changes.
  for (const line of ipv6.split('\n')) {
    const network = /^([\da-f:]+::)\/(?:32|48)$/i.exec(withoutComment(line))?.[1]
    const host = network && ipEndpoint(`${network}1`)
    if (host) candidates.add(host)
  }
  return [...candidates]
}

function remark(line: string) {
  const index = line.indexOf('#')
  return index === -1 ? '' : line.slice(index + 1).trim()
}

export function appendIpEntries(value: string, entries: string[]) {
  const seen = new Map<string, Set<string>>()
  function add(line: string) {
    const endpoint = ipEndpoint(line)
    if (!endpoint) return false
    const note = remark(line)
    const notes = seen.get(endpoint)
    if (notes) {
      if (!note || notes.has(note)) return false
      notes.add(note)
    } else {
      seen.set(endpoint, new Set([note]))
    }
    return true
  }
  value.split('\n').forEach(add)
  const added = entries.filter(add)
  if (!added.length) return value
  const separator = value && !value.endsWith('\n') ? '\n' : ''
  return value + separator + added.join('\n')
}

// shortcut: infer regions only for known colo codes; add observed codes as needed.
const coloRegions = new Map([
  ['HKG', '香港'],
  ['TPE', '台湾'],
  ['NRT', '日本'],
  ['KIX', '日本'],
  ['ICN', '韩国'],
  ['SIN', '新加坡'],
  ['LHR', '英国'],
  ['FRA', '德国'],
  ['CDG', '法国'],
  ['LAX', '美国'],
  ['SJC', '美国'],
  ['SFO', '美国'],
  ['SEA', '美国'],
])
export function formatColo(colo: string) {
  return `${colo}/${coloRegions.get(colo) ?? '—'}`
}

export async function fetchIpLibrary(library: Library) {
  const response = await fetch(library.url, {
    credentials: 'omit',
    redirect: 'error',
    referrerPolicy: 'no-referrer',
    signal: AbortSignal.timeout(10_000),
  })
  if (!response.ok || !response.body) throw new Error('IP library unavailable')

  const reader = response.body.getReader()
  const decoder = new TextDecoder('utf-8', { fatal: true })
  let text = ''
  let bytes = 0
  try {
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
      bytes += chunk.value.byteLength
      if (bytes > 128 * 1024) {
        await reader.cancel()
        throw new Error('IP library too large')
      }
      text += decoder.decode(chunk.value, { stream: true })
    }
    return text + decoder.decode()
  } finally {
    reader.releaseLock()
  }
}
