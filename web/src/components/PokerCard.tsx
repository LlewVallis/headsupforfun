import type { HTMLAttributes } from 'react'

const assetModules = import.meta.glob('../assets/cards/*.svg', {
  eager: true,
  import: 'default',
}) as Record<string, string>

const cardAssetByCode = Object.fromEntries(
  Object.entries(assetModules).map(([path, assetUrl]) => {
    const filename = path.split('/').pop()?.replace('.svg', '') ?? path
    return [filename.toUpperCase(), assetUrl]
  }),
)

interface PokerCardProps extends HTMLAttributes<HTMLDivElement> {
  card?: string | null
  hidden?: boolean
  tone?: 'table' | 'hero'
}

export function PokerCard(props: PokerCardProps) {
  const { card = null, hidden = false, tone = 'table', className, ...rest } = props
  const classes = joinClasses(
    'card-shell relative overflow-hidden rounded-[0.5rem] shadow-[0_16px_40px_rgba(0,0,0,0.24)]',
    tone === 'hero' ? 'ring-2 ring-gold-300/45' : 'ring-1 ring-black/12',
    className,
  )

  if (hidden) {
    const src = cardAssetByCode.BACK
    return (
      <div {...rest} className={classes}>
        <img
          src={src}
          alt="Face-down card"
          className="h-full w-full bg-white object-cover"
          draggable={false}
        />
      </div>
    )
  }

  if (!card) {
    return (
      <div
        {...rest}
        aria-hidden="true"
        className={joinClasses(
          classes,
          'grid place-items-center border border-white/8 bg-black/18 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.04)]',
        )}
      >
        <div className="grid h-full w-full place-items-center rounded-[0.4rem] border border-white/6 bg-black/10">
          <span className="block h-8 w-8 rounded-full border border-white/8 bg-white/4" />
        </div>
      </div>
    )
  }

  const code = toAssetCode(card)
  const src = cardAssetByCode[code]
  if (!src) {
    return (
      <div
        {...rest}
        className={joinClasses(
          classes,
          'grid place-items-center border border-white/10 bg-ivory-50 text-sm font-semibold text-slate-900',
        )}
      >
        {hidden ? 'Hidden' : card}
      </div>
    )
  }

  return (
    <div {...rest} className={classes}>
      <img
        src={src}
        alt={hidden ? 'Face-down card' : describeCard(card)}
        className="h-full w-full bg-white object-cover"
        draggable={false}
      />
    </div>
  )
}

function toAssetCode(card: string): string {
  const trimmed = card.trim()
  if (trimmed.length !== 2) {
    return trimmed.toUpperCase()
  }

  const rank = trimmed[0].toUpperCase() === 'T' ? '10' : trimmed[0].toUpperCase()
  const suit = trimmed[1].toUpperCase()
  return `${rank}${suit}`
}

function describeCard(card: string): string {
  const ranks: Record<string, string> = {
    A: 'Ace',
    K: 'King',
    Q: 'Queen',
    J: 'Jack',
    T: 'Ten',
    '9': 'Nine',
    '8': 'Eight',
    '7': 'Seven',
    '6': 'Six',
    '5': 'Five',
    '4': 'Four',
    '3': 'Three',
    '2': 'Two',
  }
  const suits: Record<string, string> = {
    s: 'spades',
    h: 'hearts',
    d: 'diamonds',
    c: 'clubs',
  }
  return `${ranks[card[0].toUpperCase()] ?? card[0]} of ${suits[card[1].toLowerCase()] ?? card[1]}`
}

function joinClasses(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(' ')
}
