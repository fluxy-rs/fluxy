/**
 *
 *
 *
 *
 *   import { logger } from './logger';
 *
 *
 *   const logFrame = logger.hot('render-loop', 'debug', { windowMs: 1000, maxPerWindow: 3 });
 *   for (const item of hugeList) {
 *   }
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LoggerOptions {
  level?: LogLevel;
  prefix?: string;
}

export interface HotOptions {
  windowMs?: number;
  maxPerWindow?: number;
}

type HotArg = unknown | (() => unknown);
type HotLogFn = (...args: HotArg[]) => void;
type LogFn = (...args: unknown[]) => void;

const LEVEL_WEIGHT: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

const LEVEL_STYLE: Record<LogLevel, string> = {
  debug: "color:#888",
  info: "color:#2b8a3e",
  warn: "color:#e8590c",
  error: "color:#c92a2a;font-weight:bold",
};

const isDev = import.meta.env.DEV;

const noop: LogFn = () => {};
const noopHot: HotLogFn = () => {};

interface HotState {
  count: number;
  windowStart: number;
  suppressed: number;
}

class Logger {
  readonly debug: LogFn;
  readonly info: LogFn;
  readonly warn: LogFn;
  readonly error: LogFn;

  private level: LogLevel;
  private prefix: string;
  private hotState = new Map<string, HotState>();

  constructor(options: LoggerOptions = {}) {
    this.level = options.level ?? "debug";
    this.prefix = options.prefix ?? "";

    this.debug = this.build("debug", console.debug);
    this.info = this.build("info", console.info);
    this.warn = isDev
      ? this.build("warn", console.warn)
      : (console.warn.bind(console, ...this.tagArgs("warn")) as LogFn);
    this.error = isDev
      ? this.build("error", console.error)
      : (console.error.bind(console, ...this.tagArgs("error")) as LogFn);
  }

  private tagArgs(level: LogLevel): unknown[] {
    const time = new Date().toLocaleTimeString("zh-CN", { hour12: false });
    const tag = this.prefix ? `[${this.prefix}]` : "";
    return [`%c${time} ${tag} ${level.toUpperCase()}`, LEVEL_STYLE[level]];
  }

  private build(level: LogLevel, method: (...args: unknown[]) => void): LogFn {
    if (!isDev) return noop;
    if (LEVEL_WEIGHT[level] < LEVEL_WEIGHT[this.level]) return noop;
    return method.bind(console, ...this.tagArgs(level)) as LogFn;
  }

  hot(tag: string, level: LogLevel = "debug", options: HotOptions = {}): HotLogFn {
    if (!isDev) return noopHot;
    if (LEVEL_WEIGHT[level] < LEVEL_WEIGHT[this.level]) return noopHot;

    const windowMs = options.windowMs ?? 1000;
    const maxPerWindow = options.maxPerWindow ?? 5;
    const fullTag = this.prefix ? `${this.prefix}:${tag}` : tag;
    const consoleMethod = level === "debug" ? console.log : console[level];

    return (...args: HotArg[]) => {
      const now = performance.now();
      let state = this.hotState.get(fullTag);

      if (!state || now - state.windowStart > windowMs) {
        if (state && state.suppressed > 0) {
          console.log(
            `%c[${fullTag}] throttled: ${state.suppressed} entries skipped in the last ${windowMs}ms`,
            "color:#999;font-style:italic",
          );
        }
        state = { count: 0, windowStart: now, suppressed: 0 };
        this.hotState.set(fullTag, state);
      }

      state.count++;
      if (state.count > maxPerWindow) {
        state.suppressed++;
        return;
      }

      const resolved = args.map((a) => (typeof a === "function" ? (a as () => unknown)() : a));
      consoleMethod.call(console, `%c[${fullTag}]`, LEVEL_STYLE[level], ...resolved);
    };
  }

  child(prefix: string): Logger {
    return new Logger({
      level: this.level,
      prefix: this.prefix ? `${this.prefix}:${prefix}` : prefix,
    });
  }
}

export { Logger };
export const logger = new Logger();
