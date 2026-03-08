import './App.css'

function App() {
  return (
    <main className="app-shell">
      <section className="hero-panel" aria-labelledby="app-title">
        <p className="eyebrow">Heads-up no-limit hold&apos;em</p>
        <h1 id="app-title">GTO Poker</h1>
        <p className="hero-copy">
          Browser client bootstrap for the Rust solver stack. The WASM worker
          session and real table UI land next.
        </p>
      </section>

      <section className="table-panel" aria-label="Poker table preview">
        <div className="seat seat-top">
          <span className="seat-label">Bot</span>
          <span className="seat-stack">100.0 bb</span>
        </div>

        <div className="board">
          <span className="board-label">Board</span>
          <div className="card-row" aria-label="Board cards">
            <span className="card-slot">?</span>
            <span className="card-slot">?</span>
            <span className="card-slot">?</span>
            <span className="card-slot">?</span>
            <span className="card-slot">?</span>
          </div>
          <p className="pot-label">Pot: 1.5 bb</p>
        </div>

        <div className="seat seat-bottom">
          <span className="seat-label">Hero</span>
          <span className="seat-stack">100.0 bb</span>
        </div>
      </section>

      <section className="status-grid">
        <article className="status-card" aria-label="Frontend status">
          <h2>Frontend status</h2>
          <p>Vite, React, and TypeScript are wired.</p>
          <p>Vitest and Playwright are part of the baseline test setup.</p>
        </article>

        <article className="status-card" aria-label="Next implementation step">
          <h2>Next implementation step</h2>
          <p>Add the `gto-web` WASM adapter and the dedicated poker worker.</p>
        </article>
      </section>
    </main>
  )
}

export default App
