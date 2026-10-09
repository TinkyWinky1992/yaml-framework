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
        ide: {
          bg: '#09090b',
          panel: '#18181b',
          raised: '#27272a',
          border: '#3f3f46',
          accent: '#6366f1',
        },
      },
    },
  },
  plugins: [],
};
