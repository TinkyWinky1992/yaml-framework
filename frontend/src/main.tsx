import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './monaco';
import './index.css';
import { setApiBase } from './api';
import { resolveApiBase } from './desktop';
import { ThemeProvider } from './theme/ThemeContext';
import { applyTheme, loadTheme } from './theme/theme';

// Apply the saved theme before the first render, so the first frame already has the right colors.
applyTheme(loadTheme());
import App from './App';

// The API address is known before the first request, so resolve it before rendering.
resolveApiBase()
  .then(setApiBase)
  .catch(() => undefined)
  .finally(() => {
    createRoot(document.getElementById('root')!).render(
      <StrictMode>
        <ThemeProvider>
          <App />
        </ThemeProvider>
      </StrictMode>,
    );
  });
