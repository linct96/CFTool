import { invoke } from '@tauri-apps/api/core'
import { RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Item, ItemActions, ItemContent, ItemGroup, ItemTitle } from '@/components/ui/item'

const engines = { cfst: 'CFST', xray: 'Xray' } as const
const engineIds = ['cfst', 'xray'] as const
type Engine = (typeof engineIds)[number]
type InspectAction = 'status' | 'check'
type ChangeAction = 'install' | 'restore'
type Action = InspectAction | ChangeAction
type Status = {
  current: string
  downloaded: boolean
  hasDownload: boolean
  latest: string | null
  updateAvailable: boolean
}

const busyLabels = {
  status: '读取版本…',
  check: '正在检查更新…',
  install: '正在下载并校验…',
  restore: '正在恢复…',
}

export function EngineUpdates({
  active,
  desktop,
  running,
  setUpdating,
}: {
  active: boolean
  desktop: boolean
  running: boolean
  setUpdating: (updating: boolean) => void
}) {
  const requested = useRef(false)
  const [statuses, setStatuses] = useState<Partial<Record<Engine, Status>>>({})
  const [busy, setBusy] = useState<{ engine: Engine | null; action: Action } | null>(null)
  const [errors, setErrors] = useState<Partial<Record<Engine, string>>>({})

  const load = useCallback(async (engine: Engine, action: Action, expected?: string) => {
    const status = await invoke<Status>('engine_update', { engine, action, expected })
    setStatuses((previous) => ({ ...previous, [engine]: status }))
  }, [])

  const inspect = useCallback(
    async (action: InspectAction) => {
      setBusy({ engine: null, action })
      setErrors({})
      const failed: Partial<Record<Engine, string>> = {}
      await Promise.all(
        engineIds.map(async (engine) => {
          try {
            await load(engine, action)
          } catch (cause) {
            if (action === 'check') await load(engine, 'status').catch(() => undefined)
            failed[engine] = String(cause)
          }
        }),
      )
      setErrors(failed)
      setBusy(null)
    },
    [load],
  )

  useEffect(() => {
    if (!active || !desktop || requested.current) return
    requested.current = true
    void inspect('status')
  }, [active, desktop, inspect])

  async function perform(engine: Engine, action: ChangeAction, expected?: string) {
    setUpdating(true)
    setBusy({ engine, action })
    setErrors((previous) => ({ ...previous, [engine]: undefined }))
    try {
      await load(engine, action, expected)
    } catch (cause) {
      await load(engine, 'status').catch(() => undefined)
      setErrors((previous) => ({ ...previous, [engine]: String(cause) }))
    } finally {
      setUpdating(false)
      setBusy(null)
    }
  }

  const disabled = !desktop || running || Boolean(busy)

  return (
    <section className="engine-tools flex flex-col gap-3" aria-labelledby="engine-tools-title">
      <div className="engine-tools-heading flex flex-wrap items-center justify-between gap-3">
        <h2 id="engine-tools-title" className="text-xs font-semibold">
          测速组件
        </h2>
        <Button
          size="sm"
          variant="outline"
          disabled={disabled}
          aria-busy={busy?.action === 'check'}
          onClick={() => void inspect('check')}
        >
          <RefreshCw
            data-icon="inline-start"
            className={busy?.action === 'check' ? 'animate-spin' : undefined}
            aria-hidden="true"
          />
          {busy?.action === 'check' ? '检查中…' : '检查更新'}
        </Button>
      </div>
      <ItemGroup className="engine-tools-list gap-2.5">
        {engineIds.map((engine) => {
          const status = statuses[engine]
          const latest = status?.latest
          const error = errors[engine]
          return (
            <Item
              key={engine}
              render={<li className="engine-tool-row bg-card" />}
              variant="outline"
              aria-label={engines[engine]}
              aria-busy={Boolean(busy && (busy.engine === null || busy.engine === engine))}
            >
              <ItemContent className="engine-tool-content min-w-0">
                <ItemTitle className="engine-tool-title">
                  <span>{engines[engine]}</span>
                  {status && (
                    <span className="engine-source-badge rounded-sm border border-border bg-muted px-1.5 py-0.25 text-[0.625rem] font-medium text-muted-foreground">
                      {status.downloaded ? '已下载' : '内置'}
                    </span>
                  )}
                </ItemTitle>
                <output className="engine-tool-status block font-mono text-muted-foreground tabular-nums">
                  {status ? `当前 ${status.current}` : '尚未读取版本'}
                  {latest && ` · 最新 ${latest}`}
                  {latest && !error && (status.updateAvailable ? ' · 有可用更新' : ' · 已是最新')}
                  {busy &&
                    (busy.engine === null || busy.engine === engine) &&
                    ` · ${busyLabels[busy.action]}`}
                </output>
              </ItemContent>
              <ItemActions className="engine-tool-actions flex-wrap">
                {status?.updateAvailable && latest && (
                  <Button
                    size="sm"
                    disabled={disabled}
                    onClick={() => void perform(engine, 'install', latest)}
                  >
                    更新到 {latest}
                  </Button>
                )}
                {status?.hasDownload && (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={disabled}
                    onClick={() => void perform(engine, 'restore')}
                  >
                    恢复内置版本
                  </Button>
                )}
              </ItemActions>
              {error && (
                <Alert variant="destructive" className="engine-tool-error basis-full">
                  <AlertDescription className="engine-tool-error-message wrap-anywhere">
                    {error}
                  </AlertDescription>
                </Alert>
              )}
            </Item>
          )
        })}
      </ItemGroup>
      {(!desktop || running) && (
        <p className="engine-tools-notice text-app-muted-foreground">
          {desktop ? '测速结束后可检查和更新组件。' : '请在桌面应用中检查和更新组件。'}
        </p>
      )}
    </section>
  )
}
