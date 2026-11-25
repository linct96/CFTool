import ipv4 from '@/assets/ip.txt?raw'
import ipv6 from '@/assets/ipv6.txt?raw'
import { parseCidrLibrary, parseIpv6CidrLibrary } from '@/ip-library'

export const bundledIpLibraries = [
  { name: 'Cloudflare IPv4', entries: parseCidrLibrary(ipv4) },
  { name: 'Cloudflare IPv6', entries: parseIpv6CidrLibrary(ipv6) },
]
