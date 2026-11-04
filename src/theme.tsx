import {
  createContext,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  useContext,
  type Accessor,
  type ParentProps,
} from "solid-js";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

type ThemeContextValue = {
  preference: Accessor<ThemePreference>;
  resolved: Accessor<ResolvedTheme>;
  setPreference: (next: ThemePreference) => void;
  cycle: () => void;
};

const STORAGE_KEY = "fluxy.theme";
const ThemeContext = createContext<ThemeContextValue>();

function readPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") {
      return stored;
    }
  } catch {
    // Storage can be unavailable in hardened WebViews; system is a safe default.
  }
  return "system";
}

function prefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ThemeProvider(props: ParentProps) {
  const [preference, setPreferenceSignal] = createSignal<ThemePreference>(readPreference());
  const [systemDark, setSystemDark] = createSignal(prefersDark());

  const resolved = (): ResolvedTheme => {
    const current = preference();
    if (current === "system") {
      return systemDark() ? "dark" : "light";
    }
    return current;
  };

  const setPreference = (next: ThemePreference) => {
    setPreferenceSignal(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // The current session still applies the choice.
    }
  };

  const cycle = () =>
    setPreference(
      preference() === "system" ? "light" : preference() === "light" ? "dark" : "system",
    );

  createEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolved();
    root.classList.toggle("dark", resolved() === "dark");
    root.style.colorScheme = resolved();
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", resolved() === "dark" ? "#0b1418" : "#edf2f3");
  });

  onMount(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", onChange);
    onCleanup(() => media.removeEventListener("change", onChange));
  });

  return (
    <ThemeContext.Provider value={{ preference, resolved, setPreference, cycle }}>
      {props.children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) {
    throw new Error("useTheme must be used inside ThemeProvider");
  }
  return value;
}
