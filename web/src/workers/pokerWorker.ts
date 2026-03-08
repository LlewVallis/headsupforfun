/// <reference lib="webworker" />

import init, { PokerSession } from '../generated/gto-web/gto_web'
import type { PokerWorkerRequest, PokerWorkerResponse } from '../lib/pokerTypes'
import { PokerWorkerRuntime } from './pokerWorkerRuntime'

const worker = self as DedicatedWorkerGlobalScope
const forcedInitError = new URL(worker.location.href).searchParams.get(
  'forceInitError',
)
const runtime = new PokerWorkerRuntime({
  initWasm: () =>
    forcedInitError
      ? Promise.reject(new Error(forcedInitError))
      : init(),
  createSession: (config) => new PokerSession(config),
})

worker.addEventListener('message', (event: MessageEvent<PokerWorkerRequest>) => {
  void handleMessage(event.data)
})

async function handleMessage(message: PokerWorkerRequest): Promise<void> {
  respond(await runtime.handle(message))
}

function respond(message: PokerWorkerResponse): void {
  worker.postMessage(message)
}

export {}
