import { Check, Copy } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatColo } from '@/ip-library'

export type ProbeRow = {
  endpoint: string
  sent: number
  received: number
  loss: number
  latency: number | null
  latency_min: number | null
  latency_max: number | null
  speed: number | null
  colo: string | null
  notes: string[]
}

export type ProbeGroup = {
  node: number | null
  cfst: ProbeRow[]
  xray: ProbeRow[]
}

export type CandidateItem = {
  endpoint: string
  notes: string[]
}

export type CandidateGroup = {
  node: number | null
  default_port: number
  items: CandidateItem[]
}

export type ProbeStage =
  | 'checkingNetwork'
  | 'preparingTask'
  | 'cfst'
  | 'preparingXray'
  | 'xray'
  | 'savingResults'
  | 'completed'
  | 'paused'
  | 'failed'

export type ProbeEvent =
  | { kind: 'stage'; stage: ProbeStage; progress: [number, number] | null }
  | { kind: 'subscription'; line: number }
  | { kind: 'routeWarning'; message: string }
  | { kind: 'candidates'; history_id: string; groups: CandidateGroup[] }
  | { kind: 'cfstBatchStart'; default_port: number; endpoints: string[] }
  | { kind: 'cfstBatchDone'; default_port: number; endpoints: string[]; rows: ProbeRow[] }
  | { kind: 'xrayTargets'; targets: { node: number; endpoints: string[] }[] }
  | { kind: 'xrayUpdate'; node: number; row: ProbeRow }

export type LiveProbeState = {
  groups: CandidateGroup[]
  scanning: Partial<Record<string, true>>
  completed: Partial<Record<string, true>>
  cfstRows: Partial<Record<string, ProbeRow>>
  xrayTargets: Partial<Record<string, true>>
  xrayRows: Partial<Record<string, ProbeRow>>
  expectDownload: boolean
}

export type RetestKind = 'cfst' | 'xray'

type CfstPhase = 'done' | 'scanning' | 'pending' | 'filtered' | 'none'
type XrayPhase = 'done' | 'testing' | 'idle'

function endpointParts(endpoint: string) {
  const split = endpoint.lastIndexOf(':')
  const rawHost = endpoint.slice(0, split)
  const ip = rawHost.startsWith('[') && rawHost.endsWith(']') ? rawHost.slice(1, -1) : rawHost
  const port = Number(endpoint.slice(split + 1))
  return { ip, port }
}

function compareCfstRows(a: ProbeRow, b: ProbeRow) {
  const speedDiff = (b.speed ?? -1) - (a.speed ?? -1)
  if (speedDiff !== 0) return speedDiff
  const latencyDiff =
    (a.latency ?? Number.POSITIVE_INFINITY) - (b.latency ?? Number.POSITIVE_INFINITY)
  if (latencyDiff !== 0) return latencyDiff
  const aParts = endpointParts(a.endpoint)
  const bParts = endpointParts(b.endpoint)
  if (aParts.ip !== bParts.ip) return aParts.ip < bParts.ip ? -1 : 1
  return aParts.port - bParts.port
}

function compareXrayRows(a: ProbeRow, b: ProbeRow) {
  const receivedDiff = b.received - a.received
  if (receivedDiff !== 0) return receivedDiff
  const latencyDiff =
    (a.latency ?? Number.POSITIVE_INFINITY) - (b.latency ?? Number.POSITIVE_INFINITY)
  if (latencyDiff !== 0) return latencyDiff
  return a.endpoint < b.endpoint ? -1 : a.endpoint > b.endpoint ? 1 : 0
}

export function applyProbeEvent(
  current: LiveProbeState | null,
  event: ProbeEvent,
  expectDownload: boolean,
): LiveProbeState | null {
  switch (event.kind) {
    case 'candidates':
      return {
        groups: event.groups,
        scanning: {},
        completed: {},
        cfstRows: {},
        xrayTargets: {},
        xrayRows: {},
        expectDownload,
      }
    case 'cfstBatchStart': {
      if (!current) return null
      const scanning = { ...current.scanning }
      for (const endpoint of event.endpoints) scanning[`${event.default_port}:${endpoint}`] = true
      return { ...current, scanning }
    }
    case 'cfstBatchDone': {
      if (!current) return null
      const scanning = { ...current.scanning }
      const completed = { ...current.completed }
      const cfstRows = { ...current.cfstRows }
      for (const endpoint of event.endpoints) {
        const key = `${event.default_port}:${endpoint}`
        delete scanning[key]
        completed[key] = true
      }
      for (const row of event.rows) cfstRows[`${event.default_port}:${row.endpoint}`] = row
      return { ...current, scanning, completed, cfstRows }
    }
    case 'xrayTargets': {
      if (!current) return null
      const nextTargets = { ...current.xrayTargets }
      for (const { node, endpoints } of event.targets) {
        for (const endpoint of endpoints) {
          nextTargets[`${node}:${endpoint}`] = true
        }
      }
      return {
        ...current,
        xrayTargets: nextTargets,
      }
    }
    case 'xrayUpdate': {
      if (!current) return null
      return {
        ...current,
        xrayRows: {
          ...current.xrayRows,
          [`${event.node}:${event.row.endpoint}`]: event.row,
        },
      }
    }
    case 'stage':
    case 'subscription':
    case 'routeWarning':
      return current
  }
}

export function snapshotLiveGroups(live: LiveProbeState): ProbeGroup[] {
  const groups = live.groups.map(({ node, default_port, items }) => {
    const cfst: ProbeRow[] = []
    const xray: ProbeRow[] = []
    for (const { endpoint } of items) {
      const scan = live.cfstRows[`${default_port}:${endpoint}`]
      if (scan) cfst.push(scan)
      if (node !== null) {
        const retest = live.xrayRows[`${node}:${endpoint}`]
        if (retest && retest.sent >= 3) xray.push(retest)
      }
    }
    cfst.sort(compareCfstRows)
    xray.sort(compareXrayRows)
    return { node, cfst, xray }
  })
  return groups.some(({ cfst, xray }) => cfst.length > 0 || xray.length > 0) ? groups : []
}

type RetestActions = {
  disabled: boolean
  run: (
    kind: RetestKind,
    endpoint: string,
    node: number | null,
  ) => Promise<{ row: ProbeRow | null } | null>
}

const cell = 'probe-result-cell px-2.5 py-2'

function formatLatency(latency: number | null | undefined) {
  return latency == null ? '—' : `${latency.toFixed(1)} ms`
}

function XrayWhisker({
  row,
  minBound,
  maxBound,
}: {
  row: ProbeRow
  minBound: number
  maxBound: number
}) {
  if (row.latency == null) return <span className="text-muted-foreground">—</span>
  const hasRange = row.latency_min != null && row.latency_max != null
  const rangeSpan = Math.max(1, maxBound - minBound)
  const low = row.latency_min ?? row.latency
  const high = row.latency_max ?? row.latency
  const leftPct = Math.min(88, Math.max(2, ((low - minBound) / rangeSpan) * 100))
  const rightPct = Math.min(98, Math.max(leftPct + 6, ((high - minBound) / rangeSpan) * 100))
  const medPct = Math.min(
    rightPct - 1,
    Math.max(leftPct + 1, ((row.latency - minBound) / rangeSpan) * 100),
  )

  return (
    <div className="probe-xray-whisker flex items-center gap-2 font-mono">
      {hasRange && (
        <div
          className="probe-whisker-track relative h-1.5 w-14 shrink-0 rounded-full border border-border bg-muted"
          aria-hidden="true"
        >
          <div
            className="probe-whisker-span absolute inset-y-0 rounded-full bg-foreground/30"
            style={{ left: `${leftPct}%`, width: `${rightPct - leftPct}%` }}
          />
          <div
            className="probe-whisker-median absolute -inset-y-0.5 w-0.5 rounded-full bg-foreground"
            style={{ left: `${medPct}%` }}
          />
        </div>
      )}
      <span className="probe-xray-median font-medium text-foreground">
        {row.latency.toFixed(1)} ms
      </span>
      {hasRange && (
        <span className="probe-xray-range text-[0.6875rem] text-muted-foreground">
          {Math.round(low)}–{Math.round(high)}
        </span>
      )}
    </div>
  )
}

export function ProbeResults({
  groups,
  live,
  actions,
  emptyTitle = '没有符合条件的结果',
}: {
  groups?: ProbeGroup[]
  live?: LiveProbeState
  actions?: RetestActions
  emptyTitle?: string
}) {
  const [temporary, setTemporary] = useState<Partial<Record<string, ProbeRow | null>>>({})
  const [copiedKey, setCopiedKey] = useState<string | null>(null)
  const copyTimer = useRef<number | undefined>(undefined)

  useEffect(
    () => () => {
      if (copyTimer.current !== undefined) window.clearTimeout(copyTimer.current)
    },
    [],
  )

  async function runRetest(kind: RetestKind, endpoint: string, node: number | null) {
    if (!actions || actions.disabled) return
    const response = await actions.run(kind, endpoint, node)
    if (response)
      setTemporary((rows) => ({ ...rows, [`${node}:${endpoint}:${kind}`]: response.row }))
  }

  function copyEndpoint(key: string, endpoint: string) {
    void navigator.clipboard.writeText(endpoint).then(() => {
      setCopiedKey(key)
      if (copyTimer.current !== undefined) window.clearTimeout(copyTimer.current)
      copyTimer.current = window.setTimeout(() => setCopiedKey(null), 1500)
    })
  }

  const showNode = live
    ? live.groups.some(({ node }) => node !== null)
    : (groups ?? []).some(({ node }) => node !== null)

  type ResolvedRow = {
    key: string
    node: number | null
    scan: ProbeRow | undefined
    cfst: ProbeRow | null | undefined
    cfstPhase: CfstPhase
    xray: ProbeRow | null | undefined
    xrayPhase: XrayPhase
    notes: string[]
    endpoint: string
    isTemporary: boolean
  }

  const resolvedRows: ResolvedRow[] = live
    ? live.groups.flatMap(({ node, default_port, items }) => {
        const entries = items.map(({ endpoint, notes }, index) => {
          const key = `${default_port}:${endpoint}`
          const cfst = live.cfstRows[key]
          const cfstPhase: CfstPhase = cfst
            ? 'done'
            : live.completed[key]
              ? 'filtered'
              : live.scanning[key]
                ? 'scanning'
                : 'pending'
          const xray = node !== null ? live.xrayRows[`${node}:${endpoint}`] : undefined
          const xrayTargeted = node !== null && Boolean(live.xrayTargets[`${node}:${endpoint}`])
          const xrayPhase: XrayPhase =
            xray && xray.sent >= 3 ? 'done' : xray || xrayTargeted ? 'testing' : 'idle'
          const tier =
            cfstPhase === 'done'
              ? 0
              : cfstPhase === 'scanning'
                ? 1
                : cfstPhase === 'pending'
                  ? 2
                  : 3
          return {
            index,
            tier,
            row: {
              key: `${node}:${endpoint}`,
              node,
              scan: cfst,
              cfst,
              cfstPhase,
              xray,
              xrayPhase,
              notes,
              endpoint,
              isTemporary: false,
            },
          }
        })
        entries.sort((a, b) => {
          if (a.tier !== b.tier) return a.tier - b.tier
          if (a.tier === 0 && a.row.cfst && b.row.cfst) {
            return compareCfstRows(a.row.cfst, b.row.cfst)
          }
          return a.index - b.index
        })
        return entries.map(({ row }) => row)
      })
    : (groups ?? []).flatMap(({ node, cfst, xray }) => {
        const remaining = new Map(cfst.map((row) => [row.endpoint, row]))
        const tested = xray.map((retest) => {
          const scan = remaining.get(retest.endpoint)
          remaining.delete(retest.endpoint)
          return { node, scan, retest, endpoint: retest.endpoint }
        })
        const merged = [
          ...tested,
          ...Array.from(remaining.values(), (scan) => ({
            node,
            scan,
            retest: undefined,
            endpoint: scan.endpoint,
          })),
        ]
        return merged.map(({ node: rowNode, scan, retest: originalRetest, endpoint }) => {
          const key = `${rowNode}:${endpoint}`
          const newScan = temporary[`${key}:cfst`]
          const newRetest = temporary[`${key}:xray`]
          const resolvedCfst = newScan === undefined ? scan : newScan
          const resolvedXray = newRetest === undefined ? originalRetest : newRetest
          const notes = (scan ?? originalRetest)?.notes ?? []
          const isTemporary = newScan !== undefined || newRetest !== undefined
          return {
            key,
            node: rowNode,
            scan,
            cfst: resolvedCfst,
            cfstPhase: resolvedCfst ? 'done' : 'none',
            xray: resolvedXray,
            xrayPhase: resolvedXray ? 'done' : 'idle',
            notes,
            endpoint,
            isTemporary,
          }
        })
      })

  if (!resolvedRows.length) {
    return (
      <Empty className="probe-results-empty min-h-0">
        <EmptyHeader>
          <EmptyTitle>{emptyTitle}</EmptyTitle>
        </EmptyHeader>
      </Empty>
    )
  }

  const showSpeed =
    Boolean(live?.expectDownload) || resolvedRows.some(({ cfst }) => cfst?.speed != null)
  const showNotes = resolvedRows.some(({ notes }) => notes.length > 0)

  let bestCfstKey: string | null = null
  let bestCfstLatency = Number.POSITIVE_INFINITY
  let bestCfstColo: string | null = null
  let bestXrayLatency = Number.POSITIVE_INFINITY
  let cfstCount = 0
  let checkedCount = 0
  let zeroLossCount = 0
  let xrayMinBound = Number.POSITIVE_INFINITY
  let xrayMaxBound = Number.NEGATIVE_INFINITY

  for (const row of resolvedRows) {
    if (row.cfstPhase === 'done' || row.cfstPhase === 'filtered') checkedCount += 1
    if (row.cfst) {
      cfstCount += 1
      if (row.cfst.loss === 0) zeroLossCount += 1
      if (row.cfst.latency != null && row.cfst.latency < bestCfstLatency) {
        bestCfstLatency = row.cfst.latency
        bestCfstKey = row.key
        bestCfstColo = row.cfst.colo
      }
    }
    if (row.xray?.latency != null) {
      if (row.xray.latency < bestXrayLatency) bestXrayLatency = row.xray.latency
      const low = row.xray.latency_min ?? row.xray.latency
      const high = row.xray.latency_max ?? row.xray.latency
      if (low < xrayMinBound) xrayMinBound = low
      if (high > xrayMaxBound) xrayMaxBound = high
    }
  }

  if (!Number.isFinite(xrayMinBound)) xrayMinBound = 0
  if (!Number.isFinite(xrayMaxBound)) xrayMaxBound = 100

  return (
    <div className="probe-results-view flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="probe-summary-strip flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-1 border-b border-border bg-sidebar px-4 py-2 text-xs">
        <div className="probe-summary-metrics flex flex-wrap items-center gap-x-4 gap-y-1 font-mono tabular-nums">
          {live && (
            <span className="probe-metric inline-flex items-center gap-1.5 text-muted-foreground">
              已测
              <strong className="font-semibold text-foreground">
                {checkedCount} / {resolvedRows.length}
              </strong>
            </span>
          )}
          {Number.isFinite(bestCfstLatency) && (
            <span className="probe-metric inline-flex items-center gap-1.5 text-muted-foreground">
              最快 CFST
              <strong className="font-semibold text-status-ok">
                {bestCfstColo ? `${formatColo(bestCfstColo)} ` : ''}
                {bestCfstLatency.toFixed(1)} ms
              </strong>
            </span>
          )}
          {showNode && Number.isFinite(bestXrayLatency) && (
            <span className="probe-metric inline-flex items-center gap-1.5 text-muted-foreground">
              最快 Xray
              <strong className="font-semibold text-foreground">
                {bestXrayLatency.toFixed(1)} ms
              </strong>
            </span>
          )}
          {cfstCount > 0 && (
            <span className="probe-metric inline-flex items-center gap-1.5 text-muted-foreground">
              零丢包
              <strong className="font-semibold text-foreground">
                {zeroLossCount} / {cfstCount}
              </strong>
            </span>
          )}
        </div>
        <span className="probe-summary-count font-mono text-muted-foreground tabular-nums">
          {live && checkedCount > 0 && cfstCount < checkedCount
            ? `通过 ${cfstCount} / 共 ${resolvedRows.length} 条`
            : `共 ${resolvedRows.length} 条`}
        </span>
      </div>

      <Table
        className="probe-results-table border-separate border-spacing-0 tabular-nums"
        containerProps={{
          className: 'probe-results-scroll min-h-0 min-w-0 flex-1 overflow-auto',
          tabIndex: 0,
          role: 'region',
          'aria-label': '测速结果表格',
        }}
      >
        <TableCaption className="probe-results-caption sr-only">测速结果</TableCaption>
        <TableHeader className="probe-results-header sticky top-0 z-10 bg-sidebar [&_tr]:border-b-0">
          <TableRow className="probe-results-columns hover:bg-transparent">
            <TableHead
              scope="col"
              className={`${cell} h-8 border-b border-border text-muted-foreground`}
            >
              IP / 端口
            </TableHead>
            <TableHead
              scope="col"
              className={`${cell} h-8 border-b border-border text-muted-foreground`}
            >
              机房/国家
            </TableHead>
            {showNotes && (
              <TableHead
                scope="col"
                className={`${cell} h-8 border-b border-border text-muted-foreground`}
              >
                备注
              </TableHead>
            )}
            {showNode && (
              <TableHead
                scope="col"
                className={`${cell} h-8 border-b border-border text-muted-foreground`}
              >
                节点
              </TableHead>
            )}
            <TableHead
              scope="col"
              className={`${cell} h-8 border-b border-border text-muted-foreground`}
            >
              CFST 延迟
            </TableHead>
            <TableHead
              scope="col"
              className={`${cell} h-8 border-b border-border text-muted-foreground`}
            >
              丢包
            </TableHead>
            <TableHead
              scope="col"
              className={`${cell} h-8 border-b border-border text-muted-foreground`}
            >
              CFST 成功
            </TableHead>
            {showSpeed && (
              <TableHead
                scope="col"
                className={`${cell} h-8 border-b border-border text-muted-foreground`}
              >
                下载速度
              </TableHead>
            )}
            {showNode && (
              <>
                <TableHead
                  scope="col"
                  className={`${cell} h-8 border-b border-border text-muted-foreground`}
                >
                  Xray 成功
                </TableHead>
                <TableHead
                  scope="col"
                  className={`${cell} h-8 border-b border-border text-muted-foreground`}
                >
                  Xray 204（低–中–高）
                </TableHead>
              </>
            )}
            {actions && (
              <TableHead
                scope="col"
                className={`${cell} h-8 border-b border-border text-right text-muted-foreground`}
              >
                重测
              </TableHead>
            )}
          </TableRow>
        </TableHeader>
        <TableBody className="probe-results-rows">
          {resolvedRows.map(
            ({
              key,
              node,
              scan,
              cfst,
              cfstPhase,
              xray,
              xrayPhase,
              notes,
              endpoint,
              isTemporary,
            }) => {
              const isBest = key === bestCfstKey
              const copied = copiedKey === key
              const lossWarn = cfst != null && cfst.loss > 0
              const filtered = cfstPhase === 'filtered'
              return (
                <TableRow
                  key={key}
                  className={`probe-result-row ${
                    isTemporary
                      ? 'bg-status-temp-surface hover:bg-status-temp-surface'
                      : filtered
                        ? 'probe-row-filtered opacity-55'
                        : ''
                  }`}
                >
                  <TableHead
                    scope="row"
                    className={`${cell} border-b border-border font-mono font-normal`}
                  >
                    <div className="probe-endpoint-cell flex items-center gap-1.5">
                      <button
                        type="button"
                        className="probe-copy-endpoint group/copy -mx-1 inline-flex items-center gap-1.5 rounded-sm px-1 py-0.5 text-left font-medium text-foreground transition-colors outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/30"
                        title={`复制 ${endpoint}`}
                        aria-label={`复制 ${endpoint}`}
                        onClick={() => copyEndpoint(key, endpoint)}
                      >
                        <span>{endpoint}</span>
                        {copied ? (
                          <Check className="size-3 text-status-ok" aria-hidden="true" />
                        ) : (
                          <Copy
                            className="size-3 text-muted-foreground opacity-0 transition-opacity group-hover/copy:opacity-100 group-focus-visible/copy:opacity-100"
                            aria-hidden="true"
                          />
                        )}
                      </button>
                      {isTemporary && (
                        <span className="probe-temp-badge rounded-sm border border-border bg-background px-1 py-0.25 text-[0.625rem] font-medium text-status-temp">
                          临时
                        </span>
                      )}
                    </div>
                  </TableHead>
                  <TableCell className={`${cell} probe-cfst-colo border-b border-border font-mono`}>
                    {cfst?.colo ? formatColo(cfst.colo) : '—'}
                  </TableCell>
                  {showNotes && (
                    <TableCell
                      className={`${cell} probe-notes-cell border-b border-border text-muted-foreground`}
                    >
                      {notes.length === 0 ? (
                        '—'
                      ) : (
                        <div className="probe-notes-stack flex max-w-56 flex-col gap-0.5">
                          {notes.map((note) => (
                            <span
                              key={note}
                              className="probe-note wrap-anywhere whitespace-pre-wrap"
                            >
                              {note}
                            </span>
                          ))}
                        </div>
                      )}
                    </TableCell>
                  )}
                  {showNode && (
                    <TableCell
                      className={`${cell} border-b border-border font-mono text-muted-foreground`}
                    >
                      {node == null ? '—' : `#${node}`}
                    </TableCell>
                  )}
                  <TableCell
                    className={`${cell} border-b border-border font-mono ${
                      isBest ? 'font-semibold text-status-ok' : lossWarn ? 'text-status-warn' : ''
                    }`}
                  >
                    {cfstPhase === 'scanning' ? (
                      <span className="probe-cell-status inline-flex items-center gap-1.5 font-sans text-muted-foreground">
                        <span
                          className="probe-cell-dot size-1.5 animate-pulse rounded-full bg-dot-status-working"
                          aria-hidden="true"
                        />
                        测速中
                      </span>
                    ) : cfstPhase === 'pending' ? (
                      <span className="probe-cell-status font-sans text-muted-foreground">
                        待测
                      </span>
                    ) : cfstPhase === 'filtered' ? (
                      <span className="probe-cell-status font-sans text-muted-foreground">
                        未通过
                      </span>
                    ) : (
                      formatLatency(cfst?.latency)
                    )}
                  </TableCell>
                  <TableCell
                    className={`${cell} border-b border-border font-mono ${
                      cfst == null
                        ? 'text-muted-foreground'
                        : lossWarn
                          ? 'font-medium text-status-warn'
                          : 'text-status-ok'
                    }`}
                  >
                    {cfst ? `${(cfst.loss * 100).toFixed(0)}%` : '—'}
                  </TableCell>
                  <TableCell
                    className={`${cell} border-b border-border font-mono text-muted-foreground`}
                  >
                    {cfst ? `${cfst.received} / ${cfst.sent}` : '—'}
                  </TableCell>
                  {showSpeed && (
                    <TableCell className={`${cell} border-b border-border font-mono`}>
                      {cfst?.speed == null ? '—' : `${cfst.speed.toFixed(2)} MB/s`}
                    </TableCell>
                  )}
                  {showNode && (
                    <>
                      <TableCell
                        className={`${cell} border-b border-border font-mono text-muted-foreground`}
                      >
                        {xrayPhase === 'testing' ? (
                          <span className="probe-cell-status inline-flex items-center gap-1.5 text-foreground">
                            <span
                              className="probe-cell-dot size-1.5 animate-pulse rounded-full bg-dot-status-working"
                              aria-hidden="true"
                            />
                            {xray ? (
                              `${xray.received} / ${xray.sent}`
                            ) : (
                              <span className="font-sans text-muted-foreground">复测中</span>
                            )}
                          </span>
                        ) : xray ? (
                          `${xray.received} / ${xray.sent}`
                        ) : (
                          '—'
                        )}
                      </TableCell>
                      <TableCell className={`${cell} border-b border-border`}>
                        {xray ? (
                          <XrayWhisker row={xray} minBound={xrayMinBound} maxBound={xrayMaxBound} />
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </TableCell>
                    </>
                  )}
                  {actions && (
                    <TableCell className={`${cell} border-b border-border text-right`}>
                      <div className="probe-retest-actions inline-flex items-center justify-end gap-1">
                        {scan && (
                          <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            disabled={actions.disabled}
                            aria-label={`重测 IP ${endpoint}`}
                            onClick={() => void runRetest('cfst', endpoint, node)}
                          >
                            重测 IP
                          </Button>
                        )}
                        {node !== null && !live && (
                          <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            disabled={actions.disabled}
                            aria-label={`${xray ? '重测' : '测'} Xray 节点 ${node}，${endpoint}`}
                            onClick={() => void runRetest('xray', endpoint, node)}
                          >
                            {xray ? '重测 Xray' : '测 Xray'}
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  )}
                </TableRow>
              )
            },
          )}
        </TableBody>
      </Table>
    </div>
  )
}
