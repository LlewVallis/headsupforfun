import { useEffect, useMemo, useRef, useState } from 'react'

import { PokerCard } from './components/PokerCard'
import { PokerChipMark } from './components/PokerChipMark'
import { preloadCardAssets } from './lib/cardAssets'
import { PokerClient } from './lib/pokerClient'
import { BOT_ACTION_BUBBLE_MS, BOARD_REVEAL_STEP_MS } from './lib/timing'
import {
  actionPrompt,
  botLabel,
  buildPlayerSessionConfig,
  completedMatchWinner,
  currentStreetBotActionLabel,
  extractBotActionLabel,
  formatBigBlinds,
  heroLabel,
  humanizeHistoryEntry,
  presentTerminalSummary,
  seatBadge,
} from './lib/presentation'
import { createTableAudio, cueForActionLabel } from './lib/tableAudio'
import type { WebSessionConfig, WebSessionSnapshot } from './lib/pokerTypes'

type BotPresence =
  | { state: 'idle' }
  | { state: 'thinking' }
  | { state: 'action'; label: string }

type SeatBubble =
  | { tone: 'thinking'; label: string }
  | { tone: 'action'; label: string }

type MatchRecord = {
  wins: number
  losses: number
}

function App() {
  const clientRef = useRef<PokerClient | null>(null)
  const initRequestRef = useRef(0)
  const botBubbleTimerRef = useRef<number | null>(null)
  const autoBotTurnKeyRef = useRef<string | null>(null)
  const previousSnapshotRef = useRef<WebSessionSnapshot | null>(null)
  const tableAudioRef = useRef<ReturnType<typeof createTableAudio> | null>(null)
  const [snapshot, setSnapshot] = useState<WebSessionSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [botPresence, setBotPresence] = useState<BotPresence>({ state: 'idle' })
  const [matchRecord, setMatchRecord] = useState<MatchRecord>({ wins: 0, losses: 0 })

  useEffect(() => {
    const disposeCardPreload = preloadCardAssets()
    tableAudioRef.current = createTableAudio()
    void recreateClientAndInitialize(buildPlayerSessionConfig())

    return () => {
      disposeCardPreload()
      clearBotBubbleTimer()
      disposeClient()
      tableAudioRef.current?.dispose()
      tableAudioRef.current = null
      previousSnapshotRef.current = null
    }
  }, [])

  useEffect(() => {
    const previousSnapshot = previousSnapshotRef.current
    if (previousSnapshot && snapshot && !previousSnapshot.matchOver && snapshot.matchOver) {
      const winner = completedMatchWinner(snapshot)
      if (winner === 'hero') {
        setMatchRecord((current) => ({ ...current, wins: current.wins + 1 }))
      } else if (winner === 'bot') {
        setMatchRecord((current) => ({ ...current, losses: current.losses + 1 }))
      }
    }

    previousSnapshotRef.current = snapshot
  }, [snapshot])

  useEffect(() => {
    if (!snapshot || loading || busy || error) {
      return
    }
    if (snapshot.terminalSummary || snapshot.currentActor !== snapshot.botSeat) {
      return
    }

    const snapshotKey = botTurnSnapshotKey(snapshot)
    if (autoBotTurnKeyRef.current === snapshotKey) {
      return
    }
    autoBotTurnKeyRef.current = snapshotKey

    void runBotTurnSequence(snapshot)
  }, [busy, error, loading, snapshot])

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

  const latestBotActionLabel = useMemo(() => {
    if (!snapshot) {
      return null
    }
    return currentStreetBotActionLabel(snapshot)
  }, [snapshot])

  const boardCards = fillBoardCards(snapshot?.boardCards ?? [])
  const heroTurn = snapshot?.currentActor === snapshot?.humanSeat && !snapshot?.terminalSummary
  const controlsLocked = busy || loading
  const heroReady = heroTurn && !controlsLocked
  const heroPromptTurn = heroTurn && (!busy || botPresence.state === 'action')
  const botBubble: SeatBubble | null =
    botPresence.state === 'thinking'
      ? { tone: 'thinking', label: 'Thinking' }
      : botPresence.state === 'action'
        ? { tone: 'action', label: botPresence.label }
        : null

  const handleNewMatch = async () => {
    clearBotBubbleTimer()
    autoBotTurnKeyRef.current = null
    setBotPresence({ state: 'idle' })
    await recreateClientAndInitialize(buildPlayerSessionConfig(), true)
  }

  const handleRetry = () => {
    window.location.reload()
  }

  const handleNextHand = async () => {
    const client = clientRef.current
    if (!client) {
      return
    }

    clearBotBubbleTimer()
    autoBotTurnKeyRef.current = null
    setBotPresence({ state: 'idle' })
    await runClientAction(async () => {
      const nextSnapshot = await client.resetHand()
      setSnapshot(nextSnapshot)
      tableAudioRef.current?.playCue('cardDeal')
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
      const botActsAfterHuman =
        !afterHumanSnapshot.terminalSummary &&
        afterHumanSnapshot.currentActor === afterHumanSnapshot.botSeat

      await transitionSnapshot(snapshot, afterHumanSnapshot)

      if (afterHumanSnapshot.terminalSummary) {
        setBotPresence({ state: 'idle' })
        return
      }

      if (!botActsAfterHuman) {
        setBotPresence({ state: 'idle' })
        return
      }

      await advanceBotUntilHumanTurn(afterHumanSnapshot, client)
    } catch (err) {
      setBotPresence({ state: 'idle' })
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  async function recreateClientAndInitialize(
    config: WebSessionConfig,
    playDealCue = false,
  ): Promise<void> {
    const previousClient = clientRef.current
    const client = new PokerClient()
    clientRef.current = client
    previousClient?.dispose()

    await initializeSession(client, config, playDealCue)
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
        current.state === 'action' && current.label === label ? { state: 'idle' } : current,
      )
      botBubbleTimerRef.current = null
    }, BOT_ACTION_BUBBLE_MS)
  }

  async function transitionSnapshot(
    previousSnapshot: WebSessionSnapshot,
    nextSnapshot: WebSessionSnapshot,
  ): Promise<void> {
    if (nextSnapshot.boardCards.length <= previousSnapshot.boardCards.length) {
      setSnapshot(nextSnapshot)
      await waitForNextPaint()
      return
    }

    const previousBoardCount = previousSnapshot.boardCards.length
    setSnapshot({
      ...nextSnapshot,
      boardCards: nextSnapshot.boardCards.slice(0, previousBoardCount),
    })
    await waitForNextPaint()

    for (
      let visibleCount = previousBoardCount + 1;
      visibleCount <= nextSnapshot.boardCards.length;
      visibleCount += 1
    ) {
      await sleep(BOARD_REVEAL_STEP_MS)
      tableAudioRef.current?.playCue('cardDeal')
      setSnapshot({
        ...nextSnapshot,
        boardCards: nextSnapshot.boardCards.slice(0, visibleCount),
      })
      await waitForNextPaint()
    }
  }

  async function initializeSession(
    client: PokerClient,
    config: WebSessionConfig,
    playDealCue: boolean,
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
      if (playDealCue) {
        tableAudioRef.current?.playCue('cardDeal')
      }
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

  async function runBotTurnSequence(startSnapshot: WebSessionSnapshot): Promise<void> {
    const client = clientRef.current
    if (!client) {
      return
    }

    clearBotBubbleTimer()
    setBusy(true)
    setError(null)

    try {
      await advanceBotUntilHumanTurn(startSnapshot, client)
    } catch (err) {
      setBotPresence({ state: 'idle' })
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  async function advanceBotUntilHumanTurn(
    startingSnapshot: WebSessionSnapshot,
    client: PokerClient,
  ): Promise<void> {
    let previousSnapshot = startingSnapshot
    while (
      !previousSnapshot.terminalSummary &&
      previousSnapshot.currentActor === previousSnapshot.botSeat
    ) {
      setBotPresence({ state: 'thinking' })
      await waitForNextPaint()
      const afterBotSnapshot = await client.advanceBot()
      await transitionSnapshot(previousSnapshot, afterBotSnapshot)
      const botAction = extractBotActionLabel(previousSnapshot, afterBotSnapshot)
      playActionCue(botAction)

      if (afterBotSnapshot.terminalSummary) {
        setBotPresence({ state: 'idle' })
        return
      }

      if (afterBotSnapshot.currentActor === afterBotSnapshot.botSeat) {
        setBotPresence({ state: 'idle' })
        previousSnapshot = afterBotSnapshot
        continue
      }

      if (botAction) {
        showBotActionBubble(botAction)
        return
      }

      setBotPresence({ state: 'idle' })
      return
    }

    setBotPresence({ state: 'idle' })
  }

  function playActionCue(label: string | null): void {
    const cue = cueForActionLabel(label)
    if (!cue) {
      return
    }

    tableAudioRef.current?.playCue(cue)
  }

  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_18%_10%,rgba(43,122,88,0.22),transparent_20%),radial-gradient(circle_at_82%_18%,rgba(31,92,67,0.18),transparent_22%),radial-gradient(circle_at_50%_120%,rgba(160,118,50,0.08),transparent_32%),linear-gradient(180deg,#020605_0%,#06130e_34%,#020705_68%,#010302_100%)] px-3 pb-10 pt-4 text-ivory-100 md:px-5 lg:px-6">
      <div className="mx-auto flex w-full max-w-[1140px] flex-col gap-4">
        <header className="relative overflow-hidden rounded-[1.4rem] border border-[#e8d8b6]/8 bg-[#07100d]/88 px-4 py-3 shadow-[0_20px_80px_rgba(0,0,0,0.5)] md:px-5">
          <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(231,199,115,0.05),transparent_24%,transparent_76%,rgba(231,199,115,0.04)),radial-gradient(circle_at_18%_0%,rgba(255,255,255,0.05),transparent_24%)]" />
          <div className="relative flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
            <div className="flex items-center gap-3">
              <span className="grid h-10 w-10 place-items-center rounded-xl border border-[#f2dfb8]/10 bg-[linear-gradient(180deg,#143627,#0a1712)] shadow-[0_12px_24px_rgba(0,0,0,0.35)]">
                <PokerChipMark className="h-7 w-7" />
              </span>
              <div>
                <p className="text-[0.66rem] font-medium uppercase tracking-[0.3em] text-gold-300/72">
                  Heads-up no-limit hold&apos;em
                </p>
                <h1 className="mt-1 text-[clamp(1.7rem,2.8vw,2.65rem)] font-semibold leading-none tracking-[-0.055em] text-white">
                  Heads-Up Hold&apos;em
                </h1>
                <p className="mt-1 max-w-xl text-[0.83rem] leading-5 text-white/50">
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
              <MatchRecordPanel wins={matchRecord.wins} losses={matchRecord.losses} />
            </div>
          </div>
        </header>

        {error ? (
          <section
            className="rounded-[1.35rem] border border-rose-200/12 bg-[linear-gradient(180deg,rgba(55,28,28,0.84),rgba(33,17,17,0.92))] px-5 py-4 text-white shadow-[0_18px_50px_rgba(0,0,0,0.35)]"
            role="alert"
          >
            <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div>
                <p className="text-[0.68rem] font-medium uppercase tracking-[0.24em] text-rose-100/76">
                  Page reload needed
                </p>
                <p className="mt-2 text-sm leading-6 text-white/74 md:text-base">
                  The table could not finish the last action. Reload the page to reopen the table.
                </p>
                <p className="mt-1 text-sm text-white/46">{error}</p>
              </div>
              <button
                type="button"
                className="inline-flex min-h-11 items-center justify-center rounded-full border border-white/12 bg-white/4 px-5 text-sm font-medium uppercase tracking-[0.16em] text-white transition duration-150 hover:bg-white/8 disabled:cursor-not-allowed disabled:opacity-55"
                onClick={handleRetry}
                disabled={busy}
              >
                Reload page
              </button>
            </div>
          </section>
        ) : null}

        {snapshot && hero && villain ? (
          <>
            <section
              className="relative px-1 py-2 md:px-2 md:py-3"
              aria-label="Poker table"
            >
              <div className="pointer-events-none absolute inset-x-[8%] inset-y-[3%] bg-[radial-gradient(circle_at_50%_0%,rgba(231,199,115,0.08),transparent_22%),radial-gradient(circle_at_50%_78%,rgba(9,32,23,0.28),transparent_40%)]" />
              <div className="table-felt-texture pointer-events-none absolute inset-[5%] rounded-[999px] border-[12px] border-[#7d5730] bg-[radial-gradient(circle_at_50%_34%,rgba(34,116,79,0.97),rgba(12,65,44,0.98)_58%,rgba(5,28,20,0.99)_100%)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.04),inset_0_28px_70px_rgba(255,255,255,0.05),inset_0_-70px_100px_rgba(0,0,0,0.4),0_24px_70px_rgba(0,0,0,0.42)]" />
              <div className="pointer-events-none absolute inset-[6.4%] rounded-[999px] border border-[#eed7ad]/10 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.03)]" />
              <div className="pointer-events-none absolute inset-x-[22%] bottom-[8%] h-10 rounded-full bg-[radial-gradient(circle,rgba(232,192,104,0.28),rgba(232,192,104,0.08)_45%,transparent_72%)] blur-xl" />

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
                  <div className="rounded-full border border-[#f0ddb7]/10 bg-[#08100d]/78 px-3 py-1.5 text-[0.66rem] font-medium uppercase tracking-[0.24em] text-gold-300/78 shadow-[0_8px_20px_rgba(0,0,0,0.22)]">
                    {snapshot.street}
                  </div>
                  <div className="mt-4 grid place-items-center rounded-[1.4rem] border border-[#f0ddb7]/10 bg-[linear-gradient(180deg,rgba(9,18,14,0.92),rgba(6,11,9,0.96))] px-7 py-3 shadow-[0_18px_34px_rgba(0,0,0,0.28)]">
                    <span className="text-[0.62rem] uppercase tracking-[0.24em] text-white/42">
                      Pot
                    </span>
                    <strong className="mt-1 text-xl font-semibold tracking-[-0.04em] text-white md:text-[1.75rem]">
                      {formatBigBlinds(snapshot.pot)}
                    </strong>
                  </div>
                  <div className="mt-5 flex flex-wrap justify-center gap-2 md:gap-3" aria-label="Board cards">
                    {boardCards.map((card, index) => (
                      <PokerCard
                        key={`${card ?? 'empty'}-${index}`}
                        card={card}
                        className={joinClasses(
                          'aspect-[167.0869141/242.6669922] h-[94px] md:h-[118px]',
                          index === 0 ? 'translate-y-[2px]' : '',
                          index === 2 ? 'translate-y-[-2px]' : '',
                          index === 4 ? 'translate-y-[1px]' : '',
                        )}
                      />
                    ))}
                  </div>
                  <div className="mt-4 max-w-md">
                    <p className="text-[0.68rem] uppercase tracking-[0.28em] text-gold-300/72">
                      {outcome ? 'Showdown' : heroPromptTurn ? 'Your move' : actionPrompt(snapshot, busy)}
                    </p>
                    <h2 className="mt-1.5 text-[1.55rem] font-semibold tracking-[-0.05em] text-white md:text-[1.95rem]">
                      {outcome ? outcome.headline : heroPromptTurn ? 'Pick your action' : 'Watch the bot respond'}
                    </h2>
                    <p className="mt-1 text-sm leading-5 text-white/62">
                      {outcome
                        ? outcome.detail
                        : heroPromptTurn
                          ? latestBotActionLabel
                            ? `${botLabel()} ${latestBotActionLabel.toLowerCase()}.`
                            : 'Choose from the available actions below the table.'
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
                  active={heroReady}
                  tone="hero"
                  align="bottom"
                />

                <section
                  className="mx-auto w-full max-w-[880px] rounded-[1.25rem] border border-[#f1ddb8]/8 bg-[linear-gradient(180deg,rgba(6,10,9,0.9),rgba(7,8,8,0.96))] px-3 py-3 shadow-[0_18px_50px_rgba(0,0,0,0.38)]"
                  aria-label="Action tray"
                >
                  <div className="flex min-h-[52px] items-center justify-center">
                    {outcome ? (
                      <button
                        type="button"
                        className="inline-flex min-h-11 items-center justify-center rounded-full border border-[#f0d48a]/38 bg-[linear-gradient(180deg,#e6ca82,#c79b43)] px-5 text-[0.78rem] font-semibold uppercase tracking-[0.16em] text-[#102116] shadow-[0_10px_24px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
                        onClick={snapshot.matchOver ? handleNewMatch : handleNextHand}
                        disabled={controlsLocked}
                      >
                        {snapshot.matchOver ? 'Start new match' : 'Deal next hand'}
                      </button>
                    ) : (
                      <div className="flex w-full flex-wrap items-center justify-center gap-2.5">
                        {snapshot.legalActions.map((action) => (
                          <ActionButton
                            key={action.id}
                            label={action.label}
                            disabled={controlsLocked}
                            onClick={() => handleAction(action.id)}
                          />
                        ))}
                      </div>
                    )}
                  </div>
                </section>
              </div>
            </section>

            <details
              className="group rounded-[1.2rem] border border-[#f0ddb7]/7 bg-[#050907]/88 px-4 py-3.5 text-white/78 shadow-[0_14px_36px_rgba(0,0,0,0.24)]"
              aria-label="Hand recap"
            >
              <summary className="flex cursor-pointer list-none items-center justify-between gap-3 text-sm font-semibold uppercase tracking-[0.18em] text-gold-300/78 marker:hidden">
                <span>Hand recap</span>
                <span className="text-[1.2rem] leading-none tracking-[0.08em] text-white/42 transition group-open:rotate-45">
                  +
                </span>
              </summary>
              <ol className="mt-3 grid gap-2 pl-5 text-sm leading-5 text-white/66 md:text-[0.92rem]">
                {recapEntries.map((entry, index) => (
                  <li key={`${entry}-${index}`}>{entry}</li>
                ))}
              </ol>
            </details>
          </>
        ) : (
          <section className="rounded-[1.45rem] border border-[#f0ddb7]/7 bg-[#050907]/92 px-6 py-8 text-center shadow-[0_24px_80px_rgba(0,0,0,0.34)]">
            <div className="mx-auto flex max-w-md flex-col items-center gap-4">
              <span className="grid h-16 w-16 place-items-center rounded-full border border-[#f0ddb7]/10 bg-[linear-gradient(180deg,#102118,#09110d)] shadow-[0_12px_28px_rgba(0,0,0,0.3)]">
                <PokerChipMark className="h-10 w-10 opacity-85" />
              </span>
              <div>
                <h2 className="text-2xl font-semibold tracking-[-0.04em] text-white">
                  Opening the table
                </h2>
                <p className="mt-2 text-sm leading-6 text-white/58 md:text-base">
                  {loading ? 'Shuffling cards and syncing the bot.' : 'No active poker table.'}
                </p>
              </div>
            </div>
          </section>
        )}

        <footer
          className="border-t border-white/6 px-1 pt-3 text-center text-[0.72rem] leading-5 text-white/42"
          aria-label="Credits"
        >
          <p>
            Cards:{' '}
            <a
              className="text-white/56 transition hover:text-gold-300/86"
              href="https://github.com/notpeter/Vector-Playing-Cards"
              target="_blank"
              rel="noreferrer"
            >
              Vector-Playing-Cards
            </a>{' '}
            by notpeter, with original artwork credited upstream to Byron Knoll. Audio:{' '}
            <a
              className="text-white/56 transition hover:text-gold-300/86"
              href="https://github.com/murbar/jacks-or-better/tree/master/src/audio"
              target="_blank"
              rel="noreferrer"
            >
              murbar/jacks-or-better
            </a>{' '}
            by Joel Bartlett, used under the MIT License.
          </p>
        </footer>
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
    ? 'ring-2 ring-gold-300/36 shadow-[0_0_0_1px_rgba(255,255,255,0.04),0_20px_42px_rgba(0,0,0,0.34)]'
    : 'ring-1 ring-[#f0ddb7]/8 shadow-[0_16px_34px_rgba(0,0,0,0.3)]'

  return (
    <section
      className={joinClasses(
        'mx-auto flex w-full max-w-[390px] flex-col items-center gap-2.5 text-center',
        props.align === 'top' ? 'pt-1' : '',
      )}
      aria-label={props.label === heroLabel() ? 'Hero panel' : 'Bot panel'}
    >
      <div className="relative">
        {props.bubble ? <ActionBubble tone={props.bubble.tone} label={props.bubble.label} /> : null}
        <div
          className={joinClasses(
            'min-w-[220px] rounded-[1.2rem] border bg-[linear-gradient(180deg,rgba(9,16,13,0.94),rgba(5,9,7,0.96))] px-4 py-3 transition duration-200',
            ringClass,
            props.mood === 'thinking' ? 'bot-thinking border-gold-300/26' : 'border-[#f0ddb7]/8',
          )}
        >
          <div className="flex items-center justify-center gap-3">
            <span className="grid h-10 w-10 place-items-center rounded-full border border-[#f0ddb7]/10 bg-[#102118] text-[0.75rem] font-semibold uppercase tracking-[0.14em] text-white/76">
              {props.label === heroLabel() ? 'You' : 'Bot'}
            </span>
            <div>
              <p className="text-[0.68rem] uppercase tracking-[0.22em] text-white/42">
                {props.badge}
              </p>
              <h3 className="mt-1 text-[1rem] font-semibold tracking-[-0.04em] text-white">
                {props.label}
              </h3>
            </div>
          </div>
          <p className="mt-2.5 text-[0.76rem] uppercase tracking-[0.2em] text-gold-300/82">
            {formatBigBlinds(props.stack)}
          </p>
        </div>
      </div>
      <div
        className={joinClasses(
          'flex justify-center gap-2',
          props.align === 'top' ? 'pt-0.5' : 'pb-1',
        )}
      >
        {(props.cards.length > 0 ? props.cards : [null, null]).map((card, index) => (
          <PokerCard
            key={`${card ?? 'hidden'}-${index}`}
            card={card}
            hidden={props.hiddenCards}
            tone={props.tone ?? 'table'}
            className={joinClasses(
              'aspect-[167.0869141/242.6669922] h-[108px] transition duration-200 md:h-[132px]',
              props.align === 'bottom' && index === 0 ? 'rotate-[-5deg] translate-x-2 translate-y-1' : '',
              props.align === 'bottom' && index === 1 ? 'rotate-[6deg] -translate-x-2 -translate-y-1' : '',
              props.align === 'top' && index === 0 ? 'rotate-[4deg] translate-x-2' : '',
              props.align === 'top' && index === 1 ? 'rotate-[-4deg] -translate-x-2' : '',
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
          ? 'border-gold-300/22 bg-[#090e0d]/92 text-gold-300'
          : 'border-gold-300/24 bg-[#12110c]/92 text-gold-300',
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
    <span className="rounded-full border border-[#f0ddb7]/8 bg-[#0b1511] px-3 py-1.5 text-[0.64rem] font-medium uppercase tracking-[0.2em] text-white/62">
      {props.label}
    </span>
  )
}

function MatchRecordPanel(props: MatchRecord) {
  return (
    <section
      className="rounded-[1rem] border border-[#f0ddb7]/8 bg-[linear-gradient(180deg,rgba(10,17,14,0.94),rgba(6,10,8,0.96))] px-3.5 py-2 text-white/78 shadow-[0_12px_24px_rgba(0,0,0,0.24)]"
      aria-label="Match record"
    >
      <div className="flex items-baseline gap-3.5">
        <ScoreValue label="Wins" value={props.wins} />
        <span className="h-4 w-px bg-white/10" aria-hidden="true" />
        <ScoreValue label="Losses" value={props.losses} />
      </div>
    </section>
  )
}

function ScoreValue(props: { label: string; value: number }) {
  return (
    <div className="flex items-baseline gap-1.5">
      <span className="text-[0.58rem] uppercase tracking-[0.2em] text-white/38">
        {props.label}
      </span>
      <strong className="text-[1.05rem] font-semibold tracking-[-0.04em] text-white tabular-nums">
        {props.value}
      </strong>
    </div>
  )
}

function ActionButton(props: {
  label: string
  disabled: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      className={joinClasses(
        'inline-flex min-h-11 items-center justify-center rounded-full border px-4 text-[0.74rem] font-semibold uppercase tracking-[0.14em] shadow-[0_8px_22px_rgba(0,0,0,0.24)] transition duration-150 hover:-translate-y-0.5 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0',
        actionButtonTone(props.label),
      )}
      onClick={props.onClick}
      disabled={props.disabled}
    >
      {props.label}
    </button>
  )
}

function actionButtonTone(label: string): string {
  const normalized = label.trim().toLowerCase()
  if (normalized.startsWith('fold')) {
    return 'border-white/7 bg-[#121312] text-white/72 hover:border-white/14 hover:bg-[#191b19]'
  }
  if (normalized.includes('all-in')) {
    return 'border-gold-300/28 bg-[linear-gradient(180deg,rgba(61,42,14,0.9),rgba(34,22,8,0.94))] text-gold-300 hover:border-gold-300/42 hover:bg-[linear-gradient(180deg,rgba(76,53,18,0.94),rgba(43,28,9,0.96))]'
  }
  if (normalized.startsWith('call') || normalized.startsWith('check')) {
    return 'border-[#eed7ad]/10 bg-[#0d1613] text-white hover:border-[#eed7ad]/20 hover:bg-[#12201b]'
  }
  return 'border-[#eed7ad]/14 bg-[linear-gradient(180deg,rgba(18,30,24,0.98),rgba(11,18,14,0.96))] text-white hover:border-gold-300/28 hover:bg-[linear-gradient(180deg,rgba(24,40,32,0.98),rgba(13,21,16,0.96))]'
}

function botTurnSnapshotKey(snapshot: WebSessionSnapshot): string {
  return [
    snapshot.handNumber,
    snapshot.street,
    snapshot.currentActor ?? 'terminal',
    snapshot.history.length,
    snapshot.button.stack,
    snapshot.bigBlind.stack,
  ].join(':')
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

async function sleep(durationMs: number): Promise<void> {
  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, durationMs)
  })
}

function joinClasses(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}

export default App
