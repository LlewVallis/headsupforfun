import { describe, expect, it } from 'vitest'

import { cueForActionLabel } from './tableAudio'

describe('table audio cue mapping', () => {
  it('maps fold and check labels to their dedicated cues', () => {
    expect(cueForActionLabel('Fold')).toBe('fold')
    expect(cueForActionLabel('Folds')).toBe('fold')
    expect(cueForActionLabel('Check')).toBe('check')
    expect(cueForActionLabel('Checks')).toBe('check')
  })

  it('maps wager-style actions to the wager cue', () => {
    expect(cueForActionLabel('Call')).toBe('wager')
    expect(cueForActionLabel('Calls')).toBe('wager')
    expect(cueForActionLabel('Bets to 4.0 BB')).toBe('wager')
    expect(cueForActionLabel('Raise to 7.0 bb')).toBe('wager')
    expect(cueForActionLabel('Raises to 4.9 BB')).toBe('wager')
  })

  it('maps all-in labels to the all-in cue', () => {
    expect(cueForActionLabel('All-in to 99.5 bb')).toBe('allIn')
    expect(cueForActionLabel('Moves all-in to 100.0 BB')).toBe('allIn')
  })

  it('returns null for unknown text', () => {
    expect(cueForActionLabel(null)).toBeNull()
    expect(cueForActionLabel('Waiting')).toBeNull()
  })
})
