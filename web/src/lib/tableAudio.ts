export type TableAudioCue =
  | 'cardDeal'
  | 'check'
  | 'fold'
  | 'wager'
  | 'allIn'

export interface TableAudioController {
  playCue(cue: TableAudioCue): void
  dispose(): void
}

type AudioLike = {
  currentTime: number
  muted: boolean
  preload: string
  volume: number
  src: string
  play(): Promise<unknown> | undefined
  pause(): void
}

type AudioConstructor = new (src?: string) => AudioLike

type HostWithAudio = typeof globalThis & {
  Audio?: AudioConstructor
}

const CUE_CONFIG: Record<TableAudioCue, { path: string; volume: number }> = {
  cardDeal: {
    path: 'audio/card-turn-alt.mp3',
    volume: 0.42,
  },
  check: {
    path: 'audio/button-press.mp3',
    volume: 0.36,
  },
  fold: {
    path: 'audio/card-turn.mp3',
    volume: 0.4,
  },
  wager: {
    path: 'audio/bet.mp3',
    volume: 0.48,
  },
  allIn: {
    path: 'audio/bet-max.mp3',
    volume: 0.52,
  },
}

export function cueForActionLabel(label: string | null): TableAudioCue | null {
  if (!label) {
    return null
  }

  const normalized = label.trim().toLowerCase()
  if (normalized.startsWith('fold')) {
    return 'fold'
  }
  if (normalized.startsWith('check')) {
    return 'check'
  }
  if (normalized.includes('all-in') || normalized.startsWith('moves all-in')) {
    return 'allIn'
  }
  if (
    normalized.startsWith('call') ||
    normalized.startsWith('bet') ||
    normalized.startsWith('raise')
  ) {
    return 'wager'
  }

  return null
}

export function createTableAudio(host: HostWithAudio = globalThis): TableAudioController {
  const players = new Map<TableAudioCue, AudioLike>()

  function ensurePlayer(cue: TableAudioCue): AudioLike | null {
    const existing = players.get(cue)
    if (existing) {
      return existing
    }

    if (typeof host.Audio !== 'function') {
      return null
    }

    const { path, volume } = CUE_CONFIG[cue]
    const player = new host.Audio(resolvePublicAsset(path))
    player.preload = 'auto'
    player.volume = volume
    players.set(cue, player)
    return player
  }

  return {
    playCue(cue) {
      const player = ensurePlayer(cue)
      if (!player) {
        return
      }

      try {
        player.currentTime = 0
      } catch {
        // Some browsers reject rewinding media that has not loaded yet.
      }

      const result = player.play()
      if (result && typeof result.catch === 'function') {
        void result.catch(() => {})
      }
    },
    dispose() {
      for (const player of players.values()) {
        player.pause()
        player.src = ''
      }
      players.clear()
    },
  }
}

function resolvePublicAsset(path: string): string {
  const base = import.meta.env.BASE_URL ?? '/'
  if (base.endsWith('/')) {
    return `${base}${path}`
  }

  return `${base}/${path}`
}
