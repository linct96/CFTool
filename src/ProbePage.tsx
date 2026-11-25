import { useStore } from '@tanstack/react-form'
import { Channel, invoke } from '@tauri-apps/api/core'
import { ListFilter, Play, Server, SlidersHorizontal, Square } from 'lucide-react'
import { useRef, useState } from 'react'
import { ProbeHistory, canResumeHistory, historyDate, historyStatusLabels } from '@/ProbeHistory'
import type { HistoryDetail, HistoryReplay } from '@/ProbeHistory'
import { ProbeResults, applyProbeEvent, snapshotLiveGroups } from '@/ProbeResults'
import type {
  LiveProbeState,
  ProbeEvent,
  ProbeGroup,
  ProbeRow,
  ProbeStage,
  RetestKind,
} from '@/ProbeResults'
import { Alert, AlertAction, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Field } from '@/components/ui/field'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Sheet, SheetContent, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { bundledIpLibraries } from '@/bundled-ip-libraries'
import { toProbeSettings } from '@/cfst-settings-schema'
import { appendIpEntries, fetchIpLibrary, ipLibraries, parseCidrLibrary } from '@/ip-library'
import type { CfstSettingsController } from '@/use-cfst-settings-form'
import { useProbeInput } from '@/use-probe-input'

const ipLibrarySources = [...bundledIpLibraries, ...ipLibraries]
const ipLibraryOptions = ipLibrarySources.map(({ name }, index) => ({ label: name, value: index }))

function countLines(text: string) {
  return text.split('\n').filter((line) => line.trim()).length
}

function IpListActions({
  disabled,
  desktop,
  ipCount,
  canClear,
  onImport,
  onClear,
}: {
  disabled: boolean
  desktop: boolean
  ipCount: number
  canClear: boolean
  onImport: (entries: string[]) => number
  onClear: () => void
}) {
  const [source, setSource] = useState(0)
  const [pending, setPending] = useState(false)
  const [status, setStatus] = useState('')
  const [clearOpen, setClearOpen] = useState(false)
  const library = ipLibrarySources[source]
  const clearDisabled = disabled || pending || !canClear

  async function importLibrary() {
    setPending(true)
    setStatus('正在读取 IP 库…')
    try {
      let entries: string[]
      if ('entries' in library) {
        entries = library.entries
      } else {
        const body = await fetchIpLibrary(library)
        entries =
          library.format === 'cidr'
            ? parseCidrLibrary(body)
            : await invoke<string[]>('import_ip_subscription', { body })
      }
      if (entries.length) {
        const added = onImport(entries)
        setStatus(added ? `已导入 ${added} 条` : '没有新增 IP')
      } else {
        setStatus('未找到有效 IP')
      }
    } catch (err) {
      setStatus(typeof err === 'string' ? err : '读取失败，请检查网络后重试')
    } finally {
      setPending(false)
    }
  }

  return (
    <div className="ip-list-actions flex flex-col gap-2">
      <div className="probe-ips-header flex h-5 shrink-0 items-center justify-between gap-2">
        <div className="probe-ip-list-heading flex shrink-0 items-center gap-2">
          <Label htmlFor="ip-list" className="text-xs font-medium">
            IP 列表
          </Label>
          <span className="probe-ip-count font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
            {ipCount} 条
          </span>
        </div>
        <output
          className="ip-library-status min-w-0 truncate text-right text-muted-foreground"
          title={status || undefined}
        >
          {status}
        </output>
      </div>
      <div className="ip-list-controls flex items-center gap-2">
        <Select
          items={ipLibraryOptions}
          value={source}
          disabled={disabled || pending}
          onValueChange={(value) => {
            if (value !== null) setSource(value)
          }}
        >
          <SelectTrigger aria-label="IP 库来源" className="min-w-0 flex-1">
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            <SelectGroup>
              {ipLibraryOptions.map(({ label, value }) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant="outline"
          disabled={
            disabled || pending || (!desktop && 'format' in library && library.format === 'ip')
          }
          onClick={() => void importLibrary()}
        >
          {pending ? '导入中…' : '导入'}
        </Button>
        <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
          <AlertDialogTrigger
            aria-label="清空 IP 列表"
            disabled={clearDisabled}
            render={<Button type="button" variant="destructive-ghost" />}
          >
            清空
          </AlertDialogTrigger>
          <AlertDialogContent
            className="ip-list-clear-dialog"
            finalFocus={() => canClear || document.getElementById('ip-list')}
          >
            <AlertDialogHeader>
              <AlertDialogTitle>清空 IP 列表？</AlertDialogTitle>
              <AlertDialogDescription>所有 IP 和订阅链接将被移除。</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>取消</AlertDialogCancel>
              <AlertDialogAction
                type="button"
                variant="destructive"
                disabled={clearDisabled}
                onClick={() => {
                  onClear()
                  setStatus('')
                  setClearOpen(false)
                }}
              >
                确认清空
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  )
}

function SourceEditor({
  id,
  ariaLabel,
  placeholder,
  storage: { value, edit, blocked, status, retry },
}: {
  id: string
  ariaLabel: string
  placeholder: string
  storage: ReturnType<typeof useProbeInput>
}) {
  const storageError = status.state === 'load-error' || status.state === 'save-error'

  return (
    <Field className="source-editor-field flex min-h-0 min-w-0 flex-1 flex-col gap-1.5">
      <Textarea
        id={id}
        aria-label={ariaLabel}
        className="source-editor-textarea flex-1 field-sizing-fixed overflow-auto py-2 font-mono text-xs/5 md:text-xs/5"
        wrap="off"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete="off"
        placeholder={placeholder}
        value={value}
        disabled={blocked}
        onChange={(event) => edit(event.currentTarget.value)}
      />
      <div className="source-editor-status flex h-5 shrink-0 items-center justify-between gap-2 text-xs">
        <output
          className={
            storageError
              ? 'source-editor-feedback truncate text-destructive'
              : 'source-editor-feedback truncate text-muted-foreground'
          }
          title={status.message || undefined}
        >
          {status.message}
        </output>
        {storageError && (
          <Button type="button" size="xs" variant="outline" onClick={retry}>
            {status.state === 'load-error' ? '重新读取' : '重试保存'}
          </Button>
        )}
      </div>
    </Field>
  )
}

const probeStageLabels: Record<ProbeStage, string> = {
  checkingNetwork: '正在检查 TUN/VPN 和代理…',
  preparingTask: '正在准备测速任务…',
  cfst: 'CFST 测速',
  preparingXray: 'CFST 已完成 · 正在准备 Xray…',
  xray: 'Xray 复测',
  savingResults: '正在保存最终结果…',
  completed: '测速完成 · 已保存历史',
  paused: '已暂停 · 可从历史继续',
  failed: '未完成 · 可从历史继续',
}

export function ProbePage({
  active,
  desktop,
  running,
  updating,
  setRunning,
  cfstSettings,
  onOpenSettings,
}: {
  active: boolean
  desktop: boolean
  running: boolean
  updating: boolean
  setRunning: (running: boolean) => void
  cfstSettings: CfstSettingsController
  onOpenSettings: () => void
}) {
  const { form: settingsForm, status: settingsStatus, invalidDraft } = cfstSettings
  const settingsBlocked =
    settingsStatus.state === 'loading' || settingsStatus.state === 'load-error'
  const activeSummary = useStore(settingsForm.store, ({ values }) => {
    const mode = values.mode === 'http' ? 'HTTPing' : 'TCPing'
    const port = values.tp != null && !Number.isNaN(values.tp) ? `:${values.tp}` : ':443'
    const threads = values.n != null && !Number.isNaN(values.n) ? `-n ${values.n}` : null
    const colo = values.mode === 'http' && values.cfcolo.trim() ? values.cfcolo.trim() : null
    const download = values.download ? '下载开' : null
    return [mode + port, threads, colo, download].filter(Boolean).join(' · ')
  })

  const settingsKey = useStore(settingsForm.store, ({ values }) =>
    JSON.stringify(toProbeSettings(values)),
  )
  const ipStorage = useProbeInput(desktop, 'ips')
  const nodeStorage = useProbeInput(desktop, 'nodes')
  const ipList = ipStorage.value
  const vlessNodes = nodeStorage.value
  const ipCount = countLines(ipList)
  const nodeCount = countLines(vlessNodes)

  const [candidatesOpen, setCandidatesOpen] = useState(false)
  const [xrayOverride, setXrayOverride] = useState<boolean | null>(null)
  const xrayEnabled = xrayOverride ?? nodeCount > 0
  const activeVlessNodes = xrayEnabled ? vlessNodes : ''
  const activeNodeCount = xrayEnabled ? nodeCount : 0

  const hasSourceError =
    ipStorage.status.state === 'load-error' ||
    ipStorage.status.state === 'save-error' ||
    (xrayEnabled &&
      (nodeStorage.status.state === 'load-error' || nodeStorage.status.state === 'save-error'))

  const operation = useRef<string | null>(null)
  const [history, setHistory] = useState<HistoryReplay | null>(null)
  const [historyError, setHistoryError] = useState('')
  const [historyStatus, setHistoryStatus] = useState('')
  const [pausable, setPausable] = useState(false)
  const [cancelling, setCancelling] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [status, setStatus] = useState('')
  const [warning, setWarning] = useState<{ message: string } | null>(null)
  const [error, setError] = useState<{ message: string; restart?: boolean } | null>(null)
  const [runId, setRunId] = useState(0)
  const [live, setLive] = useState<LiveProbeState | null>(null)
  const [result, setResult] = useState<{
    groups: ProbeGroup[]
    settings: Record<string, string>
    nodes: string[]
    historyId: string | null
    appendIps?: string
  } | null>(null)

  const canAppend =
    xrayEnabled &&
    !invalidDraft &&
    result?.appendIps === ipList &&
    JSON.stringify(result.settings) === settingsKey
  const cannotStart =
    !desktop ||
    updating ||
    settingsBlocked ||
    ipStorage.blocked ||
    (xrayEnabled && (nodeStorage.blocked || activeNodeCount === 0)) ||
    !ipList.trim()
  const startLabel = updating ? '组件更新中…' : canAppend ? '追加测速' : '开始测速'

  function startProbe(forceFull = false) {
    if (cannotStart || running || settingsForm.state.isSubmitting) return
    const append = !forceFull && canAppend
    void settingsForm.handleSubmit({
      start: async (settings) => {
        if (append && JSON.stringify(settings) !== settingsKey) {
          setError({ message: 'CFST 参数已更改，请重新测速' })
          return
        }
        setCandidatesOpen(false)
        await runProbe(settings, append)
      },
      invalid: () => setError({ message: 'CFST 参数无效，请前往设置修正字段错误。' }),
    })
  }

  function retestProgress(operationId: string, updateStatus: (message: string) => void) {
    return new Channel<string>((message) => {
      if (operation.current !== operationId) return
      if (message.startsWith('测速前提示：')) {
        setWarning({ message })
        updateStatus('等待确认网络提示…')
      } else updateStatus(message)
    })
  }

  function probeProgress(
    operationId: string,
    updateStatus: (message: string) => void,
    expectDownload: boolean,
    onStart?: () => void,
  ) {
    let latest: LiveProbeState | null = null
    let accepting = true
    let hasRecord = false
    let historyId: string | null = null
    let failed = false
    const progress = new Channel<ProbeEvent>((event) => {
      if (!accepting || operation.current !== operationId) return
      if (event.kind === 'routeWarning') {
        setWarning(event)
        updateStatus('等待确认网络提示…')
      } else if (event.kind === 'stage') {
        const { stage, progress: counts } = event
        const mayPause =
          stage === 'cfst' ||
          stage === 'preparingXray' ||
          stage === 'xray' ||
          stage === 'savingResults'
        if (mayPause) hasRecord = true
        failed = stage === 'failed'
        setPausable(mayPause)
        const label = probeStageLabels[stage]
        updateStatus(counts ? `${label} · ${counts[0]}/${counts[1]}` : label)
      } else if (event.kind === 'subscription') {
        updateStatus(`正在获取 IP 订阅（第 ${event.line} 行）`)
      } else {
        if (event.kind === 'candidates') {
          historyId = event.history_id
          onStart?.()
        }
        latest = applyProbeEvent(latest, event, expectDownload)
        setLive(latest)
      }
    })
    return {
      progress,
      hasRecord: () => hasRecord,
      failed: () => failed,
      historyId: () => historyId,
      snapshot: () => (latest ? snapshotLiveGroups(latest) : []),
      close: () => {
        accepting = false
      },
    }
  }

  function beginOperation() {
    if (operation.current) return null
    const id = crypto.getRandomValues(new Uint32Array(4)).join('-')
    operation.current = id
    setRunning(true)
    return id
  }

  async function runProbe(settings: Record<string, string>, append: boolean) {
    const operationId = beginOperation()
    if (!operationId) return
    setHistory(null)
    setStatus(append ? '正在准备 Xray 追加测速…' : '正在校验候选 IP…')
    setError(null)
    const run = probeProgress(operationId, setStatus, settings.download === 'yes', () => {
      setRunId((id) => id + 1)
      setResult((previous) => (append && previous ? { ...previous, appendIps: undefined } : null))
    })
    const { progress } = run
    const nodes = activeVlessNodes
      .split('\n')
      .map((line) => line.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, ''))
      .filter(Boolean)
    try {
      type Report = { id: string; groups: ProbeGroup[] } | null
      const response = append
        ? await invoke<Report>('append_xray_probe', {
            operationId,
            nodes: activeVlessNodes,
            progress,
          })
        : await invoke<Report>('run_probe', {
            operationId,
            request: JSON.stringify({ ips: ipList, nodes: activeVlessNodes, settings }),
            progress,
          })
      if (response === null) {
        const partial = run.snapshot()
        if (partial.length > 0)
          setResult({ groups: partial, nodes, settings, historyId: run.historyId() })
        setStatus(
          run.hasRecord()
            ? append
              ? 'CFST 已完成 · Xray 已暂停'
              : probeStageLabels.paused
            : '已取消测速',
        )
      } else {
        const appendIps =
          !append && nodes.length === 0 && response.groups.some((group) => group.cfst.length > 0)
            ? ipList
            : undefined
        setResult({ groups: response.groups, historyId: response.id, nodes, settings, appendIps })
        setStatus(append ? '追加测速完成 · 已保存历史' : '测速完成 · 已保存历史')
      }
    } catch (err) {
      setError({ message: String(err), restart: append })
      const partial = run.snapshot()
      if (partial.length > 0)
        setResult({ groups: partial, nodes, settings, historyId: run.historyId() })
      setStatus(
        run.failed() ? (append ? 'CFST 已完成 · Xray 未完成' : probeStageLabels.failed) : '',
      )
    } finally {
      run.close()
      finishOperation()
    }
  }

  async function resumeHistory(replay: HistoryReplay) {
    if (!desktop || running || updating || !canResumeHistory(replay.detail.status)) return
    const operationId = beginOperation()
    if (!operationId) return
    setHistoryError('')
    setHistoryStatus('正在读取测速检查点…')
    const run = probeProgress(operationId, setHistoryStatus, replay.detail.download)
    const { id } = replay.entry
    try {
      await invoke('resume_probe', { operationId, id, nodes: vlessNodes, progress: run.progress })
    } catch (err) {
      setHistoryError(String(err))
    } finally {
      run.close()
      try {
        const detail = await invoke<HistoryDetail>('read_probe_history', { id })
        setHistory({ entry: replay.entry, detail })
        if (result?.historyId === id) {
          setResult({ ...result, groups: detail.groups })
          setStatus(
            detail.appendedXray
              ? `CFST 已完成 · Xray ${historyStatusLabels[detail.status]}`
              : historyStatusLabels[detail.status],
          )
        }
      } catch (err) {
        setHistoryError((message) => message || String(err))
      }
      setHistoryStatus('')
      finishOperation()
    }
  }

  function finishOperation() {
    operation.current = null
    setConfirming(false)
    setLive(null)
    setRunning(false)
    setPausable(false)
    setCancelling(false)
    setWarning(null)
  }

  async function retest(kind: RetestKind, endpoint: string, node: number | null) {
    if (!desktop || running || updating) return null
    const link = node === null ? undefined : result?.nodes[node - 1]
    if (!history && (!result || (kind === 'xray' && !link))) return null
    const operationId = beginOperation()
    if (!operationId) return null
    const updateStatus = history ? setHistoryStatus : setStatus
    updateStatus(`正在重测 ${endpoint}…`)
    if (history) setHistoryError('')
    else setError(null)
    const progress = retestProgress(operationId, updateStatus)
    try {
      type Response = { row: ProbeRow | null } | null
      const response = history
        ? await invoke<Response>('retest_history_probe', {
            operationId,
            request: { id: history.entry.id, endpoint, node, kind, nodes: vlessNodes },
            progress,
          })
        : await invoke<Response>('retest_probe', {
            operationId,
            request: JSON.stringify({
              endpoint,
              test: kind === 'cfst' ? { kind, settings: result?.settings } : { kind, node: link },
            }),
            progress,
          })
      updateStatus(
        response === null
          ? '已取消重测'
          : response.row
            ? '重测完成 · 临时结果'
            : 'CFST 重测未返回符合条件的结果',
      )
      return response
    } catch (err) {
      if (history) setHistoryError(String(err))
      else setError({ message: String(err) })
      updateStatus('')
      return null
    } finally {
      finishOperation()
    }
  }

  function reportControlError(err: unknown) {
    if (history) setHistoryError(String(err))
    else setError({ message: String(err) })
  }

  async function continueProbe() {
    const operationId = operation.current
    if (!operationId) return
    setConfirming(true)
    try {
      await invoke('approve_probe', { operationId })
      if (operation.current === operationId)
        setWarning((current) => (current === warning ? null : current))
    } catch (err) {
      if (operation.current === operationId) reportControlError(err)
    } finally {
      if (operation.current === operationId) setConfirming(false)
    }
  }

  async function stopProbe() {
    const operationId = operation.current
    if (!operationId) return
    setCancelling(true)
    try {
      await invoke('stop_probe', { operationId })
    } catch (err) {
      if (operation.current !== operationId) return
      reportControlError(err)
      setCancelling(false)
    }
  }

  const stopLabel = pausable
    ? cancelling
      ? '正在暂停…'
      : '暂停测速'
    : cancelling
      ? '正在取消…'
      : '取消测速'
  const footerText = history
    ? historyStatus ||
      `${history.detail.appendedXray ? 'CFST 已完成 · Xray ' : ''}${historyStatusLabels[history.detail.status]}`
    : status ||
      (!desktop
        ? '请在桌面应用中测速'
        : settingsStatus.state === 'loading'
          ? '正在读取 CFST 参数…'
          : settingsStatus.state === 'load-error'
            ? 'CFST 参数读取失败'
            : '就绪')

  return (
    <>
      <Dialog
        open={running && Boolean(warning)}
        disablePointerDismissal
        onOpenChange={(open) => {
          if (!open && !cancelling && !confirming) void stopProbe()
        }}
      >
        <DialogContent showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>测速前网络提示</DialogTitle>
            <DialogDescription>{warning?.message.replace(/^测速前提示：/, '')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={cancelling || confirming}
              onClick={() => void stopProbe()}
            >
              {stopLabel}
            </Button>
            <Button disabled={cancelling || confirming} onClick={() => void continueProbe()}>
              继续测速
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Sheet open={candidatesOpen} onOpenChange={setCandidatesOpen}>
        <SheetContent keepMounted className="probe-candidates-sheet max-w-[min(26rem,90vw)]">
          <SheetHeader className="probe-candidates-header">
            <SheetTitle>候选 IP</SheetTitle>
          </SheetHeader>
          <div className="probe-candidates-body flex min-h-0 flex-1 flex-col gap-3 p-4">
            <div className="probe-ips-section flex min-h-0 flex-[3] flex-col gap-2">
              <IpListActions
                disabled={ipStorage.blocked}
                desktop={desktop}
                ipCount={ipCount}
                canClear={Boolean(ipList)}
                onClear={() => ipStorage.edit('')}
                onImport={(entries) => {
                  let added = 0
                  ipStorage.edit((value) => {
                    const next = appendIpEntries(value, entries)
                    added = countLines(next) - countLines(value)
                    return next
                  })
                  return added
                }}
              />
              <SourceEditor
                id="ip-list"
                ariaLabel="IP 列表"
                placeholder={'203.0.113.10\n203.0.113.20:443\nhttps://…'}
                storage={ipStorage}
              />
            </div>

            <Separator />

            <div
              className={`probe-xray-section flex flex-col gap-2 ${xrayEnabled ? 'min-h-0 flex-[2]' : 'shrink-0'}`}
            >
              <div className="probe-xray-toggle-row flex items-center justify-between gap-3">
                <Label
                  htmlFor="xray-retest-toggle"
                  className="probe-xray-label flex items-center gap-2 text-xs font-medium"
                >
                  <span>Xray 复测</span>
                  {nodeCount > 0 && (
                    <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
                      {nodeCount} 条
                    </span>
                  )}
                </Label>
                <Switch
                  id="xray-retest-toggle"
                  checked={xrayEnabled}
                  onCheckedChange={setXrayOverride}
                />
              </div>
              {xrayEnabled && (
                <SourceEditor
                  id="vless-nodes"
                  ariaLabel="VLESS 节点"
                  placeholder="vless://…"
                  storage={{
                    ...nodeStorage,
                    edit: (change) => {
                      setXrayOverride(true)
                      nodeStorage.edit(change)
                    },
                  }}
                />
              )}
            </div>
          </div>
          <SheetFooter className="probe-candidates-footer">
            <Button type="button" variant="outline" onClick={() => setCandidatesOpen(false)}>
              关闭
            </Button>
            {running ? (
              <Button variant="destructive" disabled={cancelling} onClick={() => void stopProbe()}>
                <Square data-icon="inline-start" aria-hidden="true" />
                {stopLabel}
              </Button>
            ) : !history ? (
              <Button disabled={cannotStart} onClick={() => startProbe()}>
                <Play data-icon="inline-start" aria-hidden="true" />
                {startLabel}
              </Button>
            ) : null}
          </SheetFooter>
        </SheetContent>
      </Sheet>

      <section
        id="probe-page"
        aria-labelledby="probe-title"
        hidden={!active}
        className="probe-page h-full"
      >
        <div className="probe-layout flex h-full min-h-0 flex-col">
          <header className="probe-toolbar flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-border px-4 py-2.5">
            <div className="probe-toolbar-leading flex min-w-0 flex-wrap items-center gap-2">
              <h1
                id="probe-title"
                className="probe-title text-sm font-semibold tracking-[-0.015em]"
              >
                IP 优选
              </h1>
              {!history && (
                <button
                  type="button"
                  className={`probe-config-pill inline-flex h-6 items-center gap-1.5 rounded-sm border px-2 font-mono text-[0.6875rem] transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/30 ${
                    invalidDraft || settingsStatus.state === 'load-error'
                      ? 'border-destructive/40 bg-destructive/10 text-destructive'
                      : 'border-border bg-muted text-text-normal hover:bg-muted-hover hover:text-foreground'
                  }`}
                  onClick={onOpenSettings}
                >
                  <SlidersHorizontal className="size-3 shrink-0" aria-hidden="true" />
                  <span className="truncate">{invalidDraft ? '参数无效' : activeSummary}</span>
                </button>
              )}
              {!history && activeNodeCount > 0 && (
                <button
                  type="button"
                  className="probe-nodes-pill inline-flex h-6 items-center rounded-sm border border-border bg-muted px-2 font-mono text-[0.6875rem] text-text-normal tabular-nums transition-colors outline-none hover:bg-muted-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/30"
                  onClick={() => setCandidatesOpen(true)}
                >
                  Xray ×{activeNodeCount}
                </button>
              )}
            </div>
            <div className="probe-toolbar-actions flex shrink-0 items-center gap-2">
              <Button
                type="button"
                variant="outline"
                className={hasSourceError ? 'border-destructive/50 text-destructive' : undefined}
                onClick={() => setCandidatesOpen(true)}
              >
                <ListFilter data-icon="inline-start" aria-hidden="true" />
                候选 IP
                <span className="font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
                  {ipCount}
                </span>
              </Button>
              <ProbeHistory
                disabled={!desktop || running}
                onView={(replay) => {
                  setHistory(replay)
                  setHistoryError('')
                  setHistoryStatus('')
                }}
                onDelete={(id) => setHistory((replay) => (replay?.entry.id === id ? null : replay))}
              />
              {running ? (
                <Button
                  variant="destructive"
                  disabled={cancelling}
                  onClick={() => void stopProbe()}
                >
                  <Square data-icon="inline-start" aria-hidden="true" />
                  {stopLabel}
                </Button>
              ) : !history ? (
                <Button disabled={cannotStart} onClick={() => startProbe()}>
                  <Play data-icon="inline-start" aria-hidden="true" />
                  {startLabel}
                </Button>
              ) : null}
            </div>
          </header>

          {running && (
            <div
              className="probe-progress-bar h-0.5 w-full shrink-0 overflow-hidden bg-muted"
              aria-hidden="true"
            >
              <div className="probe-progress-indicator h-full w-1/3 bg-dot-status-working" />
            </div>
          )}

          {history && (
            <section
              aria-label="历史回放"
              className="probe-history-replay flex min-h-0 min-w-0 flex-1 flex-col"
            >
              <div className="probe-replay-bar flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-2">
                <div className="probe-replay-meta flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                  <time
                    className="probe-replay-time font-mono tabular-nums"
                    dateTime={new Date(history.entry.startedAt).toISOString()}
                  >
                    {historyDate.format(history.entry.startedAt)}
                  </time>
                  {canResumeHistory(history.detail.status) && !live && (
                    <span className="probe-replay-progress text-muted-foreground">
                      CFST {history.detail.cfstProgress.join(' / ')}
                      {history.detail.xrayProgress && (
                        <> · Xray {history.detail.xrayProgress.join(' / ')}</>
                      )}
                    </span>
                  )}
                </div>
                <div className="probe-replay-actions flex items-center gap-2">
                  <Button
                    size="xs"
                    variant="outline"
                    disabled={running}
                    onClick={() => setHistory(null)}
                  >
                    返回当前结果
                  </Button>
                  {canResumeHistory(history.detail.status) && (
                    <Button
                      size="xs"
                      disabled={running || updating}
                      onClick={() => void resumeHistory(history)}
                    >
                      继续测速
                    </Button>
                  )}
                </div>
              </div>
              {historyError && (
                <Alert
                  variant="destructive"
                  className="probe-replay-error mx-4 mt-3 w-auto shrink-0"
                >
                  <AlertDescription className="probe-replay-error-message wrap-anywhere">
                    {historyError}
                  </AlertDescription>
                </Alert>
              )}
              <ProbeResults
                key={history.entry.id}
                groups={history.detail.groups}
                live={live ?? undefined}
                emptyTitle={
                  canResumeHistory(history.detail.status) ? '暂无已保存的测速结果' : undefined
                }
                actions={
                  history.detail.status === 'completed'
                    ? { disabled: running || updating, run: retest }
                    : undefined
                }
              />
            </section>
          )}

          <div
            hidden={history !== null}
            className="probe-canvas min-h-0 min-w-0 flex-1 flex-col [&:not([hidden])]:flex"
          >
            {error && (
              <Alert
                variant="destructive"
                className="probe-error mx-4 mt-3 max-h-24 w-auto shrink-0 overflow-auto"
              >
                <AlertDescription className="probe-error-message wrap-anywhere">
                  {error.message}
                </AlertDescription>
                {error.restart && canAppend && (
                  <AlertAction className="probe-error-restart">
                    <Button
                      variant="outline"
                      size="xs"
                      disabled={running || cannotStart}
                      onClick={() => startProbe(true)}
                    >
                      重新测速
                    </Button>
                  </AlertAction>
                )}
              </Alert>
            )}
            {(!history && live) || result ? (
              <ProbeResults
                key={runId}
                groups={result?.groups}
                live={history ? undefined : (live ?? undefined)}
                actions={desktop ? { disabled: running || updating, run: retest } : undefined}
              />
            ) : (
              <Empty className="probe-empty min-h-0 overflow-auto">
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <Server aria-hidden="true" />
                  </EmptyMedia>
                  <EmptyTitle>
                    {running ? (pausable ? '正在测速' : '正在准备测速') : '暂无测速结果'}
                  </EmptyTitle>
                  {running && status && <EmptyDescription>{status}</EmptyDescription>}
                </EmptyHeader>
                {!running && (
                  <EmptyContent>
                    <Button type="button" variant="outline" onClick={() => setCandidatesOpen(true)}>
                      <ListFilter data-icon="inline-start" aria-hidden="true" />
                      编辑候选 IP
                    </Button>
                  </EmptyContent>
                )}
              </Empty>
            )}
          </div>

          <footer className="probe-footer flex h-7 shrink-0 items-center justify-between gap-3 border-t border-border bg-sidebar px-4 text-xs text-app-muted-foreground">
            <output className="probe-status-output flex min-w-0 items-center gap-1.5">
              <span
                className={`probe-status-dot size-1.5 shrink-0 rounded-full ${
                  running
                    ? 'animate-pulse bg-dot-status-working'
                    : result
                      ? 'bg-status-ok'
                      : 'bg-muted-foreground/60'
                }`}
                aria-hidden="true"
              />
              <span className="truncate" title={footerText}>
                {footerText}
              </span>
            </output>
            <span className="probe-mode-badge shrink-0 font-mono text-[0.6875rem] text-muted-foreground">
              {history
                ? history.detail.groups.some((group) => group.node !== null)
                  ? 'CFST + Xray'
                  : '仅 CFST'
                : activeNodeCount > 0
                  ? 'CFST + Xray'
                  : '仅 CFST'}
            </span>
          </footer>
        </div>
      </section>
    </>
  )
}
