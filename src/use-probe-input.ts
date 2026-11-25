import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import { bundledIpLibraries } from '@/bundled-ip-libraries'

const inputs = {
  ips: {
    label: 'IP 列表',
    fallback: bundledIpLibraries.flatMap(({ entries }) => entries).join('\n'),
    loadCommand: 'load_ip_list',
    saveCommand: 'save_ip_list',
  },
  nodes: {
    label: 'VLESS 节点',
    fallback: '',
    loadCommand: 'load_vless_nodes',
    saveCommand: 'save_vless_nodes',
  },
}

type StorageStatus = {
  state: 'loading' | 'ready' | 'load-error' | 'save-error'
  message: string
}

export function useProbeInput(desktop: boolean, input: keyof typeof inputs) {
  const { label, fallback, loadCommand, saveCommand } = inputs[input]
  const loading: StorageStatus = { state: 'loading', message: `正在读取本地${label}…` }
  const [value, setValue] = useState(desktop ? '' : fallback)
  const draft = useRef(value)
  const saving = useRef(false)
  const [loadAttempt, setLoadAttempt] = useState(0)
  const [status, setStatus] = useState<StorageStatus>(
    desktop ? loading : { state: 'ready', message: '' },
  )

  useEffect(() => {
    if (!desktop) return
    let cancelled = false
    void invoke<string | null>(loadCommand).then(
      (stored) => {
        if (cancelled) return
        draft.current = stored ?? fallback
        setValue(draft.current)
        setStatus({ state: 'ready', message: stored === null ? '' : `已载入本地${label}。` })
      },
      (err: unknown) => {
        if (!cancelled) setStatus({ state: 'load-error', message: `读取失败：${String(err)}` })
      },
    )
    return () => {
      cancelled = true
    }
  }, [desktop, fallback, label, loadAttempt, loadCommand])

  async function save() {
    if (!desktop || saving.current) return
    saving.current = true
    setStatus({ state: 'ready', message: `正在保存${label}…` })
    // Only one write at a time; edits during a write only need the latest snapshot.
    let snapshot: string
    do {
      snapshot = draft.current
      try {
        await invoke(saveCommand, { [input]: snapshot })
        if (snapshot === draft.current) {
          setStatus({ state: 'ready', message: `${label}已保存到本地。` })
        }
      } catch (err) {
        if (snapshot === draft.current) {
          setStatus({ state: 'save-error', message: `保存失败：${String(err)}` })
        }
      }
    } while (snapshot !== draft.current)
    saving.current = false
  }

  function edit(change: string | ((current: string) => string)) {
    const next = typeof change === 'string' ? change : change(draft.current)
    if (next === draft.current) return
    draft.current = next
    setValue(next)
    void save()
  }

  function retry() {
    if (status.state === 'load-error') {
      setStatus(loading)
      setLoadAttempt((attempt) => attempt + 1)
    } else if (status.state === 'save-error') {
      void save()
    }
  }

  const blocked = status.state === 'loading' || status.state === 'load-error'
  return { value, edit, status, retry, blocked }
}
