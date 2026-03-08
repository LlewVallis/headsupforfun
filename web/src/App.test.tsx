import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { BOARD_REVEAL_STEP_MS, BOT_ACTION_BUBBLE_MS, BOT_MIN_THINK_MS } from './lib/presentation'
import type { WebSessionSnapshot } from './lib/pokerTypes'

const baseSnapshot: WebSessionSnapshot = {
  handNumber: 1,
  humanSeat: 'button',
  botSeat: 'bigBlind',
  botMode: 'hybridPlay',
  matchOver: false,
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
    vi.useRealTimers()
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
    expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Bot')
    expect(screen.getByLabelText('Action tray')).not.toHaveTextContent('hybrid play mode')
    expect(screen.queryByText('Session activity')).not.toBeInTheDocument()
    expect(screen.queryByText('Seed')).not.toBeInTheDocument()
    expect(screen.queryByText(/abstract/i)).not.toBeInTheDocument()
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

  it('surfaces widened abstract actions including all-in when present in the snapshot', async () => {
    initMock.mockResolvedValueOnce({
      ...baseSnapshot,
      legalActions: [
        { id: 'call', label: 'Call' },
        { id: 'raiseTo:700', label: 'Raise to 7.0 bb' },
        { id: 'allIn:9950', label: 'All-in to 99.5 bb' },
      ],
    })

    render(<App />)

    expect(await screen.findByRole('button', { name: 'Raise to 7.0 bb' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'All-in to 99.5 bb' })).toBeInTheDocument()
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
    'reveals the flop one card at a time before the bot starts thinking and keeps thinking visible for at least 0.5s',
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

      const countVisibleBoardCards = () =>
        within(screen.getByLabelText('Board cards')).queryAllByRole('img', { name: /of/i }).length

      expect(screen.getByLabelText('Bot panel')).not.toHaveTextContent('Thinking')
      expect(screen.getByText('Watch the bot respond')).toBeInTheDocument()
      await waitFor(() => expect(countVisibleBoardCards()).toBe(1), {
        timeout: BOARD_REVEAL_STEP_MS + 500,
      })
      expect(screen.getByLabelText('Bot panel')).not.toHaveTextContent('Thinking')
      await waitFor(() => expect(countVisibleBoardCards()).toBe(2), {
        timeout: (BOARD_REVEAL_STEP_MS * 2) + 500,
      })
      expect(screen.getByLabelText('Bot panel')).not.toHaveTextContent('Thinking')
      await waitFor(() => expect(countVisibleBoardCards()).toBe(3), {
        timeout: (BOARD_REVEAL_STEP_MS * 3) + 500,
      })
      await waitFor(() => expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Thinking'))
      await waitFor(() => expect(advanceBotMock).toHaveBeenCalledTimes(1))
      const thinkingStartedAt = Date.now()

      await act(async () => {
        resolveBotAction?.(postActionSnapshot)
        await Promise.resolve()
      })

      expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Thinking')

      expect(await screen.findByText('Bets to 4.0 BB')).toBeInTheDocument()
      expect(Date.now() - thinkingStartedAt).toBeGreaterThanOrEqual(BOT_MIN_THINK_MS)
      expect(screen.getByText('Pick your action')).toBeInTheDocument()
      expect(screen.getByText('Bot bets to 4.0 bb.')).toBeInTheDocument()
    },
    12_000,
  )

  it('fades the bot action bubble after a short delay', async () => {
    const user = userEvent.setup()

    render(<App />)
    const callButton = await screen.findByRole('button', { name: 'Call' })

    await user.click(callButton)

    expect(await screen.findByText('Bets to 4.0 BB', {}, { timeout: 3_500 })).toBeInTheDocument()

    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, BOT_ACTION_BUBBLE_MS + 100))
    })

    expect(screen.queryByText('Bets to 4.0 BB')).not.toBeInTheDocument()
    expect(screen.getByText('Pick your action')).toBeInTheDocument()
    expect(screen.getByText('Bot bets to 4.0 bb.')).toBeInTheDocument()
  }, 10_000)

  it('shows neutral guidance when the hero is first to act on a street', async () => {
    initMock.mockResolvedValueOnce({
      ...baseSnapshot,
      street: 'flop',
      boardCards: ['Ah', '7d', '2c'],
      history: [
        'button posts 0.5 bb',
        'big-blind posts 1.0 bb',
        'preflop: button raises to 4.0 bb',
        'preflop: big-blind calls',
        'flop: Ah 7d 2c',
      ],
      legalActions: [{ id: 'check', label: 'Check' }],
      status: 'Your turn on flop.',
    })

    render(<App />)

    expect(await screen.findByText('Pick your action')).toBeInTheDocument()
    expect(screen.getByText('Choose from the available actions below the table.')).toBeInTheDocument()
    expect(screen.queryByText(/Bot raises to 4\.0 bb\./i)).not.toBeInTheDocument()
  })

  it('updates the terminal winner text when the next hand rotates seats and the bot folds first', async () => {
    initMock.mockResolvedValueOnce(terminalSnapshot)
    resetHandMock.mockResolvedValueOnce({
      handNumber: 2,
      humanSeat: 'bigBlind',
      botSeat: 'button',
      botMode: 'hybridPlay',
      matchOver: false,
      street: 'preflop',
      phase: 'terminal',
      currentActor: null,
      pot: 150,
      boardCards: [],
      button: {
        seat: 'button',
        stack: 9850,
        totalContribution: 50,
        streetContribution: 50,
        folded: true,
        holeCards: ['Qc', 'Qd'],
      },
      bigBlind: {
        seat: 'bigBlind',
        stack: 10150,
        totalContribution: 100,
        streetContribution: 100,
        folded: false,
        holeCards: ['As', 'Kd'],
      },
      legalActions: [],
      history: [
        'button posts 0.5 bb',
        'big-blind posts 1.0 bb',
        'preflop: button folds',
        'big-blind wins uncontested for 1.5 bb',
      ],
      status: 'Hand complete.',
      terminalSummary: 'big-blind wins uncontested for 1.5 bb',
    })

    render(<App />)

    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Deal next hand' }))

    expect(await screen.findByText('You win the pot')).toBeInTheDocument()
    expect(screen.queryByText('Bot wins the pot')).not.toBeInTheDocument()
  })
})
