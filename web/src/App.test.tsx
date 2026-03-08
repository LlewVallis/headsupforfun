import { render, screen } from '@testing-library/react'

import App from './App'

describe('App', () => {
  it('renders the poker web bootstrap shell', () => {
    render(<App />)

    expect(
      screen.getByRole('heading', { name: 'GTO Poker' }),
    ).toBeInTheDocument()
    expect(screen.getByLabelText('Poker table preview')).toBeInTheDocument()
    expect(screen.getByLabelText('Frontend status')).toBeInTheDocument()
    expect(screen.getByText(/Vitest and Playwright/i)).toBeInTheDocument()
  })
})
