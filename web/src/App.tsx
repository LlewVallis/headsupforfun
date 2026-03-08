import { useEffect, useMemo, useRef, useState } from 'react'

import { PokerClient } from './lib/pokerClient'
import type {
  WebBotMode,
  WebSessionConfig,
  WebSessionSnapshot,
} from './lib/pokerTypes'

const DEFAULT_SEED = '7'
const DEFAULT_BOT_MODE: WebBotMode = 'hybridFast'
const PANEL_CLASS =
  'border border-amber-100/15 bg-[linear-gradient(180deg,rgba(18,58,43,0.94),rgba(10,31,24,0.98))] shadow-[0_24px_80px_rgba(0,0,0,0.28)] backdrop-blur-sm'
const MODE_BUTTON_BASE_CLASS =
  'group min-h-24 rounded-[1.15rem] border px-4 py-4 text-left transition duration-150 disabled:cursor-not-allowed disabled:opacity-55'
const SECONDARY_BUTTON_CLASS =
  'inline-flex min-h-12 min-w-44 items-center justify-center rounded-full border border-amber-100/18 bg-amber-50/8 px-5 text-sm tracking-[0.12em] uppercase text-amber-50 transition duration-150 hover:-translate-y-0.5 hover:border-amber-200/30 hover:bg-amber-50/12 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0'
const ACTION_BUTTON_CLASS =
  'rounded-full border border-amber-100/18 bg-amber-50/8 px-4 py-3 text-sm font-medium tracking-[0.08em] uppercase text-amber-50 transition duration-150 hover:-translate-y-0.5 hover:border-amber-200/30 hover:bg-amber-50/12 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0'

const BOT_MODE_OPTIONS: Array<{
  value: WebBotMode
  label: string
  detail: string
}> = [
  {
    value: 'blueprint',
    label: 'Blueprint',
    detail: 'Fastest cached strategy path.',
  },
  {
    value: 'hybridFast',
    label: 'Hybrid Fast',
    detail: 'Runtime solving on later streets with responsive defaults.',
  },
  {
    value: 'hybridPlay',
    label: 'Hybrid Play',
    detail: 'Stronger postflop runtime solving with a slower response budget.',
  },
]

function App() {
  const clientRef = useRef<PokerClient | null>(null)
  const initRequestRef = useRef(0)
  const [seedInput, setSeedInput] = useState(DEFAULT_SEED)
  const [selectedBotMode, setSelectedBotMode] =
    useState<WebBotMode>(DEFAULT_BOT_MODE)
  const [snapshot, setSnapshot] = useState<WebSessionSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const client = new PokerClient()
    clientRef.current = client

    void initializeSession(client, {
      seed: parseSeed(DEFAULT_SEED),
      humanSeat: 'button',
      botMode: DEFAULT_BOT_MODE,
    })

    return () => {
      clientRef.current = null
      client.dispose()
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

  const handleStartSession = async () => {
    const client = clientRef.current
    if (!client) {
      return
    }

    await initializeSession(client, {
      seed: parseSeed(seedInput),
      humanSeat: 'button',
      botMode: selectedBotMode,
    })
  }

  const handleNewHand = async () => {
    const client = clientRef.current
    if (!client) {
      return
    }

    await runClientAction(async () => {
      const nextSnapshot = await client.resetHand()
      setSnapshot(nextSnapshot)
    })
  }

  const handleAction = async (actionId: string) => {
    const client = clientRef.current
    if (!client) {
      return
    }

    await runClientAction(async () => {
      const nextSnapshot = await client.applyHumanAction(actionId)
      setSnapshot(nextSnapshot)
    })
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
      setSelectedBotMode(config.botMode)
      setSeedInput(String(config.seed))
    } catch (err) {
      if (clientRef.current !== client || initRequestRef.current !== requestId) {
        return
      }
      setError(toErrorMessage(err))
      setSnapshot(null)
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
      setError(toErrorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-[1180px] flex-col px-4 pb-16 pt-6 md:px-6 md:pt-8">
      <section
        className={`${PANEL_CLASS} rounded-[2rem_2rem_1rem_1rem] px-6 py-7 md:px-8 md:py-8`}
        aria-labelledby="app-title"
      >
        <p className="m-0 text-[0.78rem] uppercase tracking-[0.22em] text-amber-200">
          Heads-up no-limit hold&apos;em
        </p>
        <div className="mt-3 grid gap-6 xl:grid-cols-[minmax(0,1fr)_auto] xl:items-start">
          <div>
            <h1
              id="app-title"
              className="m-0 text-[clamp(2.85rem,5vw,4.85rem)] leading-[0.92] tracking-[-0.06em] text-amber-50"
            >
              GTO Poker
            </h1>
            <p className="mt-4 max-w-3xl text-base leading-7 text-stone-200/82 md:text-lg">
              Static Vite frontend backed by the Rust engine and solver through
              a dedicated worker-hosted WASM session.
            </p>
          </div>
          <div className="flex flex-wrap gap-3 xl:max-w-sm xl:justify-end">
            {['Desktop first', 'Worker-backed WASM', 'Heads-up only'].map(
              (chip) => (
                <span
                  key={chip}
                  className="rounded-full border border-amber-100/20 bg-amber-50/8 px-4 py-2 text-[0.78rem] uppercase tracking-[0.16em] text-amber-50/90"
                >
                  {chip}
                </span>
              ),
            )}
          </div>
        </div>
      </section>

      <section
        className={`${PANEL_CLASS} mt-4 grid gap-4 rounded-2xl px-5 py-5 md:grid-cols-[12rem_minmax(0,1fr)] xl:grid-cols-[12rem_minmax(0,1fr)_auto] xl:items-start`}
        aria-label="Session controls"
      >
        <label className="grid gap-2 text-[0.82rem] uppercase tracking-[0.16em] text-amber-50/85">
          <span>Seed</span>
          <input
            className="h-12 rounded-2xl border border-amber-100/20 bg-black/25 px-4 text-base tracking-normal text-amber-50 outline-none transition placeholder:text-amber-50/35 focus:border-amber-200/35 focus:bg-black/35"
            value={seedInput}
            onChange={(event) => setSeedInput(event.target.value)}
            inputMode="numeric"
            pattern="[0-9]*"
            disabled={busy}
          />
        </label>

        <div
          className="grid gap-3 md:grid-cols-3"
          aria-label="Bot mode"
        >
          {BOT_MODE_OPTIONS.map((option) => {
            const isActive = option.value === selectedBotMode
            return (
              <button
                key={option.value}
                type="button"
                className={joinClasses(
                  MODE_BUTTON_BASE_CLASS,
                  isActive
                    ? 'border-amber-200/35 bg-amber-200/16 text-amber-50 shadow-[0_18px_45px_rgba(0,0,0,0.24)]'
                    : 'border-amber-100/16 bg-amber-50/6 text-amber-50 hover:-translate-y-0.5 hover:border-amber-200/30 hover:bg-amber-50/10',
                )}
                onClick={() => setSelectedBotMode(option.value)}
                disabled={busy}
              >
                <span className="block text-[0.92rem] font-semibold uppercase tracking-[0.12em]">
                  {option.label}
                </span>
                <small className="mt-2 block text-sm leading-6 text-stone-200/68">
                  {option.detail}
                </small>
              </button>
            )
          })}
        </div>

        <div className="grid gap-3 xl:w-48">
          <button
            type="button"
            className="inline-flex min-h-12 items-center justify-center rounded-full bg-[linear-gradient(180deg,#dec37d,#b38935)] px-5 text-sm font-semibold uppercase tracking-[0.14em] text-[#102116] transition duration-150 hover:-translate-y-0.5 hover:brightness-105 disabled:cursor-not-allowed disabled:opacity-55 disabled:hover:translate-y-0"
            onClick={handleStartSession}
            disabled={busy}
          >
            {loading ? 'Initializing...' : 'Restart session'}
          </button>
          <button
            type="button"
            className={SECONDARY_BUTTON_CLASS}
            onClick={handleNewHand}
            disabled={busy || !snapshot}
          >
            Deal next hand
          </button>
        </div>
      </section>

      {error ? (
        <section
          className={`${PANEL_CLASS} mt-4 rounded-2xl border-rose-300/20 px-5 py-4`}
          role="alert"
        >
          <strong className="text-sm uppercase tracking-[0.16em] text-rose-100">
            Worker error
          </strong>
          <p className="mt-2 text-sm leading-6 text-stone-200/82">{error}</p>
        </section>
      ) : null}

      {snapshot && hero && villain ? (
        <>
          <section
            className={`${PANEL_CLASS} relative mt-4 overflow-hidden rounded-[1rem_1rem_2rem_2rem] px-5 py-6 md:px-8 md:py-8`}
            aria-label="Poker table"
          >
            <div className="pointer-events-none absolute inset-6 rounded-[999px] bg-[radial-gradient(circle_at_top,rgba(255,255,255,0.12),transparent_38%),radial-gradient(circle_at_center,rgba(33,126,86,0.9),rgba(9,56,37,0.96))] shadow-[inset_0_0_0_18px_rgba(93,56,30,0.75),inset_0_0_0_20px_rgba(224,196,134,0.14)] md:inset-8" />
            <div className="relative z-10 grid min-h-[22rem] items-center gap-8 md:min-h-[24rem]">
              <SeatPanel
                role={snapshot.botSeat === 'button' ? 'Bot - Button' : 'Bot - Big Blind'}
                holeCards={villain.holeCards}
                stack={villain.stack}
              />

              <div className="text-center">
                <span className="block text-[0.8rem] uppercase tracking-[0.18em] text-amber-200">
                  {snapshot.street}
                </span>
                <div
                  className="mt-4 flex justify-center gap-2.5 md:gap-3.5"
                  aria-label="Board cards"
                >
                  {fillCardRow(snapshot.boardCards).map((card, index) => (
                    <span
                      key={`${card}-${index}`}
                      className="grid h-16 w-11 place-items-center rounded-2xl border border-amber-100/24 bg-[linear-gradient(180deg,rgba(247,241,226,0.96),rgba(216,208,190,0.9))] text-base font-bold text-slate-900 shadow-[0_12px_24px_rgba(0,0,0,0.18)] md:h-[5.5rem] md:w-16 md:text-lg"
                    >
                      {card}
                    </span>
                  ))}
                </div>
                <p className="mt-3 text-sm uppercase tracking-[0.14em] text-stone-200/76">
                  Pot: {formatBigBlinds(snapshot.pot)}
                </p>
                <p className="mt-2 text-sm leading-6 text-stone-100/82">
                  {snapshot.status}
                </p>
              </div>

              <SeatPanel
                role={
                  snapshot.humanSeat === 'button'
                    ? 'Hero - Button'
                    : 'Hero - Big Blind'
                }
                holeCards={hero.holeCards}
                stack={hero.stack}
              />
            </div>
          </section>

          <section className="mt-4 grid gap-4 xl:grid-cols-2">
            <article
              className={`${PANEL_CLASS} rounded-[1.5rem] px-5 py-5 md:px-6`}
              aria-label="Hand status"
            >
              <h2 className="m-0 text-2xl text-amber-50">Hand status</h2>
              <dl className="mt-4 grid gap-3 sm:grid-cols-2">
                <StatCard label="Hand" value={String(snapshot.handNumber)} />
                <StatCard
                  label="Bot mode"
                  value={readableBotMode(snapshot.botMode)}
                />
                <StatCard label="Phase" value={snapshot.phase} />
                <StatCard
                  label="Actor"
                  value={snapshot.currentActor ?? 'terminal'}
                />
              </dl>
              {snapshot.terminalSummary ? (
                <p className="mt-4 max-w-2xl text-sm leading-7 text-stone-200/80 md:text-base">
                  {snapshot.terminalSummary}
                </p>
              ) : null}
            </article>

            <article
              className={`${PANEL_CLASS} rounded-[1.5rem] px-5 py-5 md:px-6`}
              aria-label="Available actions"
            >
              <h2 className="m-0 text-2xl text-amber-50">Available actions</h2>
              {snapshot.legalActions.length > 0 ? (
                <div className="mt-4 flex flex-wrap gap-3">
                  {snapshot.legalActions.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      className={ACTION_BUTTON_CLASS}
                      onClick={() => handleAction(action.id)}
                      disabled={busy}
                    >
                      {action.label}
                    </button>
                  ))}
                </div>
              ) : (
                <p className="mt-4 text-sm leading-7 text-stone-200/80 md:text-base">
                  {snapshot.terminalSummary
                    ? 'This hand is complete.'
                    : 'Waiting for the session worker.'}
                </p>
              )}
            </article>
          </section>

          <section
            className={`${PANEL_CLASS} mt-4 rounded-[1.5rem] px-5 py-5 md:px-6`}
            aria-label="Hand history"
          >
            <div className="flex flex-wrap items-baseline justify-between gap-3">
              <h2 className="m-0 text-2xl text-amber-50">Hand history</h2>
              <span className="text-sm uppercase tracking-[0.12em] text-stone-200/62">
                {snapshot.history.length} events
              </span>
            </div>
            <ol className="mt-4 grid gap-2.5 pl-5 text-sm leading-6 text-stone-200/84 md:text-base">
              {snapshot.history.map((entry, index) => (
                <li key={`${entry}-${index}`}>{entry}</li>
              ))}
            </ol>
          </section>
        </>
      ) : (
        <section
          className={`${PANEL_CLASS} mt-4 rounded-[1.5rem] px-5 py-5`}
          aria-label="Loading state"
        >
          <p className="m-0 text-base leading-7 text-stone-200/82">
            {loading
              ? 'Initializing poker worker...'
              : 'No active poker session.'}
          </p>
        </section>
      )}
    </main>
  )
}

function SeatPanel(props: {
  role: string
  holeCards: string[]
  stack: number
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <span className="block text-[0.8rem] uppercase tracking-[0.18em] text-amber-50/82">
          {props.role}
        </span>
        <strong className="mt-1 block text-lg font-semibold text-amber-50">
          {formatHoleCards(props.holeCards)}
        </strong>
      </div>
      <span className="text-sm uppercase tracking-[0.14em] text-stone-200/76">
        {formatBigBlinds(props.stack)}
      </span>
    </div>
  )
}

function StatCard(props: { label: string; value: string }) {
  return (
    <div className="rounded-3xl bg-amber-50/5 px-4 py-4">
      <dt className="text-[0.76rem] uppercase tracking-[0.18em] text-amber-50/74">
        {props.label}
      </dt>
      <dd className="mt-2 text-base text-amber-50">{props.value}</dd>
    </div>
  )
}

function parseSeed(value: string): number {
  const parsed = Number.parseInt(value, 10)
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    return Number.parseInt(DEFAULT_SEED, 10)
  }
  return parsed
}

function toErrorMessage(value: unknown): string {
  return value instanceof Error ? value.message : 'Unknown frontend error'
}

function fillCardRow(cards: string[]): string[] {
  const row = [...cards]
  while (row.length < 5) {
    row.push('??')
  }
  return row
}

function formatHoleCards(cards: string[]): string {
  if (cards.length === 0) {
    return 'Hidden'
  }
  return cards.join(' ')
}

function readableBotMode(botMode: WebBotMode): string {
  switch (botMode) {
    case 'blueprint':
      return 'Blueprint'
    case 'hybridFast':
      return 'Hybrid Fast'
    case 'hybridPlay':
      return 'Hybrid Play'
  }
}

function formatBigBlinds(chips: number): string {
  return `${(chips / 100).toFixed(1)} bb`
}

function joinClasses(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}

export default App
