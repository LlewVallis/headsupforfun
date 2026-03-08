import { useEffect, useMemo, useRef, useState } from 'react'

import { PokerCard } from './components/PokerCard'
import { PokerChipMark } from './components/PokerChipMark'
import { PokerClient } from './lib/pokerClient'
import {
  BOT_ACTION_BUBBLE_MS,
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

type SeatBubble =
  | { tone: 'thinking'; label: string }
  | { tone: 'action'; label: string }

function App() {
  const clientRef = useRef<PokerClient | null>(null)
  const initRequestRef = useRef(0)
  const botBubbleTimerRef = useRef<number | null>(null)
  const [snapshot, setSnapshot] = useState<WebSessionSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [botPresence, setBotPresence] = useState<BotPresence>({ state: 'idle' })

  useEffect(() => {
    void recreateClientAndInitialize(buildPlayerSessionConfig())

    return () => {
      clearBotBubbleTimer()
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
  const controlsLocked = busy || loading
  const botBubble: SeatBubble | null =
    botPresence.state === 'thinking'
      ? { tone: 'thinking', label: 'Thinking' }
      : botPresence.state === 'action'
        ? { tone: 'action', label: botPresence.label }
        : null

  const handleNewMatch = async () => {
    clearBotBubbleTimer()
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

    clearBotBubbleTimer()
    setBotPresence({ state: 'idle' })
    await runClientAction(async () => {
      const nextSnapshot = await client.resetHand()
      setSnapshot(nextSnapshot)
    })
  }

  const handleAction = async (actionId: string) => {
    const client = clientRef.current
    if (!client || !snapshot) {
      return
    }

    clearBotBubbleTimer()
    setBusy(true)
    setError(null)

    try {
      const afterHumanSnapshot = await client.applyHumanAction(actionId)
      setSnapshot(afterHumanSnapshot)

      if (afterHumanSnapshot.terminalSummary) {
        setBotPresence({ state: 'idle' })
        return
      }

      if (afterHumanSnapshot.currentActor !== afterHumanSnapshot.botSeat) {
        setBotPresence({ state: 'idle' })
        return
      }

      setBotPresence({ state: 'thinking' })
      await waitForNextPaint()
      const afterBotSnapshot = await client.advanceBot()
      setSnapshot(afterBotSnapshot)

      const botAction = extractBotActionLabel(afterHumanSnapshot, afterBotSnapshot)
      if (botAction) {
        showBotActionBubble(botAction)
        return
      }

      setBotPresence({ state: 'idle' })
    } catch (err) {
      setBotPresence({ state: 'idle' })
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
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

  function clearBotBubbleTimer(): void {
    if (botBubbleTimerRef.current === null) {
      return
    }
    window.clearTimeout(botBubbleTimerRef.current)
    botBubbleTimerRef.current = null
  }

  function showBotActionBubble(label: string): void {
    clearBotBubbleTimer()
    setBotPresence({ state: 'action', label })
    botBubbleTimerRef.current = window.setTimeout(() => {
      setBotPresence((current) =>
        current.state === 'action' && current.label === label
          ? { state: 'idle' }
          : current,
      )
      botBubbleTimerRef.current = null
    }, BOT_ACTION_BUBBLE_MS)
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
      setBotPresence({ state: 'idle' })
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top,rgba(53,113,85,0.35),transparent_28%),radial-gradient(circle_at_bottom,rgba(10,39,29,0.75),transparent_42%),linear-gradient(180deg,#07110d_0%,#071914_42%,#050a08_100%)] px-3 pb-8 pt-4 text-ivory-100 md:px-5 lg:px-6">
      <div className="mx-auto flex w-full max-w-[1120px] flex-col gap-4">
        <header className="flex flex-col gap-3 rounded-[1.65rem] border border-white/8 bg-black/16 px-4 py-4 shadow-[0_25px_80px_rgba(0,0,0,0.34)] backdrop-blur-sm md:flex-row md:items-center md:justify-between md:px-5">
          <div className="flex items-center gap-3">
            <span className="grid h-12 w-12 place-items-center rounded-2xl bg-[linear-gradient(180deg,rgba(15,51,38,0.96),rgba(10,22,18,0.92))] ring-1 ring-white/8 shadow-[0_14px_26px_rgba(0,0,0,0.28)]">
              <PokerChipMark className="h-8 w-8" />
            </span>
            <div>
              <p className="text-[0.72rem] font-medium uppercase tracking-[0.28em] text-gold-300/88">
                Heads-up no-limit hold&apos;em
              </p>
              <h1 className="mt-1.5 text-[clamp(1.9rem,3.2vw,3rem)] font-semibold leading-none tracking-[-0.05em] text-white">
                Heads-Up Hold&apos;em
              </h1>
              <p className="mt-1.5 max-w-xl text-sm leading-5 text-white/64">
                One table. One opponent. Pick your line.
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2.5 md:justify-end">
            {snapshot ? (
              <div className="flex flex-wrap gap-2">
                <InfoChip label={`Hand ${snapshot.handNumber}`} />
                <InfoChip label="0.5 / 1 blinds" />
              </div>
            ) : null}
            <button
              type="button"
              className="inline-flex min-h-10 items-center justify-center rounded-full bg-[linear-gradient(180deg,#efdca4,#d6ac56)] px-4 text-[0.78rem] font-semibold uppercase tracking-[0.16em] text-[#102116] shadow-[0_10px_24px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
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
              className="relative overflow-hidden rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(8,24,18,0.95),rgba(5,14,11,0.98))] px-3 py-4 shadow-[0_32px_120px_rgba(0,0,0,0.42)] md:px-6 md:py-5"
              aria-label="Poker table"
            >
              <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_16%,rgba(223,183,91,0.12),transparent_22%),radial-gradient(circle_at_50%_55%,rgba(52,133,92,0.36),transparent_38%)]" />
              <div className="pointer-events-none absolute inset-[7%] rounded-[999px] border-[12px] border-[#6e4d28] bg-[radial-gradient(circle_at_50%_35%,rgba(36,122,82,0.96),rgba(10,63,42,0.98)_60%,rgba(8,40,28,0.98)_100%)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.04),inset_0_40px_80px_rgba(255,255,255,0.06),inset_0_-60px_90px_rgba(0,0,0,0.34)]" />
              <div className="pointer-events-none absolute inset-[8.6%] rounded-[999px] border border-white/7" />

              <div className="relative z-10 grid min-h-[620px] grid-rows-[auto_1fr_auto_auto] gap-3 pt-5 md:min-h-[680px] md:pt-6">
                <PlayerSeat
                  label={botLabel()}
                  badge={seatBadge(villain.seat)}
                  stack={villain.stack}
                  cards={villain.holeCards}
                  hiddenCards={villain.holeCards.length === 0}
                  active={false}
                  mood={botPresence.state}
                  bubble={botBubble ?? undefined}
                  align="top"
                />

                <section className="flex flex-col items-center justify-center px-2 text-center">
                  <div className="rounded-full border border-white/10 bg-black/22 px-3 py-1.5 text-[0.68rem] font-medium uppercase tracking-[0.22em] text-gold-300/88 shadow-[0_10px_20px_rgba(0,0,0,0.22)]">
                    {snapshot.street}
                  </div>
                  <div className="mt-4 grid place-items-center rounded-full border border-gold-300/18 bg-black/18 px-7 py-3 shadow-[0_16px_34px_rgba(0,0,0,0.26)]">
                    <span className="text-[0.64rem] uppercase tracking-[0.22em] text-white/45">
                      Pot
                    </span>
                    <strong className="mt-1 text-xl font-semibold text-white md:text-[1.75rem]">
                      {formatBigBlinds(snapshot.pot)}
                    </strong>
                  </div>
                  <div className="mt-5 flex flex-wrap justify-center gap-2.5 md:gap-3" aria-label="Board cards">
                    {boardCards.map((card, index) => (
                      <PokerCard
                        key={`${card ?? 'empty'}-${index}`}
                        card={card}
                        className="h-[94px] w-[68px] md:h-[118px] md:w-[84px]"
                      />
                    ))}
                  </div>
                  <div className="mt-4 max-w-md">
                    <p className="text-[0.72rem] uppercase tracking-[0.26em] text-gold-300/80">
                      {outcome ? 'Showdown' : heroTurn ? 'Your move' : actionPrompt(snapshot, busy)}
                    </p>
                    <h2 className="mt-1.5 text-[1.7rem] font-semibold tracking-[-0.04em] text-white md:text-[2.05rem]">
                      {outcome ? outcome.headline : heroTurn ? 'Pick your action' : 'Watch the bot respond'}
                    </h2>
                    <p className="mt-1 text-sm leading-5 text-white/68">
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
                  className="mx-auto w-full max-w-[880px] rounded-[1.45rem] border border-white/8 bg-black/22 px-3 py-3 shadow-[0_18px_50px_rgba(0,0,0,0.28)] backdrop-blur-sm md:px-4"
                  aria-label="Action tray"
                >
                  <div className="flex min-h-[52px] items-center justify-center">
                    {outcome ? (
                      <button
                        type="button"
                        className="inline-flex min-h-11 items-center justify-center rounded-full bg-[linear-gradient(180deg,#efdca4,#d6ac56)] px-5 text-[0.78rem] font-semibold uppercase tracking-[0.16em] text-[#102116] shadow-[0_10px_24px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
                        onClick={handleNextHand}
                        disabled={controlsLocked}
                      >
                        Deal next hand
                      </button>
                    ) : (
                      <div className="flex w-full flex-wrap items-center justify-center gap-2.5">
                        {snapshot.legalActions.map((action) => (
                          <button
                            key={action.id}
                            type="button"
                            className="inline-flex min-h-11 items-center justify-center rounded-full border border-white/12 bg-white/8 px-4 text-[0.76rem] font-semibold uppercase tracking-[0.14em] text-white shadow-[0_8px_22px_rgba(0,0,0,0.18)] transition duration-150 hover:-translate-y-0.5 hover:border-gold-300/34 hover:bg-white/12 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
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
              className="group rounded-[1.45rem] border border-white/8 bg-black/14 px-4 py-3.5 text-white/82 shadow-[0_14px_36px_rgba(0,0,0,0.18)]"
              aria-label="Hand recap"
            >
              <summary className="flex cursor-pointer list-none items-center justify-between gap-3 text-sm font-semibold uppercase tracking-[0.18em] text-gold-300/84 marker:hidden">
                <span>Hand recap</span>
                <span className="text-[1.2rem] leading-none tracking-[0.08em] text-white/48 transition group-open:rotate-45">
                  +
                </span>
              </summary>
              <ol className="mt-3 grid gap-2 pl-5 text-sm leading-5 text-white/70 md:text-[0.92rem]">
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
  bubble?: SeatBubble
}) {
  const ringClass = props.active
    ? 'ring-2 ring-gold-300/48 shadow-[0_0_0_1px_rgba(255,255,255,0.06),0_20px_42px_rgba(0,0,0,0.24)]'
    : 'ring-1 ring-white/8 shadow-[0_16px_34px_rgba(0,0,0,0.22)]'

  return (
    <section
      className={joinClasses(
        'mx-auto flex w-full max-w-[390px] flex-col items-center gap-2.5 text-center',
        props.align === 'top' ? 'pt-1' : '',
      )}
      aria-label={props.label === heroLabel() ? 'Hero panel' : 'Bot panel'}
    >
      <div className="relative">
        {props.bubble ? (
          <ActionBubble tone={props.bubble.tone} label={props.bubble.label} />
        ) : null}
        <div
          className={joinClasses(
            'min-w-[220px] rounded-[1.45rem] border bg-black/24 px-4 py-3 backdrop-blur-sm transition duration-200',
            ringClass,
            props.mood === 'thinking' ? 'bot-thinking border-gold-300/32' : 'border-white/8',
          )}
        >
          <div className="flex items-center justify-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-full border border-white/10 bg-white/6 text-[0.75rem] font-semibold uppercase tracking-[0.14em] text-white/82">
              {props.label === heroLabel() ? 'You' : 'Bot'}
            </span>
            <div>
              <p className="text-[0.72rem] uppercase tracking-[0.22em] text-white/48">
                {props.badge}
              </p>
              <h3 className="mt-1 text-[1.05rem] font-semibold tracking-[-0.04em] text-white">
                {props.label}
              </h3>
            </div>
          </div>
          <p className="mt-2.5 text-[0.8rem] uppercase tracking-[0.18em] text-gold-300/84">
            {formatBigBlinds(props.stack)}
          </p>
        </div>
      </div>
      <div className={joinClasses('flex justify-center gap-2.5', props.align === 'top' ? 'pt-0.5' : 'pb-0.5')}>
        {(props.cards.length > 0 ? props.cards : [null, null]).map((card, index) => (
          <PokerCard
            key={`${card ?? 'hidden'}-${index}`}
            card={card}
            hidden={props.hiddenCards}
            tone={props.tone ?? 'table'}
            className={joinClasses(
              'h-[108px] w-[76px] transition duration-200 md:h-[132px] md:w-[94px]',
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

function ActionBubble(props: SeatBubble) {
  return (
    <div
      className={joinClasses(
        'action-bubble absolute left-1/2 top-0 z-10 inline-flex min-h-10 -translate-x-1/2 -translate-y-[52%] items-center gap-2 whitespace-nowrap rounded-full border px-3.5 py-2 text-[0.68rem] font-semibold uppercase tracking-[0.12em] shadow-[0_12px_30px_rgba(0,0,0,0.28)] md:text-[0.72rem]',
        props.tone === 'action' ? 'action-bubble-fade' : '',
        props.tone === 'thinking'
          ? 'border-gold-300/24 bg-black/62 text-gold-300'
          : 'border-gold-300/26 bg-black/55 text-gold-300',
      )}
    >
      <span>{props.label}</span>
      {props.tone === 'thinking' ? <ThinkingDots /> : null}
    </div>
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
    <span className="rounded-full border border-white/10 bg-white/6 px-3 py-1.5 text-[0.68rem] font-medium uppercase tracking-[0.18em] text-white/74">
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

async function waitForNextPaint(): Promise<void> {
  if (typeof window.requestAnimationFrame === 'function') {
    await new Promise<void>((resolve) => {
      window.requestAnimationFrame(() => resolve())
    })
    return
  }

  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, 0)
  })
}

function joinClasses(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}

export default App
