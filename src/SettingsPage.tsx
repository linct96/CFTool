import { useState } from 'react'
import { CfstSettings } from '@/CfstSettings'
import { EngineUpdates } from '@/EngineUpdates'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTab } from '@/components/ui/tabs'
import type { CfstSettingsController } from '@/use-cfst-settings-form'

const sections = [
  { id: 'cfst', label: 'CFST 参数' },
  { id: 'engines', label: '依赖工具' },
] as const

type SettingsSection = (typeof sections)[number]['id']

export function SettingsPage({
  active,
  desktop,
  running,
  setUpdating,
  settings,
}: {
  active: boolean
  desktop: boolean
  running: boolean
  setUpdating: (updating: boolean) => void
  settings: CfstSettingsController
}) {
  const { form, status, retry, invalidDraft } = settings
  const [section, setSection] = useState<SettingsSection>('cfst')

  return (
    <section
      id="settings-page"
      aria-labelledby="settings-title"
      hidden={!active}
      className="settings-page h-full overflow-hidden [&:not([hidden])]:flex [&:not([hidden])]:flex-col"
    >
      <header className="settings-toolbar flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b border-border px-4 py-2.5">
        <div className="settings-toolbar-leading flex items-center gap-3">
          <h1
            id="settings-title"
            className="settings-title text-sm font-semibold tracking-[-0.015em]"
          >
            设置
          </h1>
          <Tabs
            value={section}
            onValueChange={(value) => {
              if (value === 'cfst' || value === 'engines') setSection(value)
            }}
          >
            <TabsList aria-label="设置分区">
              {sections.map(({ id, label }) => (
                <TabsTab key={id} value={id}>
                  {label}
                </TabsTab>
              ))}
            </TabsList>
          </Tabs>
        </div>
        {section === 'cfst' && (
          <output
            className={`cfst-storage-status text-xs ${
              invalidDraft ? 'text-destructive' : 'text-app-muted-foreground'
            }`}
          >
            {status.state === 'loading'
              ? '正在读取…'
              : status.state === 'saving'
                ? '正在保存…'
                : invalidDraft
                  ? '有无效参数，未保存'
                  : null}
          </output>
        )}
      </header>

      <div className="settings-detail-pane flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto bg-background [scrollbar-gutter:stable]">
        <div className="settings-detail-container @container mx-auto flex min-h-full w-full max-w-3xl shrink-0 flex-col px-4 py-5 pb-12 text-xs/relaxed sm:px-6">
          {/* Keep the CFST form and any in-flight tool updates mounted when switching sections. */}
          <div hidden={section !== 'cfst'} className="settings-detail-section">
            {(status.state === 'load-error' || status.state === 'save-error') && (
              <Alert
                variant="destructive"
                className="cfst-storage-error mb-4 flex flex-wrap items-center justify-between gap-2"
              >
                <AlertDescription className="cfst-storage-error-message min-w-0 wrap-anywhere">
                  {status.state === 'load-error' ? '读取失败' : '保存失败'}：{status.message}
                </AlertDescription>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={retry}
                  disabled={status.state === 'save-error' && invalidDraft}
                >
                  重试
                </Button>
              </Alert>
            )}
            {status.state !== 'loading' && status.state !== 'load-error' && (
              <CfstSettings form={form} />
            )}
          </div>

          <div hidden={section !== 'engines'} className="settings-detail-section">
            <EngineUpdates
              active={active && section === 'engines'}
              desktop={desktop}
              running={running}
              setUpdating={setUpdating}
            />
          </div>
        </div>
      </div>
    </section>
  )
}
