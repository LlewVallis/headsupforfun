import { describe, expect, it, vi } from 'vitest'

import type { WebSessionConfig, WebSessionSnapshot } from '../lib/pokerTypes'
import { PokerWorkerRuntime } from './pokerWorkerRuntime'

const mockConfig: WebSessionConfig = {
  seed: 7,
  humanSeat: 'button',
  botMode: 'hybridFast',
}

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

describe('PokerWorkerRuntime', () => {
  it('creates a session and returns the initial snapshot', async () => {
    const createSession = vi.fn(() => ({
      snapshot: vi.fn(() => mockSnapshot),
      applyHumanAction: vi.fn(),
      advanceBot: vi.fn(),
      resetHand: vi.fn(),
    }))
    const runtime = new PokerWorkerRuntime({
      initWasm: vi.fn(async () => undefined),
      createSession,
    })

    const response = await runtime.handle({
      id: 1,
      type: 'init',
      config: mockConfig,
    })

    expect(response).toEqual({ id: 1, ok: true, snapshot: mockSnapshot })
    expect(createSession).toHaveBeenCalledWith(mockConfig)
  })

  it('returns an error when commands arrive before initialization', async () => {
    const runtime = new PokerWorkerRuntime({
      initWasm: vi.fn(async () => undefined),
      createSession: vi.fn(),
    })

    const response = await runtime.handle({ id: 2, type: 'snapshot' })

    expect(response).toEqual({
      id: 2,
      ok: false,
      error: 'Poker session is not initialized',
    })
  })

  it('passes human actions through to the current session', async () => {
    const applyHumanAction = vi.fn(() => mockSnapshot)
    const runtime = new PokerWorkerRuntime({
      initWasm: vi.fn(async () => undefined),
      createSession: vi.fn(() => ({
        snapshot: vi.fn(() => mockSnapshot),
        applyHumanAction,
        advanceBot: vi.fn(() => mockSnapshot),
        resetHand: vi.fn(() => mockSnapshot),
      })),
    })

    await runtime.handle({ id: 1, type: 'init', config: mockConfig })
    const response = await runtime.handle({
      id: 2,
      type: 'applyHumanAction',
      actionId: 'call',
    })

    expect(applyHumanAction).toHaveBeenCalledWith('call')
    expect(response).toEqual({ id: 2, ok: true, snapshot: mockSnapshot })
  })

  it('waits for a forced bot-action delay when requested by the test harness', async () => {
    vi.useFakeTimers()
    const advanceBot = vi.fn(() => mockSnapshot)
    const runtime = new PokerWorkerRuntime({
      initWasm: vi.fn(async () => undefined),
      createSession: vi.fn(() => ({
        snapshot: vi.fn(() => mockSnapshot),
        applyHumanAction: vi.fn(() => mockSnapshot),
        advanceBot,
        resetHand: vi.fn(() => mockSnapshot),
      })),
    })

    await runtime.handle({ id: 1, type: 'init', config: mockConfig })
    const pending = runtime.handle({
      id: 2,
      type: 'advanceBot',
      forceActionDelayMs: 120,
    })

    await vi.advanceTimersByTimeAsync(100)
    expect(advanceBot).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(20)
    await expect(pending).resolves.toEqual({ id: 2, ok: true, snapshot: mockSnapshot })
    expect(advanceBot).toHaveBeenCalledTimes(1)
    vi.useRealTimers()
  })

  it('surfaces wasm initialization failures as worker errors', async () => {
    const runtime = new PokerWorkerRuntime({
      initWasm: vi.fn(async () => {
        throw new Error('failed to load wasm package')
      }),
      createSession: vi.fn(),
    })

    const response = await runtime.handle({
      id: 3,
      type: 'init',
      config: mockConfig,
    })

    expect(response).toEqual({
      id: 3,
      ok: false,
      error: 'failed to load wasm package',
    })
  })
})
