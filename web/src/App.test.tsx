import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

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

const postActionSnapshot: WebSessionSnapshot = {
  ...baseSnapshot,
  street: 'flop',
  pot: 800,
  boardCards: ['Ah', '7d', '2c'],
  history: [
    ...baseSnapshot.history,
    'preflop: button calls',
    'preflop: big-blind raises to 4.0 bb',
    'preflop: button calls',
    'flop: Ah 7d 2c',
  ],
  legalActions: [{ id: 'check', label: 'Check' }],
}

const initMock = vi.fn().mockResolvedValue(baseSnapshot)
const resetHandMock = vi.fn().mockResolvedValue(baseSnapshot)
const applyHumanActionMock = vi.fn().mockResolvedValue(postActionSnapshot)
const disposeMock = vi.fn()

vi.mock('./lib/pokerClient', () => ({
  PokerClient: class {
    init = initMock
    resetHand = resetHandMock
    applyHumanAction = applyHumanActionMock
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
    applyHumanActionMock.mockResolvedValue(postActionSnapshot)
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

    await user.click(screen.getByRole('button', { name: 'Deal next hand' }))

    expect(resetHandMock).toHaveBeenCalledTimes(1)
  })

  it(
    'keeps the bot action bubble visible until the player acts again',
    async () => {
      const user = userEvent.setup()

      let resolveAction: ((value: WebSessionSnapshot) => void) | null = null
      applyHumanActionMock.mockImplementation(
        () =>
          new Promise<WebSessionSnapshot>((resolve) => {
            resolveAction = resolve
          }),
      )

      render(<App />)
      const callButton = await screen.findByRole('button', { name: 'Call' })

      await user.click(callButton)

      expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Thinking')
      expect(callButton).toBeDisabled()

      await act(async () => {
        resolveAction?.(postActionSnapshot)
        await Promise.resolve()
      })

      expect(await screen.findByText('Raises to 4.0 BB')).toBeInTheDocument()
      const checkButton = screen.getByRole('button', { name: 'Check' })
      expect(checkButton).not.toBeDisabled()

      let resolveFollowUp: ((value: WebSessionSnapshot) => void) | null = null
      applyHumanActionMock.mockImplementationOnce(
        () =>
          new Promise<WebSessionSnapshot>((resolve) => {
            resolveFollowUp = resolve
          }),
      )

      await user.click(checkButton)

      expect(screen.queryByText('Raises to 4.0 BB')).not.toBeInTheDocument()
      expect(screen.getByLabelText('Bot panel')).toHaveTextContent('Thinking')

      await act(async () => {
        resolveFollowUp?.(terminalSnapshot)
        await Promise.resolve()
      })
    },
    10_000,
  )
})
