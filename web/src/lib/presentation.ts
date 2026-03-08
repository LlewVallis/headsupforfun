import type { WebBotMode, WebSeat, WebSessionConfig, WebSessionSnapshot } from './pokerTypes'

export const PLAYER_BOT_MODE: WebBotMode = 'hybridPlay'
export const BOT_ACTION_BUBBLE_MS = 1400
export const BOT_MIN_THINK_MS = 500
export const BOARD_REVEAL_STEP_MS = 500

const DEFAULT_TEST_SEED = 7

type HostGlobals = typeof globalThis & {
  __GTO_TEST_SEED__?: number | string
}

export interface PresentedOutcome {
  headline: string
  detail: string
}

export function buildPlayerSessionConfig(): WebSessionConfig {
  return {
    seed: readSessionSeedOverride() ?? generateSessionSeed(),
    humanSeat: 'button',
    botMode: PLAYER_BOT_MODE,
  }
}

export function generateSessionSeed(host: Crypto | undefined = globalThis.crypto): number {
  if (host && typeof host.getRandomValues === 'function') {
    const values = new Uint32Array(1)
    host.getRandomValues(values)
    return values[0]
  }

  return Math.floor(Math.random() * 0xffff_ffff)
}

export function readSessionSeedOverride(host: HostGlobals = globalThis): number | null {
  const rawValue = host.__GTO_TEST_SEED__
  if (typeof rawValue === 'number' && Number.isSafeInteger(rawValue) && rawValue >= 0) {
    return rawValue
  }

  if (typeof rawValue === 'string') {
    const parsed = Number.parseInt(rawValue, 10)
    if (Number.isSafeInteger(parsed) && parsed >= 0) {
      return parsed
    }
  }

  return null
}

export function defaultTestSeed(): number {
  return DEFAULT_TEST_SEED
}

export function formatBigBlinds(chips: number): string {
  return `${(chips / 100).toFixed(1)} BB`
}

export function seatToken(seat: WebSeat): string {
  return seat === 'button' ? 'button' : 'big-blind'
}

export function seatBadge(seat: WebSeat): string {
  return seat === 'button' ? 'Dealer' : 'Big Blind'
}

export function heroLabel(): string {
  return 'You'
}

export function botLabel(): string {
  return 'Bot'
}

export function humanizeHistoryEntry(
  entry: string,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): string {
  const blindMatch = entry.match(/^(button|big-blind) posts (.+)$/)
  if (blindMatch) {
    const seat = blindMatch[1] as WebSeatToken
    const verb = seat === seatToken(snapshot.humanSeat) ? 'post' : 'posts'
    return `${labelForSeat(seat, snapshot)} ${verb} ${normalizeBigBlindText(blindMatch[2])}`
  }

  const actionMatch = entry.match(/^(preflop|flop|turn|river): (button|big-blind) (.+)$/)
  if (actionMatch) {
    const [, street, seat, phrase] = actionMatch
    return `${titleCase(street)}: ${subjectForSeat(seat as WebSeatToken, snapshot)} ${toDisplayPhrase(
      phrase,
      seat === seatToken(snapshot.humanSeat),
    )}`
  }

  const boardMatch = entry.match(/^(flop|turn|river): (.+)$/)
  if (boardMatch) {
    return `${titleCase(boardMatch[1])}: ${boardMatch[2]}`
  }

  return humanizeOutcomeEntry(entry, snapshot)
}

export function presentTerminalSummary(
  summary: string | null,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): PresentedOutcome | null {
  if (!summary) {
    return null
  }

  const uncontested = summary.match(/^(button|big-blind) wins uncontested for (.+)$/)
  if (uncontested) {
    return {
      headline: winnerHeadline(uncontested[1] as WebSeatToken, snapshot),
      detail: `${normalizeBigBlindText(uncontested[2])} without showdown`,
    }
  }

  const showdown = summary.match(/^(button|big-blind) wins at showdown for (.+)$/)
  if (showdown) {
    return {
      headline: winnerHeadline(showdown[1] as WebSeatToken, snapshot),
      detail: `${normalizeBigBlindText(showdown[2])} at showdown`,
    }
  }

  const split = summary.match(/^showdown split pot for (.+)$/)
  if (split) {
    return {
      headline: 'Split pot',
      detail: normalizeBigBlindText(split[1]),
    }
  }

  return {
    headline: 'Hand complete',
    detail: normalizeBigBlindText(summary),
  }
}

export function extractBotActionLabel(
  previous: WebSessionSnapshot | null,
  next: WebSessionSnapshot,
): string | null {
  if (!previous) {
    return currentStreetBotActionLabel(next)
  }

  const botHistory = next.history.slice(previous?.history.length ?? 0)
  const token = seatToken(next.botSeat)

  for (const entry of [...botHistory].reverse()) {
    const actionMatch = entry.match(/^(preflop|flop|turn|river): (button|big-blind) (.+)$/)
    if (!actionMatch || actionMatch[2] !== token) {
      continue
    }

    return titleCase(toDisplayPhrase(actionMatch[3], false))
  }

  return null
}

export function currentStreetBotActionLabel(snapshot: WebSessionSnapshot): string | null {
  const token = seatToken(snapshot.botSeat)

  for (const entry of [...snapshot.history].reverse()) {
    const actionMatch = entry.match(/^(preflop|flop|turn|river): (button|big-blind) (.+)$/)
    if (!actionMatch || actionMatch[1] !== snapshot.street || actionMatch[2] !== token) {
      continue
    }

    return titleCase(toDisplayPhrase(actionMatch[3], false))
  }

  return null
}

export function actionPrompt(snapshot: WebSessionSnapshot, busy: boolean): string {
  if (busy) {
    return 'Bot is thinking...'
  }

  if (snapshot.terminalSummary) {
    return 'Hand complete'
  }

  return 'Choose your move'
}

type WebSeatToken = 'button' | 'big-blind'

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}

function normalizeBigBlindText(value: string): string {
  return value.replace(/\bbb\b/g, 'BB')
}

function labelForSeat(
  seat: WebSeatToken,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): string {
  return seat === seatToken(snapshot.humanSeat) ? heroLabel() : botLabel()
}

function subjectForSeat(
  seat: WebSeatToken,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): string {
  return seat === seatToken(snapshot.humanSeat) ? heroLabel() : botLabel()
}

function winnerHeadline(
  seat: WebSeatToken,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): string {
  return seat === seatToken(snapshot.humanSeat) ? 'You win the pot' : 'Bot wins the pot'
}

function humanizeOutcomeEntry(
  entry: string,
  snapshot: Pick<WebSessionSnapshot, 'humanSeat' | 'botSeat'>,
): string {
  const outcome = presentTerminalSummary(entry, snapshot)
  if (!outcome) {
    return entry
  }

  return `${outcome.headline} - ${outcome.detail}`
}

function toDisplayPhrase(phrase: string, isHero: boolean): string {
  const normalized = normalizeBigBlindText(phrase)
  if (!isHero) {
    return normalized
  }

  if (normalized.startsWith('folds')) {
    return normalized.replace('folds', 'fold')
  }
  if (normalized.startsWith('checks')) {
    return normalized.replace('checks', 'check')
  }
  if (normalized.startsWith('calls')) {
    return normalized.replace('calls', 'call')
  }
  if (normalized.startsWith('bets')) {
    return normalized.replace('bets', 'bet')
  }
  if (normalized.startsWith('raises')) {
    return normalized.replace('raises', 'raise')
  }
  if (normalized.startsWith('moves')) {
    return normalized.replace('moves', 'move')
  }

  return normalized
}
