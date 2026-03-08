export function PokerChipMark(props: { className?: string }) {
  return (
    <svg
      viewBox="0 0 96 96"
      aria-hidden="true"
      className={props.className ?? ''}
      fill="none"
    >
      <circle cx="48" cy="48" r="44" fill="#0F3326" stroke="#E7D39C" strokeWidth="6" />
      <circle cx="48" cy="48" r="30" fill="#174E39" stroke="#E7D39C" strokeWidth="4" strokeDasharray="10 8" />
      <path
        d="M48 24C40.6 33.6 30.4 42.1 30.4 52.8C30.4 62.7 38 70.5 48 70.5C58 70.5 65.6 62.7 65.6 52.8C65.6 42.1 55.4 33.6 48 24Z"
        fill="#E7D39C"
      />
      <path d="M48 72L36.4 88H59.6L48 72Z" fill="#E7D39C" />
    </svg>
  )
}
