/** Neutral and accent colors read CSS variables, so the theme can change at runtime (see src/theme/theme.ts). */
const STEPS = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];
const scale = (name) =>
  Object.fromEntries(STEPS.map((step) => [step, `rgb(var(--${name}-${step}) / <alpha-value>)`]));

/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'Segoe UI', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Cascadia Code', 'Consolas', 'monospace'],
      },
      colors: {
        zinc: scale('zinc'),
        indigo: scale('indigo'),
        rose: scale('rose'),
        emerald: scale('emerald'),
        amber: scale('amber'),
        sky: scale('sky'),
        ide: {
          bg: 'rgb(var(--ide-bg) / <alpha-value>)',
          panel: 'rgb(var(--ide-panel) / <alpha-value>)',
          raised: 'rgb(var(--ide-raised) / <alpha-value>)',
          border: 'rgb(var(--ide-border) / <alpha-value>)',
          accent: 'rgb(var(--ide-accent) / <alpha-value>)',
        },
      },
    },
  },
  plugins: [],
};
