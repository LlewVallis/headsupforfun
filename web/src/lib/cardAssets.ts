const assetModules = import.meta.glob('../assets/cards/*.svg', {
  eager: true,
  import: 'default',
}) as Record<string, string>

export const cardAssetByCode = Object.freeze(
  Object.fromEntries(
    Object.entries(assetModules).map(([path, assetUrl]) => {
      const filename = path.split('/').pop()?.replace('.svg', '') ?? path
      return [filename.toUpperCase(), assetUrl]
    }),
  ) as Record<string, string>,
)

const cardAssetUrls = Object.freeze(Array.from(new Set(Object.values(cardAssetByCode))))

type ImageLike = {
  src: string
  decoding?: 'sync' | 'async' | 'auto'
}

type ImageConstructor = new () => ImageLike

type HostWithImage = typeof globalThis & {
  Image?: ImageConstructor
}

export function preloadCardAssets(host: HostWithImage = globalThis): () => void {
  if (typeof host.Image !== 'function') {
    return () => {}
  }

  const images = cardAssetUrls.map((assetUrl) => {
    const image = new host.Image()
    image.decoding = 'async'
    image.src = assetUrl
    return image
  })

  return () => {
    for (const image of images) {
      image.src = ''
    }
  }
}

export function cardAssetUrl(code: string): string | undefined {
  return cardAssetByCode[code.toUpperCase()]
}

export function cardAssetCount(): number {
  return cardAssetUrls.length
}
