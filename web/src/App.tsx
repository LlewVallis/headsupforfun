import { useEffect, useMemo, useRef, useState } from 'react'

import { PokerCard } from './components/PokerCard'
import { PokerChipMark } from './components/PokerChipMark'
import { PokerClient } from './lib/pokerClient'
import {
  BOT_ACTION_REVEAL_MS,
  actionPrompt,
  botLabel,
  buildPlayerSessionConfig,
  extractBotActionLabel,
  formatBigBlinds,
  heroLabel,
  humanizeHistoryEntry,
  presentTerminalSummary,
  seatBadge,
} from './lib/presentation'
import type { WebSessionConfig, WebSessionSnapshot } from './lib/pokerTypes'

type BotPresence =
  | { state: 'idle' }
  | { state: 'thinking' }
  | { state: 'action'; label: string }

function App() {
  const clientRef = useRef<PokerClient | null>(null)
  const initRequestRef = useRef(0)
  const revealTimerRef = useRef<number | null>(null)
  const [snapshot, setSnapshot] = useState<WebSessionSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [revealLocked, setRevealLocked] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [botPresence, setBotPresence] = useState<BotPresence>({ state: 'idle' })

  useEffect(() => {
    void recreateClientAndInitialize(buildPlayerSessionConfig())

    return () => {
      clearRevealTimer()
      disposeClient()
    }
  }, [])

  const hero = useMemo(() => {
    if (!snapshot) {
      return null
    }
    return snapshot.humanSeat === 'button' ? snapshot.button : snapshot.bigBlind
  }, [snapshot])

  const villain = useMemo(() => {
    if (!snapshot) {
      return null
    }
    return snapshot.botSeat === 'button' ? snapshot.button : snapshot.bigBlind
  }, [snapshot])

  const recapEntries = useMemo(() => {
    if (!snapshot) {
      return []
    }

    return snapshot.history.map((entry) => humanizeHistoryEntry(entry, snapshot))
  }, [snapshot])

  const outcome = useMemo(() => {
    if (!snapshot) {
      return null
    }
    return presentTerminalSummary(snapshot.terminalSummary, snapshot)
  }, [snapshot])

  const boardCards = fillBoardCards(snapshot?.boardCards ?? [])
  const heroTurn = snapshot?.currentActor === snapshot?.humanSeat && !snapshot?.terminalSummary
  const controlsLocked = busy || loading || revealLocked

  const handleNewMatch = async () => {
    clearRevealTimer()
    setBotPresence({ state: 'idle' })
    await recreateClientAndInitialize(buildPlayerSessionConfig())
  }

  const handleRetry = async () => {
    await handleNewMatch()
  }

  const handleNextHand = async () => {
    const client = clientRef.current
    if (!client) {
      return
    }

    clearRevealTimer()
    setBotPresence({ state: 'idle' })
    await runClientAction(async () => {
      const nextSnapshot = await client.resetHand()
      setSnapshot(nextSnapshot)
    })
  }

  const handleAction = async (actionId: string) => {
    const client = clientRef.current
    const previousSnapshot = snapshot
    if (!client || !previousSnapshot) {
      return
    }

    clearRevealTimer()
    setBotPresence({ state: 'thinking' })
    await runClientAction(async () => {
      const nextSnapshot = await client.applyHumanAction(actionId)
      setSnapshot(nextSnapshot)

      const botAction = extractBotActionLabel(previousSnapshot, nextSnapshot)
      if (botAction) {
        setBotPresence({ state: 'action', label: botAction })
        setRevealLocked(true)
        revealTimerRef.current = window.setTimeout(() => {
          setBotPresence({ state: 'idle' })
          setRevealLocked(false)
          revealTimerRef.current = null
        }, BOT_ACTION_REVEAL_MS)
        return
      }

      setBotPresence({ state: 'idle' })
      setRevealLocked(false)
    })
  }

  async function recreateClientAndInitialize(config: WebSessionConfig): Promise<void> {
    const previousClient = clientRef.current
    const client = new PokerClient()
    clientRef.current = client
    previousClient?.dispose()

    await initializeSession(client, config)
  }

  function disposeClient(): void {
    const client = clientRef.current
    clientRef.current = null
    client?.dispose()
  }

  function clearRevealTimer(): void {
    if (revealTimerRef.current !== null) {
      window.clearTimeout(revealTimerRef.current)
      revealTimerRef.current = null
    }
    setRevealLocked(false)
  }

  async function initializeSession(
    client: PokerClient,
    config: WebSessionConfig,
  ): Promise<void> {
    const requestId = initRequestRef.current + 1
    initRequestRef.current = requestId
    setLoading(true)
    setBusy(true)
    setError(null)

    try {
      const nextSnapshot = await client.init(config)
      if (clientRef.current !== client || initRequestRef.current !== requestId) {
        return
      }
      setSnapshot(nextSnapshot)
      setBotPresence({ state: 'idle' })
    } catch (err) {
      if (clientRef.current !== client || initRequestRef.current !== requestId) {
        return
      }
      setError(toErrorMessage(err))
      setSnapshot(null)
      setBotPresence({ state: 'idle' })
    } finally {
      if (clientRef.current !== client || initRequestRef.current !== requestId) {
        return
      }
      setBusy(false)
      setLoading(false)
    }
  }

  async function runClientAction(action: () => Promise<void>): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await action()
    } catch (err) {
      clearRevealTimer()
      setBotPresence({ state: 'idle' })
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top,rgba(53,113,85,0.35),transparent_28%),radial-gradient(circle_at_bottom,rgba(10,39,29,0.75),transparent_42%),linear-gradient(180deg,#07110d_0%,#071914_42%,#050a08_100%)] px-4 pb-12 pt-6 text-ivory-100 md:px-6 lg:px-8">
      <div className="mx-auto flex w-full max-w-[1200px] flex-col gap-5">
        <header className="flex flex-col gap-4 rounded-[2rem] border border-white/8 bg-black/16 px-5 py-5 shadow-[0_25px_80px_rgba(0,0,0,0.34)] backdrop-blur-sm md:flex-row md:items-center md:justify-between md:px-7">
          <div className="flex items-center gap-4">
            <span className="grid h-14 w-14 place-items-center rounded-2xl bg-[linear-gradient(180deg,rgba(15,51,38,0.96),rgba(10,22,18,0.92))] ring-1 ring-white/8 shadow-[0_16px_30px_rgba(0,0,0,0.28)]">
              <PokerChipMark className="h-10 w-10" />
            </span>
            <div>
              <p className="text-[0.72rem] font-medium uppercase tracking-[0.28em] text-gold-300/88">
                Heads-up no-limit hold&apos;em
              </p>
              <h1 className="mt-2 text-[clamp(2.2rem,4vw,3.8rem)] font-semibold leading-none tracking-[-0.05em] text-white">
                Heads-Up Hold&apos;em
              </h1>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-white/68 md:text-base">
                One table. One opponent. Pick your line and play out the hand.
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-3 md:justify-end">
            {snapshot ? (
              <div className="flex flex-wrap gap-2">
                <InfoChip label={`Hand ${snapshot.handNumber}`} />
                <InfoChip label="0.5 / 1 blinds" />
                <InfoChip label="Fresh 100 BB duel" />
              </div>
            ) : null}
            <button
              type="button"
              className="inline-flex min-h-12 items-center justify-center rounded-full bg-[linear-gradient(180deg,#efdca4,#d6ac56)] px-5 text-sm font-semibold uppercase tracking-[0.16em] text-[#102116] shadow-[0_10px_24px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
              onClick={handleNewMatch}
              disabled={busy}
            >
              {loading ? 'Opening table...' : 'New match'}
            </button>
          </div>
        </header>

        {error ? (
          <section
            className="rounded-[1.6rem] border border-rose-200/16 bg-rose-300/8 px-5 py-4 text-white shadow-[0_20px_60px_rgba(0,0,0,0.22)]"
            role="alert"
          >
            <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div>
                <p className="text-[0.72rem] font-medium uppercase tracking-[0.22em] text-rose-100/82">
                  Table reset needed
                </p>
                <p className="mt-2 text-sm leading-6 text-white/78 md:text-base">
                  The table could not finish the last action. Start a fresh match.
                </p>
                <p className="mt-1 text-sm text-white/50">{error}</p>
              </div>
              <button
                type="button"
                className="inline-flex min-h-11 items-center justify-center rounded-full border border-white/12 bg-white/6 px-5 text-sm font-medium uppercase tracking-[0.16em] text-white transition duration-150 hover:bg-white/10 disabled:cursor-not-allowed disabled:opacity-55"
                onClick={handleRetry}
                disabled={busy}
              >
                Reload table
              </button>
            </div>
          </section>
        ) : null}

        {snapshot && hero && villain ? (
          <>
            <section
              className="relative overflow-hidden rounded-[2.35rem] border border-white/8 bg-[linear-gradient(180deg,rgba(8,24,18,0.95),rgba(5,14,11,0.98))] px-4 py-5 shadow-[0_32px_120px_rgba(0,0,0,0.42)] md:px-8 md:py-8"
              aria-label="Poker table"
            >
              <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_16%,rgba(223,183,91,0.12),transparent_22%),radial-gradient(circle_at_50%_55%,rgba(52,133,92,0.36),transparent_38%)]" />
              <div className="pointer-events-none absolute inset-[7%] rounded-[999px] border-[14px] border-[#6e4d28] bg-[radial-gradient(circle_at_50%_35%,rgba(36,122,82,0.96),rgba(10,63,42,0.98)_60%,rgba(8,40,28,0.98)_100%)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.04),inset_0_40px_80px_rgba(255,255,255,0.06),inset_0_-60px_90px_rgba(0,0,0,0.34)]" />
              <div className="pointer-events-none absolute inset-[9%] rounded-[999px] border border-white/7" />

              <div className="relative z-10 grid min-h-[720px] grid-rows-[auto_1fr_auto_auto] gap-5 md:min-h-[760px]">
                <PlayerSeat
                  label={botLabel()}
                  badge={seatBadge(villain.seat)}
                  stack={villain.stack}
                  cards={villain.holeCards}
                  hiddenCards={villain.holeCards.length === 0}
                  active={false}
                  mood={botPresence.state}
                  bubbleLabel={
                    botPresence.state === 'action' ? botPresence.label : undefined
                  }
                  thinkingLabel={
                    botPresence.state === 'thinking' ? 'Thinking' : undefined
                  }
                  align="top"
                />

                <section className="flex flex-col items-center justify-center px-2 text-center">
                  <div className="rounded-full border border-white/10 bg-black/22 px-4 py-2 text-[0.72rem] font-medium uppercase tracking-[0.24em] text-gold-300/88 shadow-[0_12px_24px_rgba(0,0,0,0.22)]">
                    {snapshot.street}
                  </div>
                  <div className="mt-5 grid place-items-center rounded-full border border-gold-300/18 bg-black/18 px-8 py-4 shadow-[0_18px_40px_rgba(0,0,0,0.26)]">
                    <span className="text-[0.68rem] uppercase tracking-[0.24em] text-white/45">
                      Pot
                    </span>
                    <strong className="mt-1 text-2xl font-semibold text-white md:text-3xl">
                      {formatBigBlinds(snapshot.pot)}
                    </strong>
                  </div>
                  <div className="mt-6 flex flex-wrap justify-center gap-3 md:gap-4" aria-label="Board cards">
                    {boardCards.map((card, index) => (
                      <PokerCard
                        key={`${card ?? 'empty'}-${index}`}
                        card={card}
                        className="h-[112px] w-[80px] md:h-[136px] md:w-[96px]"
                      />
                    ))}
                  </div>
                  <div className="mt-6 max-w-lg">
                    <p className="text-[0.72rem] uppercase tracking-[0.26em] text-gold-300/80">
                      {outcome ? 'Showdown' : heroTurn ? 'Your move' : actionPrompt(snapshot, busy)}
                    </p>
                    <h2 className="mt-2 text-2xl font-semibold tracking-[-0.04em] text-white md:text-[2.4rem]">
                      {outcome ? outcome.headline : heroTurn ? 'Pick your action' : 'Watch the bot respond'}
                    </h2>
                    <p className="mt-2 text-sm leading-6 text-white/68 md:text-base">
                      {outcome
                        ? outcome.detail
                        : botPresence.state === 'action'
                          ? `${botLabel()} ${botPresence.label.toLowerCase()}.`
                          : heroTurn
                            ? 'Choose from the available abstract actions below the table.'
                            : 'The next action will appear beside the bot seat.'}
                    </p>
                  </div>
                </section>

                <PlayerSeat
                  label={heroLabel()}
                  badge={seatBadge(hero.seat)}
                  stack={hero.stack}
                  cards={hero.holeCards}
                  hiddenCards={false}
                  active={heroTurn && !controlsLocked}
                  tone="hero"
                  align="bottom"
                />

                <section
                  className="mx-auto w-full max-w-3xl rounded-[1.7rem] border border-white/8 bg-black/22 px-4 py-4 shadow-[0_18px_50px_rgba(0,0,0,0.28)] backdrop-blur-sm md:px-5"
                  aria-label="Action tray"
                >
                  <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
                    <div>
                      <p className="text-[0.72rem] uppercase tracking-[0.24em] text-gold-300/80">
                        Action tray
                      </p>
                      <p className="mt-2 text-sm leading-6 text-white/70 md:text-base">
                        {outcome
                          ? 'Shuffle up and deal the next hand whenever you are ready.'
                          : botPresence.state === 'thinking'
                            ? `${botLabel()} is working through the spot.`
                            : 'The bot stays on hybrid play mode for every hand.'}
                      </p>
                    </div>
                    {outcome ? (
                      <button
                        type="button"
                        className="inline-flex min-h-12 items-center justify-center rounded-full bg-[linear-gradient(180deg,#efdca4,#d6ac56)] px-6 text-sm font-semibold uppercase tracking-[0.16em] text-[#102116] shadow-[0_10px_24px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
                        onClick={handleNextHand}
                        disabled={controlsLocked}
                      >
                        Deal next hand
                      </button>
                    ) : (
                      <div className="flex flex-wrap justify-end gap-3 md:max-w-[65%]">
                        {snapshot.legalActions.map((action) => (
                          <button
                            key={action.id}
                            type="button"
                            className="inline-flex min-h-12 items-center justify-center rounded-full border border-white/12 bg-white/8 px-4 text-sm font-semibold uppercase tracking-[0.14em] text-white shadow-[0_8px_22px_rgba(0,0,0,0.18)] transition duration-150 hover:-translate-y-0.5 hover:border-gold-300/34 hover:bg-white/12 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
                            onClick={() => handleAction(action.id)}
                            disabled={controlsLocked}
                          >
                            {action.label}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </section>
              </div>
            </section>

            <details
              className="group rounded-[1.6rem] border border-white/8 bg-black/14 px-5 py-4 text-white/82 shadow-[0_14px_36px_rgba(0,0,0,0.18)]"
              aria-label="Hand recap"
            >
              <summary className="flex cursor-pointer list-none items-center justify-between gap-3 text-sm font-semibold uppercase tracking-[0.18em] text-gold-300/84 marker:hidden">
                <span>Hand recap</span>
                <span className="text-[0.7rem] tracking-[0.16em] text-white/42 transition group-open:rotate-45">
                  +
                </span>
              </summary>
              <ol className="mt-4 grid gap-2.5 pl-5 text-sm leading-6 text-white/70 md:text-[0.95rem]">
                {recapEntries.map((entry, index) => (
                  <li key={`${entry}-${index}`}>{entry}</li>
                ))}
              </ol>
            </details>
          </>
        ) : (
          <section className="rounded-[1.8rem] border border-white/8 bg-black/16 px-6 py-8 text-center shadow-[0_24px_80px_rgba(0,0,0,0.26)]">
            <div className="mx-auto flex max-w-md flex-col items-center gap-4">
              <span className="grid h-16 w-16 place-items-center rounded-full border border-white/10 bg-white/6">
                <PokerChipMark className="h-10 w-10 opacity-85" />
              </span>
              <div>
                <h2 className="text-2xl font-semibold tracking-[-0.04em] text-white">
                  Opening the table
                </h2>
                <p className="mt-2 text-sm leading-6 text-white/64 md:text-base">
                  {loading ? 'Shuffling cards and syncing the bot.' : 'No active poker table.'}
                </p>
              </div>
            </div>
          </section>
        )}
      </div>
    </main>
  )
}

function PlayerSeat(props: {
  label: string
  badge: string
  stack: number
  cards: string[]
  hiddenCards: boolean
  active: boolean
  align: 'top' | 'bottom'
  tone?: 'table' | 'hero'
  mood?: BotPresence['state']
  bubbleLabel?: string
  thinkingLabel?: string
}) {
  const ringClass = props.active
    ? 'ring-2 ring-gold-300/48 shadow-[0_0_0_1px_rgba(255,255,255,0.06),0_28px_60px_rgba(0,0,0,0.26)]'
    : 'ring-1 ring-white/8 shadow-[0_20px_50px_rgba(0,0,0,0.24)]'

  return (
    <section
      className="mx-auto flex w-full max-w-[460px] flex-col items-center gap-3 text-center"
      aria-label={props.label === heroLabel() ? 'Hero panel' : 'Bot panel'}
    >
      <div className="relative">
        {props.bubbleLabel ? (
          <div className="action-bubble absolute -right-4 -top-6 rounded-full border border-gold-300/26 bg-black/55 px-4 py-2 text-[0.72rem] font-semibold uppercase tracking-[0.14em] text-gold-300 shadow-[0_12px_30px_rgba(0,0,0,0.28)] md:-right-10">
            {props.bubbleLabel}
          </div>
        ) : null}
        <div
          className={joinClasses(
            'min-w-[240px] rounded-[1.7rem] border bg-black/24 px-5 py-4 backdrop-blur-sm transition duration-200',
            ringClass,
            props.mood === 'thinking' ? 'bot-thinking border-gold-300/32' : 'border-white/8',
          )}
        >
          <div className="flex items-center justify-center gap-3">
            <span className="grid h-12 w-12 place-items-center rounded-full border border-white/10 bg-white/6 text-sm font-semibold uppercase tracking-[0.14em] text-white/82">
              {props.label === heroLabel() ? 'You' : 'Bot'}
            </span>
            <div>
              <p className="text-[0.72rem] uppercase tracking-[0.22em] text-white/48">
                {props.badge}
              </p>
              <h3 className="mt-1 text-xl font-semibold tracking-[-0.04em] text-white">
                {props.label}
              </h3>
            </div>
          </div>
          <p className="mt-3 text-sm uppercase tracking-[0.2em] text-gold-300/84">
            {formatBigBlinds(props.stack)}
          </p>
          {props.thinkingLabel ? (
            <div className="mt-3 inline-flex items-center gap-2 rounded-full border border-gold-300/18 bg-gold-300/10 px-3 py-1.5 text-[0.72rem] uppercase tracking-[0.16em] text-gold-300">
              <span>{props.thinkingLabel}</span>
              <ThinkingDots />
            </div>
          ) : null}
        </div>
      </div>
      <div className={joinClasses('flex justify-center gap-3', props.align === 'top' ? 'pt-1' : 'pb-1')}>
        {(props.cards.length > 0 ? props.cards : [null, null]).map((card, index) => (
          <PokerCard
            key={`${card ?? 'hidden'}-${index}`}
            card={card}
            hidden={props.hiddenCards}
            tone={props.tone ?? 'table'}
            className={joinClasses(
              'h-[124px] w-[88px] transition duration-200 md:h-[152px] md:w-[108px]',
              props.align === 'bottom' && index === 0 ? 'rotate-[-4deg]' : '',
              props.align === 'bottom' && index === 1 ? 'rotate-[5deg]' : '',
              props.align === 'top' && index === 0 ? 'rotate-[3deg]' : '',
              props.align === 'top' && index === 1 ? 'rotate-[-3deg]' : '',
            )}
          />
        ))}
      </div>
    </section>
  )
}

function ThinkingDots() {
  return (
    <span className="flex gap-1" aria-hidden="true">
      <span className="thinking-dot h-1.5 w-1.5 rounded-full bg-current" />
      <span className="thinking-dot h-1.5 w-1.5 rounded-full bg-current [animation-delay:120ms]" />
      <span className="thinking-dot h-1.5 w-1.5 rounded-full bg-current [animation-delay:240ms]" />
    </span>
  )
}

function InfoChip(props: { label: string }) {
  return (
    <span className="rounded-full border border-white/10 bg-white/6 px-4 py-2 text-[0.72rem] font-medium uppercase tracking-[0.2em] text-white/74">
      {props.label}
    </span>
  )
}

function fillBoardCards(cards: string[]): Array<string | null> {
  const board: Array<string | null> = [...cards]
  while (board.length < 5) {
    board.push(null)
  }
  return board
}

function toErrorMessage(value: unknown): string {
  return value instanceof Error ? value.message : 'Unknown frontend error'
}

function joinClasses(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}

export default App
