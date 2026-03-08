import { useEffect, useMemo, useRef, useState } from 'react'

import './App.css'
import { PokerClient } from './lib/pokerClient'
import type {
  WebBotMode,
  WebSessionConfig,
  WebSessionSnapshot,
} from './lib/pokerTypes'

const DEFAULT_SEED = '7'
const DEFAULT_BOT_MODE: WebBotMode = 'hybridFast'

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
    <main className="app-shell">
      <section className="hero-panel" aria-labelledby="app-title">
        <p className="eyebrow">Heads-up no-limit hold&apos;em</p>
        <div className="hero-header">
          <div>
            <h1 id="app-title">GTO Poker</h1>
            <p className="hero-copy">
              Static Vite frontend backed by the Rust engine and solver through a
              dedicated worker-hosted WASM session.
            </p>
          </div>
          <div className="hero-meta">
            <span className="hero-chip">Desktop first</span>
            <span className="hero-chip">Worker-backed WASM</span>
            <span className="hero-chip">Heads-up only</span>
          </div>
        </div>
      </section>

      <section className="control-panel" aria-label="Session controls">
        <label className="field">
          <span>Seed</span>
          <input
            value={seedInput}
            onChange={(event) => setSeedInput(event.target.value)}
            inputMode="numeric"
            pattern="[0-9]*"
            disabled={busy}
          />
        </label>

        <div className="mode-group" aria-label="Bot mode">
          {BOT_MODE_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={
                option.value === selectedBotMode
                  ? 'mode-button mode-button-active'
                  : 'mode-button'
              }
              onClick={() => setSelectedBotMode(option.value)}
              disabled={busy}
            >
              <span>{option.label}</span>
              <small>{option.detail}</small>
            </button>
          ))}
        </div>

        <div className="control-actions">
          <button
            type="button"
            className="primary-button"
            onClick={handleStartSession}
            disabled={busy}
          >
            {loading ? 'Initializing…' : 'Restart session'}
          </button>
          <button
            type="button"
            className="secondary-button"
            onClick={handleNewHand}
            disabled={busy || !snapshot}
          >
            Deal next hand
          </button>
        </div>
      </section>

      {error ? (
        <section className="error-panel" role="alert">
          <strong>Worker error</strong>
          <p>{error}</p>
        </section>
      ) : null}

      {snapshot && hero && villain ? (
        <>
          <section className="table-panel" aria-label="Poker table">
            <div className="seat seat-top">
              <div>
                <span className="seat-role">
                  {snapshot.botSeat === 'button' ? 'Bot · Button' : 'Bot · Big Blind'}
                </span>
                <strong>{formatHoleCards(villain.holeCards)}</strong>
              </div>
              <span className="seat-stack">{formatBigBlinds(villain.stack)}</span>
            </div>

            <div className="board">
              <span className="board-label">{snapshot.street}</span>
              <div className="card-row" aria-label="Board cards">
                {fillCardRow(snapshot.boardCards).map((card, index) => (
                  <span key={`${card}-${index}`} className="card-slot">
                    {card}
                  </span>
                ))}
              </div>
              <p className="pot-label">Pot: {formatBigBlinds(snapshot.pot)}</p>
              <p className="status-label">{snapshot.status}</p>
            </div>

            <div className="seat seat-bottom">
              <div>
                <span className="seat-role">
                  {snapshot.humanSeat === 'button'
                    ? 'Hero · Button'
                    : 'Hero · Big Blind'}
                </span>
                <strong>{formatHoleCards(hero.holeCards)}</strong>
              </div>
              <span className="seat-stack">{formatBigBlinds(hero.stack)}</span>
            </div>
          </section>

          <section className="status-grid">
            <article className="status-card" aria-label="Hand status">
              <h2>Hand status</h2>
              <dl className="stat-list">
                <div>
                  <dt>Hand</dt>
                  <dd>{snapshot.handNumber}</dd>
                </div>
                <div>
                  <dt>Bot mode</dt>
                  <dd>{readableBotMode(snapshot.botMode)}</dd>
                </div>
                <div>
                  <dt>Phase</dt>
                  <dd>{snapshot.phase}</dd>
                </div>
                <div>
                  <dt>Actor</dt>
                  <dd>{snapshot.currentActor ?? 'terminal'}</dd>
                </div>
              </dl>
              {snapshot.terminalSummary ? (
                <p className="terminal-copy">{snapshot.terminalSummary}</p>
              ) : null}
            </article>

            <article className="status-card" aria-label="Available actions">
              <h2>Available actions</h2>
              {snapshot.legalActions.length > 0 ? (
                <div className="action-grid">
                  {snapshot.legalActions.map((action) => (
                    <button
                      key={action.id}
                      type="button"
                      className="action-button"
                      onClick={() => handleAction(action.id)}
                      disabled={busy}
                    >
                      {action.label}
                    </button>
                  ))}
                </div>
              ) : (
                <p className="muted-copy">
                  {snapshot.terminalSummary
                    ? 'This hand is complete.'
                    : 'Waiting for the session worker.'}
                </p>
              )}
            </article>
          </section>

          <section className="history-panel" aria-label="Hand history">
            <div className="history-header">
              <h2>Hand history</h2>
              <span>{snapshot.history.length} events</span>
            </div>
            <ol className="history-list">
              {snapshot.history.map((entry, index) => (
                <li key={`${entry}-${index}`}>{entry}</li>
              ))}
            </ol>
          </section>
        </>
      ) : (
        <section className="loading-panel" aria-label="Loading state">
          <p>{loading ? 'Initializing poker worker…' : 'No active poker session.'}</p>
        </section>
      )}
    </main>
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

export default App
