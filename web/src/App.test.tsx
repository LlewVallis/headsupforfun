import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { BOT_ACTION_BUBBLE_MS } from './lib/presentation'
import type { WebSessionSnapshot } from './lib/pokerTypes'

const baseSnapshot: WebSessionSnapshot = {
  handNumber: 1,
  humanSeat: 'button',
  botSeat: 'bigBlind',
  botMode: 'hybridPlay',
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
  legalActions: [
    { id: 'call', label: 'Call' },
    { id: 'raiseTo:400', label: 'Raise to 4.0 bb' },
  ],
  history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
  status: 'Your turn on preflop.',
  terminalSummary: null,
}

const terminalSnapshot: WebSessionSnapshot = {
  ...baseSnapshot,
  currentActor: null,
  legalActions: [],
  terminalSummary: 'button wins at showdown for 6.5 bb',
  bigBlind: {
    ...baseSnapshot.bigBlind,
    holeCards: ['Qc', 'Qd'],
  },
  history: [
    ...baseSnapshot.history,
    'preflop: button calls',
    'preflop: big-blind checks',
    'flop: As Kd 7h',
    'turn: 2c',
    'river: 2d',
    'button wins at showdown for 6.5 bb',
  ],
}

const afterHumanSnapshot: WebSessionSnapshot = {
  ...baseSnapshot,
  street: 'flop',
  currentActor: 'bigBlind',
  legalActions: [],
  pot: 400,
  boardCards: ['Ah', '7d', '2c'],
  history: [
    ...baseSnapshot.history,
    'preflop: button raises to 4.0 bb',
    'preflop: big-blind calls',
    'flop: Ah 7d 2c',
  ],
  status: 'Bot to act on flop (big-blind).',
}

const postActionSnapshot: WebSessionSnapshot = {
  ...afterHumanSnapshot,
  pot: 800,
  history: [
    ...afterHumanSnapshot.history,
    'flop: big-blind bets to 4.0 bb',
  ],
  legalActions: [{ id: 'check', label: 'Check' }],
  currentActor: 'button',
  status: 'Your turn on flop.',
}

const initMock = vi.fn().mockResolvedValue(baseSnapshot)
const resetHandMock = vi.fn().mockResolvedValue(baseSnapshot)
const applyHumanActionMock = vi.fn().mockResolvedValue(afterHumanSnapshot)
const advanceBotMock = vi.fn().mockResolvedValue(postActionSnapshot)
const disposeMock = vi.fn()

vi.mock('./lib/pokerClient', () => ({
  PokerClient: class {
    init = initMock
    resetHand = resetHandMock
    applyHumanAction = applyHumanActionMock
    advanceBot = advanceBotMock
    dispose = disposeMock
  },
}))

import App from './App'

describe('App', () => {
  beforeEach(() => {
    vi.useRealTimers()
    initMock.mockReset()
    initMock.mockResolvedValue(baseSnapshot)
    resetHandMock.mockReset()
    resetHandMock.mockResolvedValue(baseSnapshot)
    applyHumanActionMock.mockReset()
    applyHumanActionMock.mockResolvedValue(afterHumanSnapshot)
    advanceBotMock.mockReset()
    advanceBotMock.mockResolvedValue(postActionSnapshot)
    disposeMock.mockClear()
    ;(globalThis as typeof globalThis & { __GTO_TEST_SEED__?: number }).__GTO_TEST_SEED__ = 7
  })

  afterEach(() => {
    delete (globalThis as typeof globalThis & { __GTO_TEST_SEED__?: number }).__GTO_TEST_SEED__
  })

  it('renders the game-first poker table shell', async () => {
    render(<App />)

    expect(
      screen.getByRole('heading', { name: "Heads-Up Hold'em" }),
    ).toBeInTheDocument()
    expect(await screen.findByRole('button', { name: 'New match' })).toBeInTheDocument()
    expect(await screen.findByLabelText('Poker table')).toBeInTheDocument()
    expect(screen.getByLabelText('Hero panel')).toHaveTextContent('You')
    expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Solver Bot')
    expect(screen.getByLabelText('Action tray')).not.toHaveTextContent('hybrid play mode')
    expect(screen.queryByText('Session activity')).not.toBeInTheDocument()
    expect(screen.queryByText('Seed')).not.toBeInTheDocument()
  })

  it('initializes the table with the fixed hybrid-play bot mode', async () => {
    render(<App />)
    await screen.findByRole('button', { name: 'Call' })

    expect(initMock).toHaveBeenCalledWith({
      seed: 7,
      humanSeat: 'button',
      botMode: 'hybridPlay',
    })
  })

  it('shows a recoverable table-reset banner when initialization fails', async () => {
    initMock.mockRejectedValueOnce(new Error('init failed'))

    render(<App />)

    expect(await screen.findByRole('alert')).toHaveTextContent('Table reset needed')
    expect(screen.getByRole('alert')).toHaveTextContent('init failed')

    const user = userEvent.setup()
    initMock.mockResolvedValueOnce(baseSnapshot)
    await user.click(screen.getByRole('button', { name: 'Reload table' }))

    expect(await screen.findByRole('button', { name: 'Call' })).toBeInTheDocument()
  })

  it('shows the next-hand action when the hand is complete', async () => {
    initMock.mockResolvedValueOnce(terminalSnapshot)

    render(<App />)

    const user = userEvent.setup()
    expect(await screen.findByRole('button', { name: 'Deal next hand' })).toBeInTheDocument()
    expect(
      within(screen.getByLabelText('Bot panel')).getAllByRole('img', { name: /of/i }),
    ).toHaveLength(2)

    await user.click(screen.getByRole('button', { name: 'Deal next hand' }))

    expect(resetHandMock).toHaveBeenCalledTimes(1)
  })

  it(
    'shows the revealed board while the bot is thinking',
    async () => {
      const user = userEvent.setup()

      let resolveHumanAction: ((value: WebSessionSnapshot) => void) | null = null
      applyHumanActionMock.mockImplementation(
        () =>
          new Promise<WebSessionSnapshot>((resolve) => {
            resolveHumanAction = resolve
          }),
      )
      let resolveBotAction: ((value: WebSessionSnapshot) => void) | null = null
      advanceBotMock.mockImplementation(
        () =>
          new Promise<WebSessionSnapshot>((resolve) => {
            resolveBotAction = resolve
          }),
      )

      render(<App />)
      const callButton = await screen.findByRole('button', { name: 'Call' })

      await user.click(callButton)

      expect(callButton).toBeDisabled()

      await act(async () => {
        resolveHumanAction?.(afterHumanSnapshot)
        await Promise.resolve()
      })

      expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Thinking')
      expect(screen.getByText('Watch the bot respond')).toBeInTheDocument()
      expect(within(screen.getByLabelText('Board cards')).getAllByRole('img', { name: /of/i })).toHaveLength(3)
      await waitFor(() => expect(advanceBotMock).toHaveBeenCalledTimes(1))

      await act(async () => {
        resolveBotAction?.(postActionSnapshot)
        await Promise.resolve()
      })

      expect(await screen.findByText('Bets to 4.0 BB')).toBeInTheDocument()
    },
    10_000,
  )

  it('fades the bot action bubble after a short delay', async () => {
    const user = userEvent.setup()

    render(<App />)
    const callButton = await screen.findByRole('button', { name: 'Call' })

    await user.click(callButton)

    expect(await screen.findByText('Bets to 4.0 BB')).toBeInTheDocument()

    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, BOT_ACTION_BUBBLE_MS + 100))
    })

    expect(screen.queryByText('Bets to 4.0 BB')).not.toBeInTheDocument()
  }, 10_000)
})
