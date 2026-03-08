import type {
  PokerWorkerCommand,
  PokerWorkerRequest,
  PokerWorkerResponse,
  WebSessionConfig,
  WebSessionSnapshot,
} from './pokerTypes'

type PendingRequest = {
  resolve: (snapshot: WebSessionSnapshot) => void
  reject: (error: Error) => void
}

export class PokerClient {
  private readonly worker: Worker
  private readonly pending = new Map<number, PendingRequest>()
  private nextId = 1

  constructor() {
    this.worker = new Worker(createWorkerUrl(), { type: 'module' })
    this.worker.addEventListener('message', this.handleMessage)
    this.worker.addEventListener('error', this.handleWorkerError)
  }

  async init(config: WebSessionConfig): Promise<WebSessionSnapshot> {
    return this.send({ type: 'init', config })
  }

  async snapshot(): Promise<WebSessionSnapshot> {
    return this.send({ type: 'snapshot' })
  }

  async applyHumanAction(actionId: string): Promise<WebSessionSnapshot> {
    return this.send({ type: 'applyHumanAction', actionId })
  }

  async resetHand(): Promise<WebSessionSnapshot> {
    return this.send({ type: 'resetHand' })
  }

  dispose(): void {
    this.worker.removeEventListener('message', this.handleMessage)
    this.worker.removeEventListener('error', this.handleWorkerError)
    this.worker.terminate()

    for (const request of this.pending.values()) {
      request.reject(new Error('Poker worker was disposed before replying'))
    }
    this.pending.clear()
  }

  private async send(
    message: PokerWorkerCommand,
  ): Promise<WebSessionSnapshot> {
    const id = this.nextId++

    return new Promise<WebSessionSnapshot>((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      this.worker.postMessage({ id, ...message } as PokerWorkerRequest)
    })
  }

  private handleMessage = (event: MessageEvent<PokerWorkerResponse>): void => {
    const message = event.data
    const request = this.pending.get(message.id)
    if (!request) {
      return
    }

    this.pending.delete(message.id)
    if (message.ok) {
      request.resolve(message.snapshot)
      return
    }

    request.reject(new Error(message.error))
  }

  private handleWorkerError = (): void => {
    for (const request of this.pending.values()) {
      request.reject(new Error('Poker worker crashed'))
    }
    this.pending.clear()
  }
}

function createWorkerUrl(): URL {
  const workerUrl = new URL('../workers/pokerWorker.ts', import.meta.url)
  const forcedInitError = readForcedWorkerInitError()
  if (forcedInitError) {
    workerUrl.searchParams.set('forceInitError', forcedInitError)
  }
  return workerUrl
}

function readForcedWorkerInitError(): string | null {
  const host = globalThis as typeof globalThis & {
    __GTO_FORCE_WORKER_ERROR__?: string
  }
  const value = host.__GTO_FORCE_WORKER_ERROR__
  if (typeof value !== 'string' || value.length === 0) {
    return null
  }
  return value
}
