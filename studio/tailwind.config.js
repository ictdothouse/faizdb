/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ['class'],
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        background: '#09090b', // Zinc 950 obsidian
        foreground: '#fafafa', // Zinc 50
        card: {
          DEFAULT: '#121215',
          foreground: '#fafafa',
        },
        border: '#27272a', // Zinc 800
        input: '#18181b', // Zinc 900
        brand: {
          DEFAULT: '#10b981', // Emerald 500
          hover: '#059669',
          flame: '#f59e0b', // Amber 500
          dark: '#064e3b',
        },
        sidebar: {
          DEFAULT: '#0c0c0e',
          border: '#1f1f23',
        },
      },
      fontFamily: {
        sans: ['Geist', 'Inter', 'system-ui', 'sans-serif'],
        mono: ['Geist Mono', 'JetBrains Mono', 'Fira Code', 'monospace'],
      },
      boxShadow: {
        glow: '0 0 25px -5px rgba(16, 185, 129, 0.15)',
        'glow-amber': '0 0 25px -5px rgba(245, 158, 11, 0.15)',
      },
    },
  },
  plugins: [],
};
