import { invoke } from '@tauri-apps/api/core'
import { History } from 'lucide-react'
import { useRef, useState } from 'react'
import type { ProbeGroup } from '@/ProbeResults'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from '@/components/ui/item'
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from '@/components/ui/sheet'

export const historyStatusLabels = {
  interrupted: '已中断',
  paused: '已暂停',
  failed: '未完成',
  completed: '已完成',
  unavailable: '无法读取',
}

type HistoryStatus = keyof typeof historyStatusLabels
export type HistoryEntry = { id: string; startedAt: number; status: HistoryStatus }
export type HistoryDetail = {
  status: HistoryStatus
  groups: ProbeGroup[]
  download: boolean
  cfstProgress: [number, number]
  xrayProgress: [number, number] | null
  appendedXray: boolean
}
export type HistoryReplay = { entry: HistoryEntry; detail: HistoryDetail }
export const historyDate = new Intl.DateTimeFormat('zh-CN', {
  dateStyle: 'medium',
  timeStyle: 'medium',
})

export function canResumeHistory(status: HistoryStatus) {
  return status === 'interrupted' || status === 'paused' || status === 'failed'
}

export function ProbeHistory({
  disabled,
  onView,
  onDelete,
}: {
  disabled: boolean
  onView: (replay: HistoryReplay) => void
  onDelete: (id: string) => void
}) {
  const heading = useRef<HTMLHeadingElement>(null)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [entries, setEntries] = useState<HistoryEntry[]>([])
  const [pendingDelete, setPendingDelete] = useState<HistoryEntry | null>(null)

  async function perform(action: () => Promise<void>) {
    setBusy(true)
    setError('')
    try {
      await action()
    } catch (err) {
      setError(String(err))
    } finally {
      setBusy(false)
    }
  }

  function changeOpen(next: boolean) {
    if (busy) return
    setOpen(next)
    if (next) {
      setPendingDelete(null)
      setEntries([])
      void perform(async () => setEntries(await invoke<HistoryEntry[]>('list_probe_history')))
    }
  }

  function view(entry: HistoryEntry) {
    void perform(async () => {
      const detail = await invoke<HistoryDetail>('read_probe_history', { id: entry.id })
      onView({ entry, detail })
      setOpen(false)
    })
  }

  function remove(entry: HistoryEntry) {
    void perform(async () => {
      await invoke('delete_probe_history', { id: entry.id })
      onDelete(entry.id)
      setPendingDelete(null)
      setEntries((current) => current.filter(({ id }) => id !== entry.id))
    })
  }

  return (
    <Sheet open={open} onOpenChange={changeOpen}>
      <SheetTrigger render={<Button variant="outline" disabled={disabled} />}>
        <History data-icon="inline-start" aria-hidden="true" />
        历史
      </SheetTrigger>
      <SheetContent className="probe-history-sheet max-w-[min(28rem,92vw)]">
        <SheetHeader className="probe-history-header">
          <SheetTitle ref={heading} tabIndex={-1} className="probe-history-title">
            测速历史
          </SheetTitle>
        </SheetHeader>
        {error && !pendingDelete && (
          <Alert variant="destructive" className="probe-history-error mx-4 my-2 w-auto shrink-0">
            <AlertDescription className="probe-history-error-message wrap-anywhere">
              {error}
            </AlertDescription>
          </Alert>
        )}
        {busy && (
          <output className="probe-history-status block border-b border-border bg-sidebar px-4 py-1.5 text-muted-foreground">
            处理中…
          </output>
        )}
        <div className="probe-history-list min-h-0 flex-1 overflow-y-auto px-3 py-2">
          <AlertDialog
            open={pendingDelete !== null}
            triggerId={pendingDelete ? `delete-history-${pendingDelete.id}` : null}
            onOpenChange={(next, event) => {
              if (busy) event.cancel()
              else if (!next) {
                setPendingDelete(null)
                setError('')
              }
            }}
          >
            {entries.length > 0 ? (
              <ItemGroup aria-label="测速历史记录" className="probe-history-entries gap-1">
                {entries.map((entry) => (
                  <Item
                    key={entry.id}
                    size="sm"
                    variant="outline"
                    render={<li className="probe-history-entry bg-background hover:bg-muted/40" />}
                  >
                    <ItemContent className="probe-history-entry-content">
                      <ItemTitle>
                        <time
                          className="probe-history-timestamp font-mono tabular-nums"
                          dateTime={new Date(entry.startedAt).toISOString()}
                        >
                          {historyDate.format(entry.startedAt)}
                        </time>
                      </ItemTitle>
                      <ItemDescription className="probe-history-entry-status">
                        {historyStatusLabels[entry.status]}
                      </ItemDescription>
                    </ItemContent>
                    <ItemActions className="probe-history-actions ml-auto">
                      <Button
                        variant="outline"
                        size="xs"
                        disabled={busy || entry.status === 'unavailable'}
                        onClick={() => view(entry)}
                      >
                        查看
                      </Button>
                      <AlertDialogTrigger
                        id={`delete-history-${entry.id}`}
                        disabled={busy}
                        render={<Button variant="destructive" size="xs" />}
                        onClick={() => {
                          setError('')
                          setPendingDelete(entry)
                        }}
                      >
                        删除
                      </AlertDialogTrigger>
                    </ItemActions>
                  </Item>
                ))}
              </ItemGroup>
            ) : (
              !busy &&
              !error && (
                <Empty className="probe-history-empty">
                  <EmptyHeader>
                    <EmptyTitle>暂无测速历史</EmptyTitle>
                  </EmptyHeader>
                </Empty>
              )
            )}
            <AlertDialogContent
              className="probe-history-delete-dialog"
              finalFocus={() => entries.length > 0 || heading.current}
            >
              <AlertDialogHeader>
                <AlertDialogTitle>删除测速记录？</AlertDialogTitle>
                <AlertDialogDescription>
                  {pendingDelete && historyDate.format(pendingDelete.startedAt)}{' '}
                  的记录将被永久删除。
                </AlertDialogDescription>
              </AlertDialogHeader>
              {error && pendingDelete && (
                <Alert variant="destructive" className="probe-history-delete-error">
                  <AlertDescription className="probe-history-delete-error-message wrap-anywhere">
                    {error}
                  </AlertDescription>
                </Alert>
              )}
              <AlertDialogFooter>
                <AlertDialogCancel disabled={busy}>取消</AlertDialogCancel>
                <AlertDialogAction
                  variant="destructive"
                  disabled={busy || !pendingDelete}
                  onClick={() => pendingDelete && remove(pendingDelete)}
                >
                  {busy ? '正在删除…' : '确认删除'}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </SheetContent>
    </Sheet>
  )
}
