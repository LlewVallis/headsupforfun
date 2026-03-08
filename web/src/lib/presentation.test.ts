import { describe, expect, it, vi } from 'vitest'

import type { WebSessionSnapshot } from './pokerTypes'
import {
  BOT_ACTION_BUBBLE_MS,
  BOT_MIN_THINK_MS,
  BOARD_REVEAL_STEP_MS,
  actionPrompt,
  buildPlayerSessionConfig,
  completedMatchWinner,
  currentStreetBotActionLabel,
  defaultTestSeed,
  extractBotActionLabel,
  formatBigBlinds,
  humanizeHistoryEntry,
  presentTerminalSummary,
  readSessionSeedOverride,
  seatBadge,
} from './presentation'

const snapshot: WebSessionSnapshot = {
  handNumber: 2,
  humanSeat: 'button',
  botSeat: 'bigBlind',
  botMode: 'hybridPlay',
  matchOver: false,
  street: 'turn',
  phase: 'bettingRound',
  currentActor: 'button',
  pot: 950,
  boardCards: ['As', 'Kd', '7h', '2c'],
  button: {
    seat: 'button',
    stack: 9200,
    totalContribution: 800,
    streetContribution: 200,
    folded: false,
    holeCards: ['Qh', 'Qs'],
  },
  bigBlind: {
    seat: 'bigBlind',
    stack: 8850,
    totalContribution: 1150,
    streetContribution: 200,
    folded: false,
    holeCards: [],
  },
  legalActions: [{ id: 'call', label: 'Call' }],
  history: ['button posts 0.5 bb'],
  status: 'Your turn on turn.',
  terminalSummary: null,
}

describe('presentation helpers', () => {
  it('builds player configs with the fixed hybrid-play bot mode', () => {
    const crypto = {
      getRandomValues(values: Uint32Array) {
        values[0] = 42
        return values
      },
    } as Crypto

    const spy = vi.spyOn(globalThis, 'crypto', 'get').mockReturnValue(crypto)
    expect(buildPlayerSessionConfig()).toEqual({
      seed: 42,
      humanSeat: 'button',
      botMode: 'hybridPlay',
    })
    spy.mockRestore()
  })

  it('prefers the hidden test seed override when present', () => {
    expect(readSessionSeedOverride({ __GTO_TEST_SEED__: '19' } as unknown as typeof globalThis)).toBe(19)
    expect(readSessionSeedOverride({ __GTO_TEST_SEED__: 21 } as unknown as typeof globalThis)).toBe(21)
    expect(defaultTestSeed()).toBe(7)
  })

  it('humanizes history entries for the player-facing recap', () => {
    expect(humanizeHistoryEntry('button posts 0.5 bb', snapshot)).toBe('You post 0.5 BB')
    expect(humanizeHistoryEntry('turn: button raises to 9.5 bb', snapshot)).toBe(
      'Turn: You raise to 9.5 BB',
    )
    expect(humanizeHistoryEntry('turn: big-blind calls', snapshot)).toBe(
      'Turn: Bot calls',
    )
  })

  it('extracts the latest bot action from newly appended history', () => {
    const nextSnapshot: WebSessionSnapshot = {
      ...snapshot,
      history: [
        ...snapshot.history,
        'turn: button calls',
        'river: As Kd 7h 2c 2d',
        'river: big-blind bets to 12.0 bb',
      ],
      street: 'river',
      boardCards: ['As', 'Kd', '7h', '2c', '2d'],
      currentActor: 'button',
      legalActions: [{ id: 'call', label: 'Call' }],
    }

    expect(extractBotActionLabel(snapshot, nextSnapshot)).toBe('Bets to 12.0 BB')
  })

  it('presents terminal summaries in player language', () => {
    expect(presentTerminalSummary('button wins at showdown for 12.0 bb', snapshot)).toEqual({
      headline: 'You win the pot',
      detail: '12.0 BB at showdown',
    })
    expect(presentTerminalSummary('big-blind wins uncontested for 1.5 bb', snapshot)).toEqual({
      headline: 'Bot wins the pot',
      detail: '1.5 BB without showdown',
    })
    expect(
      presentTerminalSummary('big-blind wins uncontested for 1.5 bb', {
        humanSeat: 'bigBlind',
        botSeat: 'button',
      }),
    ).toEqual({
      headline: 'You win the pot',
      detail: '1.5 BB without showdown',
    })
    expect(presentTerminalSummary('showdown split pot for 4.5 bb', snapshot)).toEqual({
      headline: 'Split pot',
      detail: '4.5 BB',
    })
  })

  it('only keeps the bot action detail when it happened on the current street', () => {
    expect(
      currentStreetBotActionLabel({
        ...snapshot,
        street: 'flop',
        history: [
          'button posts 0.5 bb',
          'big-blind posts 1.0 bb',
          'preflop: big-blind raises to 4.0 bb',
          'preflop: button calls',
          'flop: As Kd 7h',
        ],
      }),
    ).toBeNull()

    expect(
      currentStreetBotActionLabel({
        ...snapshot,
        street: 'turn',
        history: [
          'button posts 0.5 bb',
          'big-blind posts 1.0 bb',
          'turn: big-blind bets to 12.0 bb',
        ],
      }),
    ).toBe('Bets to 12.0 BB')
  })

  it('provides simple display helpers for the table UI', () => {
    expect(formatBigBlinds(150)).toBe('1.5 BB')
    expect(seatBadge('button')).toBe('Dealer')
    expect(actionPrompt(snapshot, false)).toBe('Choose your move')
    expect(actionPrompt({ ...snapshot, terminalSummary: 'button wins uncontested for 1.5 bb' }, false)).toBe(
      'Hand complete',
    )
    expect(actionPrompt(snapshot, true)).toBe('Bot is thinking...')
    expect(BOT_ACTION_BUBBLE_MS).toBeGreaterThan(0)
    expect(BOT_MIN_THINK_MS).toBeGreaterThan(0)
    expect(BOARD_REVEAL_STEP_MS).toBeGreaterThan(0)
  })

  it('identifies completed match winners from the persistent stacks', () => {
    expect(
      completedMatchWinner({
        ...snapshot,
        matchOver: true,
        button: {
          ...snapshot.button,
          stack: 20_000,
        },
        bigBlind: {
          ...snapshot.bigBlind,
          stack: 0,
        },
      }),
    ).toBe('hero')

    expect(
      completedMatchWinner({
        ...snapshot,
        matchOver: true,
        button: {
          ...snapshot.button,
          stack: 0,
        },
        bigBlind: {
          ...snapshot.bigBlind,
          stack: 20_000,
        },
      }),
    ).toBe('bot')

    expect(
      completedMatchWinner({
        ...snapshot,
        matchOver: false,
      }),
    ).toBeNull()

    expect(
      completedMatchWinner({
        ...snapshot,
        matchOver: true,
        button: {
          ...snapshot.button,
          stack: 10_000,
        },
        bigBlind: {
          ...snapshot.bigBlind,
          stack: 10_000,
        },
      }),
    ).toBeNull()
  })
})
