import { describe, expect, it } from 'vitest'

import { cardAssetCount, cardAssetUrl, preloadCardAssets } from './cardAssets'

describe('card asset preloading', () => {
  it('preloads every bundled card asset once', () => {
    const createdImages: Array<{ src: string; decoding?: 'sync' | 'async' | 'auto' }> = []

    class FakeImage {
      src = ''
      decoding: 'sync' | 'async' | 'auto' = 'auto'

      constructor() {
        createdImages.push(this)
      }
    }

    const dispose = preloadCardAssets({ Image: FakeImage } as unknown as typeof globalThis)

    expect(createdImages).toHaveLength(cardAssetCount())
    expect(createdImages.every((image) => image.decoding === 'async')).toBe(true)
    expect(createdImages.some((image) => image.src === cardAssetUrl('BACK'))).toBe(true)
    expect(createdImages.some((image) => image.src === cardAssetUrl('AS'))).toBe(true)
    expect(createdImages.some((image) => image.src === cardAssetUrl('KD'))).toBe(true)

    dispose()

    expect(createdImages.every((image) => image.src === '')).toBe(true)
  })

  it('no-ops when the host cannot construct images', () => {
    expect(() => preloadCardAssets({} as typeof globalThis)).not.toThrow()
  })
})
