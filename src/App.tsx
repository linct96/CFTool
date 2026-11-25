import { GaugeIcon } from '@phosphor-icons/react/dist/csr/Gauge'
import { GearSixIcon } from '@phosphor-icons/react/dist/csr/GearSix'
import { isTauri } from '@tauri-apps/api/core'
import { useState } from 'react'
import { ProbePage } from '@/ProbePage'
import { SettingsPage } from '@/SettingsPage'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { useCfstSettingsForm } from '@/use-cfst-settings-form'

const navigation = [
  { id: 'probe', label: '优选', icon: GaugeIcon },
  { id: 'settings', label: '设置', icon: GearSixIcon },
] as const

function App() {
  const [page, setPage] = useState<(typeof navigation)[number]['id']>('probe')
  const [running, setRunning] = useState(false)
  const [updating, setUpdating] = useState(false)
  const desktop = isTauri()
  const nativeTitlebar = import.meta.env.TAURI_ENV_PLATFORM === 'linux'
  const settings = useCfstSettingsForm(desktop)

  return (
    <div className="app-shell flex h-dvh flex-col overflow-hidden bg-muted">
      {(!nativeTitlebar || running) && (
        <header
          data-tauri-drag-region={nativeTitlebar ? undefined : 'deep'}
          className="app-titlebar flex h-11 shrink-0 items-center justify-end px-3 text-xs select-none"
        >
          <output className="app-running-indicator flex items-center gap-1.5 text-app-muted-foreground">
            {running && (
              <>
                <span
                  className="app-running-dot size-1.5 animate-pulse rounded-full bg-dot-status-working"
                  aria-hidden="true"
                />
                测速进行中
              </>
            )}
          </output>
        </header>
      )}
      <div className="app-frame flex min-h-0 flex-1">
        <TooltipProvider>
          <nav
            aria-label="主导航"
            className="app-rail flex w-12 shrink-0 flex-col items-center justify-between pt-2 pb-3"
          >
            {navigation.map(({ id, label, icon: Icon }) => (
              <Tooltip key={id}>
                <TooltipTrigger
                  render={
                    <Button
                      type="button"
                      variant="navigation"
                      size="icon-xl"
                      aria-label={label}
                      aria-controls={`${id}-page`}
                      aria-current={page === id ? 'page' : undefined}
                      onClick={() => setPage(id)}
                    />
                  }
                >
                  <Icon weight={page === id ? 'fill' : 'regular'} aria-hidden="true" />
                </TooltipTrigger>
                <TooltipContent side="right">{label}</TooltipContent>
              </Tooltip>
            ))}
          </nav>
        </TooltipProvider>
        <main className="app-workspace min-h-0 min-w-0 flex-1 overflow-hidden rounded-tl-2xl border-t border-l border-border bg-background">
          {/* Keep drafts and in-flight operations mounted when changing pages. */}
          <ProbePage
            active={page === 'probe'}
            desktop={desktop}
            running={running}
            updating={updating}
            setRunning={setRunning}
            cfstSettings={settings}
            onOpenSettings={() => setPage('settings')}
          />
          <SettingsPage
            active={page === 'settings'}
            desktop={desktop}
            running={running}
            setUpdating={setUpdating}
            settings={settings}
          />
        </main>
      </div>
    </div>
  )
}

export default App
