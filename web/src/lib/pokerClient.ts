import type {
  PokerWorkerCommand,
  PokerWorkerRequest,
  PokerWorkerResponse,
  WebActionChoice,
  WebSessionConfig,
  WebSessionSnapshot,
} from './pokerTypes'

type PendingRequest = {
  resolve: (snapshot: WebSessionSnapshot) => void
  reject: (error: Error) => void
}

interface PokerClientBackend {
  init(config: WebSessionConfig): Promise<WebSessionSnapshot>
  snapshot(): Promise<WebSessionSnapshot>
  applyHumanAction(actionId: string): Promise<WebSessionSnapshot>
  advanceBot(): Promise<WebSessionSnapshot>
  resetHand(): Promise<WebSessionSnapshot>
  dispose(): void
}

type TestScenario =
  | 'flopRevealThenAction'
  | 'matchOverHeroWin'
  | 'matchOverBotWin'
  | 'initError'

export class PokerClient {
  private readonly backend: PokerClientBackend

  constructor() {
    this.backend = createBackend()
  }

  async init(config: WebSessionConfig): Promise<WebSessionSnapshot> {
    return this.backend.init(config)
  }

  async snapshot(): Promise<WebSessionSnapshot> {
    return this.backend.snapshot()
  }

  async applyHumanAction(actionId: string): Promise<WebSessionSnapshot> {
    return this.backend.applyHumanAction(actionId)
  }

  async advanceBot(): Promise<WebSessionSnapshot> {
    return this.backend.advanceBot()
  }

  async resetHand(): Promise<WebSessionSnapshot> {
    return this.backend.resetHand()
  }

  dispose(): void {
    this.backend.dispose()
  }
}

class WorkerPokerClientBackend implements PokerClientBackend {
  private readonly worker: Worker
  private readonly pending = new Map<number, PendingRequest>()
  private nextId = 1

  constructor() {
    this.worker = new Worker(
      new URL('../workers/pokerWorker.ts', import.meta.url),
      { type: 'module' },
    )
    this.worker.addEventListener('message', this.handleMessage)
    this.worker.addEventListener('error', this.handleWorkerError)
  }

  async init(config: WebSessionConfig): Promise<WebSessionSnapshot> {
    return this.send({
      type: 'init',
      config,
      forceInitError: readForcedWorkerInitError(),
    })
  }

  async snapshot(): Promise<WebSessionSnapshot> {
    return this.send({ type: 'snapshot' })
  }

  async applyHumanAction(actionId: string): Promise<WebSessionSnapshot> {
    return this.send({
      type: 'applyHumanAction',
      actionId,
    })
  }

  async advanceBot(): Promise<WebSessionSnapshot> {
    return this.send({
      type: 'advanceBot',
      forceActionDelayMs: readForcedWorkerActionDelay(),
    })
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

class ScenarioPokerClientBackend implements PokerClientBackend {
  private readonly scenario: TestScenario

  constructor(scenario: TestScenario) {
    this.scenario = scenario
  }

  private config: WebSessionConfig | null = null
  private handNumber = 1
  private stage: 'opening' | 'afterHuman' | 'afterBot' | 'terminal' = 'opening'

  async init(config: WebSessionConfig): Promise<WebSessionSnapshot> {
    if (this.scenario === 'initError') {
      throw new Error('forced initialization failure for e2e')
    }

    this.config = config
    this.handNumber = 1
    this.stage = 'opening'
    return this.currentSnapshot()
  }

  async snapshot(): Promise<WebSessionSnapshot> {
    return this.currentSnapshot()
  }

  async applyHumanAction(_actionId: string): Promise<WebSessionSnapshot> {
    this.assertInitialized()

    if (this.scenario === 'matchOverHeroWin' && this.stage === 'opening') {
      this.stage = 'terminal'
      return this.currentSnapshot()
    }

    if (this.scenario === 'matchOverBotWin' && this.stage === 'opening') {
      this.stage = 'afterHuman'
      return this.currentSnapshot()
    }

    if (this.stage === 'opening') {
      this.stage = 'afterHuman'
      return this.currentSnapshot()
    }

    if (this.stage === 'afterBot') {
      this.stage = 'terminal'
      return this.currentSnapshot()
    }

    throw new Error(`Test scenario does not support human action during ${this.stage}`)
  }

  async advanceBot(): Promise<WebSessionSnapshot> {
    this.assertInitialized()
    if (this.stage !== 'afterHuman') {
      throw new Error(`Test scenario does not support bot action during ${this.stage}`)
    }

    const forcedDelayMs = readForcedWorkerActionDelay()
    if (forcedDelayMs) {
      await sleep(forcedDelayMs)
    }

    this.stage = this.scenario === 'matchOverBotWin' ? 'terminal' : 'afterBot'
    return this.currentSnapshot()
  }

  async resetHand(): Promise<WebSessionSnapshot> {
    this.assertInitialized()
    this.handNumber += 1
    this.stage = 'opening'
    return this.currentSnapshot()
  }

  dispose(): void {}

  private currentSnapshot(): WebSessionSnapshot {
    const config = this.assertInitialized()
    switch (this.scenario) {
      case 'flopRevealThenAction':
        switch (this.stage) {
          case 'opening':
            return buildScenarioOpeningSnapshot(config, this.handNumber)
          case 'afterHuman':
            return buildScenarioAfterHumanSnapshot(config, this.handNumber)
          case 'afterBot':
            return buildScenarioAfterBotSnapshot(config, this.handNumber)
          case 'terminal':
            return buildScenarioTerminalSnapshot(config, this.handNumber)
        }
      case 'matchOverHeroWin':
        switch (this.stage) {
          case 'opening':
            return buildHeroMatchScenarioOpeningSnapshot(config, this.handNumber)
          case 'terminal':
            return buildHeroMatchScenarioTerminalSnapshot(config, this.handNumber)
          default:
            throw new Error(`Test scenario does not support stage ${this.stage}`)
        }
      case 'matchOverBotWin':
        switch (this.stage) {
          case 'opening':
            return buildBotMatchScenarioOpeningSnapshot(config, this.handNumber)
          case 'afterHuman':
            return buildBotMatchScenarioAfterHumanSnapshot(config, this.handNumber)
          case 'terminal':
            return buildBotMatchScenarioTerminalSnapshot(config, this.handNumber)
          default:
            throw new Error(`Test scenario does not support stage ${this.stage}`)
        }
      case 'initError':
        throw new Error('Init-error test scenario does not produce snapshots')
    }
  }

  private assertInitialized(): WebSessionConfig {
    if (!this.config) {
      throw new Error('Test scenario client must be initialized before use')
    }
    return this.config
  }
}

function createBackend(): PokerClientBackend {
  const scenario = readForcedTestScenario()
  return scenario ? new ScenarioPokerClientBackend(scenario) : new WorkerPokerClientBackend()
}

function readForcedWorkerInitError(): string | null {
  const host = globalThis as typeof globalThis & {
    __GTO_FORCE_WORKER_ERROR__?: string
  }
  const value = host.__GTO_FORCE_WORKER_ERROR__
  if (typeof value !== 'string' || value.length === 0) {
    return null
  }
  delete host.__GTO_FORCE_WORKER_ERROR__
  return value
}

function readForcedWorkerActionDelay(): number | null {
  const host = globalThis as typeof globalThis & {
    __GTO_FORCE_ACTION_DELAY_MS__?: number | string
  }
  const value = host.__GTO_FORCE_ACTION_DELAY_MS__
  if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
    delete host.__GTO_FORCE_ACTION_DELAY_MS__
    return value
  }

  if (typeof value === 'string') {
    const parsed = Number.parseInt(value, 10)
    if (Number.isFinite(parsed) && parsed >= 0) {
      delete host.__GTO_FORCE_ACTION_DELAY_MS__
      return parsed
    }
  }

  return null
}

function readForcedTestScenario(): TestScenario | null {
  const host = globalThis as typeof globalThis & {
    __GTO_TEST_SCENARIO__?: string
  }
  switch (host.__GTO_TEST_SCENARIO__) {
    case 'flopRevealThenAction':
    case 'matchOverHeroWin':
    case 'matchOverBotWin':
    case 'initError':
      return host.__GTO_TEST_SCENARIO__
    default:
      return null
  }
}

function buildScenarioOpeningSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'preflop',
    currentActor: 'button',
    pot: 150,
    boardCards: [],
    buttonStack: 9950,
    bigBlindStack: 9900,
    buttonContribution: 50,
    bigBlindContribution: 100,
    legalActions: openingActions(),
    history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
    status: 'Your turn on preflop.',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildScenarioAfterHumanSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'flop',
    currentActor: 'bigBlind',
    pot: 200,
    boardCards: ['Jc', '4h', '8c'],
    buttonStack: 9900,
    bigBlindStack: 9900,
    buttonContribution: 100,
    bigBlindContribution: 100,
    legalActions: [],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button calls',
      'preflop: big-blind checks',
      'flop: Jc 4h 8c',
    ],
    status: 'Bot to act on flop (big-blind).',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildScenarioAfterBotSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'flop',
    currentActor: 'button',
    pot: 390,
    boardCards: ['Jc', '4h', '8c'],
    buttonStack: 9900,
    bigBlindStack: 9810,
    buttonContribution: 100,
    bigBlindContribution: 190,
    legalActions: [
      { id: 'fold', label: 'Fold' },
      { id: 'call', label: 'Call' },
      { id: 'raiseTo:490', label: 'Raise to 4.9 bb' },
    ],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button calls',
      'preflop: big-blind checks',
      'flop: Jc 4h 8c',
      'flop: big-blind bets to 1.9 bb',
    ],
    status: 'Your turn on flop.',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildScenarioTerminalSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'river',
    currentActor: null,
    pot: 780,
    boardCards: ['Jc', '4h', '8c', '3d', '9s'],
    buttonStack: 9610,
    bigBlindStack: 9610,
    buttonContribution: 390,
    bigBlindContribution: 390,
    legalActions: [],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button calls',
      'preflop: big-blind checks',
      'flop: Jc 4h 8c',
      'flop: big-blind bets to 1.9 bb',
      'flop: button calls',
      'turn: 3d',
      'turn: big-blind checks',
      'turn: button checks',
      'river: 9s',
      'river: big-blind checks',
      'river: button checks',
      'button wins at showdown for 7.8 bb',
    ],
    status: 'Hand complete.',
    terminalSummary: 'button wins at showdown for 7.8 bb',
    botHoleCards: ['As', '7s'],
    matchOver: false,
  })
}

function buildHeroMatchScenarioOpeningSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'preflop',
    currentActor: 'button',
    pot: 150,
    boardCards: [],
    buttonStack: 9_950,
    bigBlindStack: 9_900,
    buttonContribution: 50,
    bigBlindContribution: 100,
    legalActions: [{ id: 'call', label: 'Call' }],
    history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
    status: 'Your turn on preflop.',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildHeroMatchScenarioTerminalSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'preflop',
    currentActor: null,
    pot: 20_000,
    boardCards: [],
    buttonStack: 20_000,
    bigBlindStack: 0,
    buttonContribution: 10_000,
    bigBlindContribution: 10_000,
    legalActions: [],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button moves all-in to 100.0 bb',
      'preflop: big-blind calls',
      'button wins at showdown for 200.0 bb',
    ],
    status: 'Match over.',
    terminalSummary: 'button wins at showdown for 200.0 bb',
    botHoleCards: ['Qc', 'Qd'],
    matchOver: true,
  })
}

function buildBotMatchScenarioOpeningSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'preflop',
    currentActor: 'button',
    pot: 150,
    boardCards: [],
    buttonStack: 9_950,
    bigBlindStack: 9_900,
    buttonContribution: 50,
    bigBlindContribution: 100,
    legalActions: [{ id: 'call', label: 'Call' }],
    history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
    status: 'Your turn on preflop.',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildBotMatchScenarioAfterHumanSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'flop',
    currentActor: 'bigBlind',
    pot: 20_000,
    boardCards: ['Jh', '7c', '2d'],
    buttonStack: 0,
    bigBlindStack: 0,
    buttonContribution: 10_000,
    bigBlindContribution: 10_000,
    legalActions: [],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button calls',
      'preflop: big-blind checks',
      'flop: Jh 7c 2d',
    ],
    status: 'Bot to act on flop (big-blind).',
    terminalSummary: null,
    botHoleCards: [],
    matchOver: false,
  })
}

function buildBotMatchScenarioTerminalSnapshot(
  config: WebSessionConfig,
  handNumber: number,
): WebSessionSnapshot {
  return buildScenarioSnapshot(config, handNumber, {
    street: 'flop',
    currentActor: null,
    pot: 20_000,
    boardCards: ['Jh', '7c', '2d'],
    buttonStack: 0,
    bigBlindStack: 20_000,
    buttonContribution: 10_000,
    bigBlindContribution: 10_000,
    legalActions: [],
    history: [
      'button posts 0.5 bb',
      'big-blind posts 1.0 bb',
      'preflop: button calls',
      'preflop: big-blind checks',
      'flop: Jh 7c 2d',
      'flop: big-blind moves all-in to 100.0 bb',
      'big-blind wins uncontested for 200.0 bb',
    ],
    status: 'Match over.',
    terminalSummary: 'big-blind wins uncontested for 200.0 bb',
    botHoleCards: ['Qc', 'Qd'],
    matchOver: true,
  })
}

function buildScenarioSnapshot(
  config: WebSessionConfig,
  handNumber: number,
  state: {
    street: string
    currentActor: 'button' | 'bigBlind' | null
    pot: number
    boardCards: string[]
    buttonStack: number
    bigBlindStack: number
    buttonContribution: number
    bigBlindContribution: number
    legalActions: WebActionChoice[]
    history: string[]
    status: string
    terminalSummary: string | null
    botHoleCards: string[]
    matchOver: boolean
  },
): WebSessionSnapshot {
  return {
    handNumber,
    humanSeat: 'button',
    botSeat: 'bigBlind',
    botMode: config.botMode,
    matchOver: state.matchOver,
    street: state.street,
    phase: state.terminalSummary ? 'terminal' : 'bettingRound',
    currentActor: state.currentActor,
    pot: state.pot,
    boardCards: state.boardCards,
    button: {
      seat: 'button',
      stack: state.buttonStack,
      totalContribution: state.buttonContribution,
      streetContribution: state.street === 'preflop' ? state.buttonContribution : 0,
      folded: false,
      holeCards: ['Qh', '4d'],
    },
    bigBlind: {
      seat: 'bigBlind',
      stack: state.bigBlindStack,
      totalContribution: state.bigBlindContribution,
      streetContribution: state.street === 'preflop' ? state.bigBlindContribution : 0,
      folded: false,
      holeCards: state.botHoleCards,
    },
    legalActions: state.legalActions,
    history: state.history,
    status: state.status,
    terminalSummary: state.terminalSummary,
  }
}

function openingActions(): WebActionChoice[] {
  return [
    { id: 'fold', label: 'Fold' },
    { id: 'call', label: 'Call' },
    { id: 'raiseTo:250', label: 'Raise to 2.5 bb' },
    { id: 'raiseTo:400', label: 'Raise to 4.0 bb' },
    { id: 'raiseTo:700', label: 'Raise to 7.0 bb' },
    { id: 'allIn:10000', label: 'All-in to 100.0 bb' },
  ]
}

async function sleep(durationMs: number): Promise<void> {
  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, durationMs)
  })
}
