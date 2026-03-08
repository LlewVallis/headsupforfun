/// <reference lib="webworker" />

import init, { PokerSession } from '../generated/gto-web/gto_web'
import type {
  PokerWorkerRequest,
  PokerWorkerResponse,
  WebSessionSnapshot,
} from '../lib/pokerTypes'

const worker = self as DedicatedWorkerGlobalScope
const wasmReady = init()
let session: PokerSession | null = null

worker.addEventListener('message', (event: MessageEvent<PokerWorkerRequest>) => {
  void handleMessage(event.data)
})

async function handleMessage(message: PokerWorkerRequest): Promise<void> {
  try {
    await wasmReady

    let snapshot: WebSessionSnapshot

    switch (message.type) {
      case 'init':
        session = new PokerSession(message.config)
        snapshot = session.snapshot() as WebSessionSnapshot
        break
      case 'snapshot':
        snapshot = requireSession().snapshot() as WebSessionSnapshot
        break
      case 'applyHumanAction':
        snapshot = requireSession().applyHumanAction(
          message.actionId,
        ) as WebSessionSnapshot
        break
      case 'resetHand':
        snapshot = requireSession().resetHand() as WebSessionSnapshot
        break
      default:
        throw new Error('Unknown poker worker command')
    }

    respond({ id: message.id, ok: true, snapshot })
  } catch (error) {
    respond({
      id: message.id,
      ok: false,
      error: error instanceof Error ? error.message : 'Unknown poker worker error',
    })
  }
}

function requireSession(): PokerSession {
  if (!session) {
    throw new Error('Poker session is not initialized')
  }
  return session
}

function respond(message: PokerWorkerResponse): void {
  worker.postMessage(message)
}

export {}
