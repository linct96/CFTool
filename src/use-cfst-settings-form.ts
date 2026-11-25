import { revalidateLogic, useForm, useStore } from '@tanstack/react-form'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import {
  cfstSettingsSchema,
  defaultCfstSettings,
  storedCfstSettingsSchema,
  toProbeSettings,
  toSavedCfstSettings,
} from './cfst-settings-schema'
import type { CfstSettingsDraft } from './cfst-settings-schema'

type SubmitProbe = {
  start: (settings: Record<string, string>) => Promise<void>
  invalid: () => void
}

type StorageStatus =
  | { state: 'loading' | 'ready' | 'saving' }
  | { state: 'load-error' | 'save-error'; message: string }

export function useCfstSettingsForm(desktop: boolean) {
  const defaultValues: CfstSettingsDraft = defaultCfstSettings
  const form = useForm({
    defaultValues,
    onSubmitMeta: undefined as SubmitProbe | undefined,
    validationLogic: revalidateLogic({ mode: 'change', modeAfterSubmission: 'change' }),
    validators: { onDynamic: cfstSettingsSchema },
    onSubmit: ({ value, meta }) => meta.start(toProbeSettings(value)),
    onSubmitInvalid: ({ meta }) => meta.invalid(),
  })
  const [status, setStatus] = useState<StorageStatus>({ state: desktop ? 'loading' : 'ready' })
  const [loadAttempt, setLoadAttempt] = useState(0)
  const saved = useRef<string | null>(null)
  const snapshot = useStore(form.store, (state) =>
    desktop ? toSavedCfstSettings(state.values) : null,
  )

  useEffect(() => {
    if (!desktop) return
    let cancelled = false
    void invoke<unknown>('load_cfst_settings')
      .then((stored) => {
        if (cancelled) return
        const parsed = storedCfstSettingsSchema.safeParse(stored ?? {})
        if (!parsed.success) throw new Error('config.json 中的 CFST 参数无效')
        saved.current = toSavedCfstSettings(parsed.data)
        form.reset(parsed.data, { keepDefaultValues: true })
        setStatus({ state: 'ready' })
      })
      .catch((err: unknown) => {
        if (!cancelled) setStatus({ state: 'load-error', message: String(err) })
      })
    return () => {
      cancelled = true
    }
  }, [desktop, form, loadAttempt])

  useEffect(() => {
    if (status.state !== 'ready' || snapshot === null || snapshot === saved.current) return
    // Keep writes serial; after completion the effect sees the latest form snapshot.
    setStatus({ state: 'saving' })
    void invoke('save_cfst_settings', { settings: snapshot }).then(
      () => {
        saved.current = snapshot
        setStatus({ state: 'ready' })
      },
      (err: unknown) => setStatus({ state: 'save-error', message: String(err) }),
    )
  }, [snapshot, status.state])

  function retry() {
    if (status.state === 'load-error') {
      setStatus({ state: 'loading' })
      setLoadAttempt((attempt) => attempt + 1)
    } else if (status.state === 'save-error' && snapshot !== null) {
      setStatus({ state: 'ready' })
    }
  }

  return { form, status, retry, invalidDraft: desktop && snapshot === null }
}

export type CfstSettingsController = ReturnType<typeof useCfstSettingsForm>
export type CfstSettingsForm = CfstSettingsController['form']
