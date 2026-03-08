import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import type { WebSessionSnapshot } from './lib/pokerTypes'

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
  legalActions: [
    { id: 'call', label: 'Call' },
    { id: 'raiseTo:250', label: 'Raise to 2.5 bb' },
  ],
  history: ['button posts 0.5 bb', 'big-blind posts 1.0 bb'],
  status: 'Your turn on preflop.',
  terminalSummary: null,
}

const initMock = vi.fn().mockResolvedValue(mockSnapshot)
const resetHandMock = vi.fn().mockResolvedValue(mockSnapshot)
const applyHumanActionMock = vi.fn().mockResolvedValue(mockSnapshot)
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
    initMock.mockResolvedValue(mockSnapshot)
    resetHandMock.mockResolvedValue(mockSnapshot)
    applyHumanActionMock.mockResolvedValue(mockSnapshot)
    disposeMock.mockClear()
  })

  it('renders the Rust-backed poker UI shell', async () => {
    render(<App />)

    expect(
      screen.getByRole('heading', { name: 'GTO Poker' }),
    ).toBeInTheDocument()
    expect(await screen.findByText('Restart session')).toBeInTheDocument()
    expect(await screen.findByText('Call')).toBeInTheDocument()
    expect(screen.getByLabelText('Poker table')).toBeInTheDocument()
    expect(screen.getByLabelText('Hand history')).toBeInTheDocument()
    expect(screen.getByLabelText('Session activity')).toHaveTextContent('Ready')
  })

  it('shows a recoverable worker error banner when initialization fails', async () => {
    initMock.mockRejectedValueOnce(new Error('init failed'))

    render(<App />)

    expect(await screen.findByRole('alert')).toHaveTextContent('init failed')

    const user = userEvent.setup()
    initMock.mockResolvedValueOnce(mockSnapshot)
    await user.click(screen.getByRole('button', { name: 'Retry session' }))

    expect(await screen.findByText('Call')).toBeInTheDocument()
  })
})
