import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { applyTheme, loadTheme, resolveTheme, saveTheme, type ResolvedTheme, type ThemeSettings } from './theme';

interface ThemeContextValue extends ResolvedTheme {
  settings: ThemeSettings;
  setSettings: (next: ThemeSettings) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [settings, setSettingsState] = useState<ThemeSettings>(loadTheme);

  useEffect(() => {
    applyTheme(settings);
    saveTheme(settings);
  }, [settings]);

  const setSettings = useCallback((next: ThemeSettings) => setSettingsState(next), []);

  const value = useMemo<ThemeContextValue>(
    () => ({ ...resolveTheme(settings), settings, setSettings }),
    [settings, setSettings],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error('useTheme must be used inside ThemeProvider');
  return value;
}
