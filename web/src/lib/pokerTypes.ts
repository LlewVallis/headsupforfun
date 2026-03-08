export type WebSeat = 'button' | 'bigBlind'
export type WebBotMode = 'blueprint' | 'hybridFast' | 'hybridPlay'

export interface WebSessionConfig {
  seed: number
  humanSeat: WebSeat
  botMode: WebBotMode
  blueprintArtifactJson?: string | null
}

export interface WebActionChoice {
  id: string
  label: string
}

export interface WebPlayerSnapshot {
  seat: WebSeat
  stack: number
  totalContribution: number
  streetContribution: number
  folded: boolean
  holeCards: string[]
}

export interface WebSessionSnapshot {
  handNumber: number
  humanSeat: WebSeat
  botSeat: WebSeat
  botMode: WebBotMode
  street: string
  phase: string
  currentActor: WebSeat | null
  pot: number
  boardCards: string[]
  button: WebPlayerSnapshot
  bigBlind: WebPlayerSnapshot
  legalActions: WebActionChoice[]
  history: string[]
  status: string
  terminalSummary: string | null
}

export type PokerWorkerCommand =
  | { type: 'init'; config: WebSessionConfig; forceInitError?: string | null }
  | { type: 'snapshot' }
  | { type: 'applyHumanAction'; actionId: string }
  | { type: 'advanceBot'; forceActionDelayMs?: number | null }
  | { type: 'resetHand' }

export type PokerWorkerRequest =
  | {
      id: number
      type: 'init'
      config: WebSessionConfig
      forceInitError?: string | null
    }
  | { id: number; type: 'snapshot' }
  | {
      id: number
      type: 'applyHumanAction'
      actionId: string
    }
  | { id: number; type: 'advanceBot'; forceActionDelayMs?: number | null }
  | { id: number; type: 'resetHand' }

export type PokerWorkerResponse =
  | { id: number; ok: true; snapshot: WebSessionSnapshot }
  | { id: number; ok: false; error: string }
