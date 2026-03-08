import type {
  PokerWorkerRequest,
  PokerWorkerResponse,
  WebSessionConfig,
  WebSessionSnapshot,
} from '../lib/pokerTypes'
import { BOT_MIN_THINK_MS } from '../lib/timing'

export interface PokerSessionLike {
  snapshot(): unknown
  applyHumanAction(actionId: string): unknown
  advanceBot(): unknown
  resetHand(): unknown
}

interface PokerWorkerRuntimeOptions {
  initWasm: () => Promise<unknown>
  createSession: (config: WebSessionConfig) => PokerSessionLike
}

export class PokerWorkerRuntime {
  private readonly options: PokerWorkerRuntimeOptions
  private readonly wasmReady: Promise<unknown>
  private session: PokerSessionLike | null = null

  constructor(options: PokerWorkerRuntimeOptions) {
    this.options = options
    this.wasmReady = options.initWasm()
  }

  async handle(message: PokerWorkerRequest): Promise<PokerWorkerResponse> {
    try {
      if (message.type === 'init' && message.forceInitError) {
        throw new Error(message.forceInitError)
      }

      await this.wasmReady

      let snapshot: WebSessionSnapshot

      switch (message.type) {
        case 'init':
          this.session = this.options.createSession(message.config)
          snapshot = asSnapshot(this.session.snapshot())
          break
        case 'snapshot':
          snapshot = asSnapshot(this.requireSession().snapshot())
          break
        case 'applyHumanAction':
          snapshot = asSnapshot(
            this.requireSession().applyHumanAction(message.actionId),
          )
          break
        case 'advanceBot': {
          const thinkStartedAt = nowMs()
          snapshot = asSnapshot(this.requireSession().advanceBot())
          await waitForMinimumThink(
            thinkStartedAt,
            message.forceActionDelayMs,
          )
          break
        }
        case 'resetHand':
          snapshot = asSnapshot(this.requireSession().resetHand())
          break
        default:
          throw new Error('Unknown poker worker command')
      }

      return { id: message.id, ok: true, snapshot }
    } catch (error) {
      return {
        id: message.id,
        ok: false,
        error: error instanceof Error ? error.message : 'Unknown poker worker error',
      }
    }
  }

  private requireSession(): PokerSessionLike {
    if (!this.session) {
      throw new Error('Poker session is not initialized')
    }
    return this.session
  }
}

function asSnapshot(value: unknown): WebSessionSnapshot {
  return value as WebSessionSnapshot
}

async function maybeDelay(delayMs: number | null | undefined): Promise<void> {
  if (!delayMs || delayMs <= 0) {
    return
  }

  await new Promise((resolve) => globalThis.setTimeout(resolve, delayMs))
}

async function waitForMinimumThink(
  startedAtMs: number,
  forcedDelayMs: number | null | undefined,
): Promise<void> {
  const minimumDelayMs = Math.max(BOT_MIN_THINK_MS, forcedDelayMs ?? 0)
  const remainingMs = minimumDelayMs - (nowMs() - startedAtMs)
  await maybeDelay(remainingMs)
}

function nowMs(): number {
  if (typeof globalThis.performance?.now === 'function') {
    return globalThis.performance.now()
  }

  return Date.now()
}
