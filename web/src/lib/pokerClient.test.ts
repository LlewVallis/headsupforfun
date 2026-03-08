import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { PokerWorkerRequest, WebSessionSnapshot } from './pokerTypes'
import { PokerClient } from './pokerClient'

const mockSnapshot: WebSessionSnapshot = {
  handNumber: 1,
  humanSeat: 'button',
  botSeat: 'bigBlind',
  botMode: 'hybridFast',
  street: 'preflop',
  phase: 'bettingRound',
  currentActor: 'button',
  pot: 150,
  boardCards: [],
  button: {
    seat: 'button',
    stack: 9950,
    totalContribution: 50,
    streetContribution: 50,
    folded: false,
    holeCards: ['As', 'Kd'],
  },
  bigBlind: {
    seat: 'bigBlind',
    stack: 9900,
    totalContribution: 100,
    streetContribution: 100,
    folded: false,
    holeCards: [],
  },
  legalActions: [{ id: 'call', label: 'Call' }],
  history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
  status: 'Your turn on preflop.',
  terminalSummary: null,
}

type WorkerListenerMap = {
  message: Array<(event: MessageEvent) => void>
  error: Array<(event: Event) => void>
}

class FakeWorker {
  static instances: FakeWorker[] = []

  readonly listeners: WorkerListenerMap = { message: [], error: [] }
  readonly terminate = vi.fn()
  readonly postedMessages: PokerWorkerRequest[] = []

  constructor() {
    FakeWorker.instances.push(this)
  }

  addEventListener(type: 'message', listener: (event: MessageEvent) => void): void
  addEventListener(type: 'error', listener: (event: Event) => void): void
  addEventListener(
    type: keyof WorkerListenerMap,
    listener: ((event: MessageEvent) => void) | ((event: Event) => void),
  ): void {
    if (type === 'message') {
      this.listeners.message.push(listener as (event: MessageEvent) => void)
      return
    }

    this.listeners.error.push(listener as (event: Event) => void)
  }

  removeEventListener(
    type: 'message',
    listener: (event: MessageEvent) => void,
  ): void
  removeEventListener(type: 'error', listener: (event: Event) => void): void
  removeEventListener(
    type: keyof WorkerListenerMap,
    listener: ((event: MessageEvent) => void) | ((event: Event) => void),
  ): void {
    if (type === 'message') {
      this.listeners.message = this.listeners.message.filter(
        (entry) => entry !== listener,
      )
      return
    }

    this.listeners.error = this.listeners.error.filter(
      (entry) => entry !== listener,
    )
  }

  postMessage(message: PokerWorkerRequest): void {
    this.postedMessages.push(message)
  }

  emitMessage(data: unknown): void {
    for (const listener of this.listeners.message) {
      listener({ data } as MessageEvent)
    }
  }

  emitError(): void {
    for (const listener of this.listeners.error) {
      listener(new Event('error'))
    }
  }
}

describe('PokerClient', () => {
  const originalWorker = globalThis.Worker

  beforeEach(() => {
    FakeWorker.instances = []
    globalThis.Worker = FakeWorker as unknown as typeof Worker
  })

  afterEach(() => {
    globalThis.Worker = originalWorker
  })

  it('sends init requests and resolves snapshots from worker replies', async () => {
    const client = new PokerClient()
    const worker = expectWorker()
    const initPromise = client.init({
      seed: 7,
      humanSeat: 'button',
      botMode: 'hybridFast',
    })

    expect(worker.postedMessages).toHaveLength(1)
    expect(worker.postedMessages[0]).toMatchObject({
      id: 1,
      type: 'init',
      config: {
        seed: 7,
        humanSeat: 'button',
        botMode: 'hybridFast',
      },
    })

    worker.emitMessage({ id: 1, ok: true, snapshot: mockSnapshot })

    await expect(initPromise).resolves.toEqual(mockSnapshot)
    client.dispose()
  })

  it('forwards a forced action delay override for the next human action', async () => {
    ;(globalThis as typeof globalThis & { __GTO_FORCE_ACTION_DELAY_MS__?: number }).__GTO_FORCE_ACTION_DELAY_MS__ =
      180
    const client = new PokerClient()
    const worker = expectWorker()
    const pending = client.applyHumanAction('call')

    expect(worker.postedMessages[0]).toMatchObject({
      id: 1,
      type: 'applyHumanAction',
      actionId: 'call',
      forceActionDelayMs: 180,
    })

    worker.emitMessage({ id: 1, ok: true, snapshot: mockSnapshot })
    await expect(pending).resolves.toEqual(mockSnapshot)
    expect(
      (globalThis as typeof globalThis & { __GTO_FORCE_ACTION_DELAY_MS__?: number })
        .__GTO_FORCE_ACTION_DELAY_MS__,
    ).toBeUndefined()
    client.dispose()
  })

  it('rejects requests when the worker replies with an initialization failure', async () => {
    const client = new PokerClient()
    const worker = expectWorker()
    const initPromise = client.init({
      seed: 7,
      humanSeat: 'button',
      botMode: 'blueprint',
    })

    worker.emitMessage({
      id: 1,
      ok: false,
      error: 'WASM initialization failed',
    })

    await expect(initPromise).rejects.toThrow('WASM initialization failed')
    client.dispose()
  })

  it('rejects pending requests when the worker crashes', async () => {
    const client = new PokerClient()
    const worker = expectWorker()
    const pending = client.snapshot()

    worker.emitError()

    await expect(pending).rejects.toThrow('Poker worker crashed')
    client.dispose()
  })

  it('rejects pending requests when the client is disposed', async () => {
    const client = new PokerClient()
    const pending = client.resetHand()

    client.dispose()

    await expect(pending).rejects.toThrow(
      'Poker worker was disposed before replying',
    )
  })
})

function expectWorker(): FakeWorker {
  expect(FakeWorker.instances).toHaveLength(1)
  return FakeWorker.instances[0]
}
