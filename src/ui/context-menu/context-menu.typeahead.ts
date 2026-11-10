/** 键盘字符导航的缓冲窗口（毫秒） */
export const TYPEAHEAD_TIMEOUT = 500;

export interface TypeaheadCandidate {
  id: string;
  label: string;
}

export interface CreateTypeaheadOptions {
  /** 参与匹配的候选：按 DOM 顺序、已过滤 disabled */
  candidates: () => TypeaheadCandidate[];
  /** 命中后的高亮回调 */
  onMatch: (id: string) => void;
  /** 缓冲窗口，默认 500ms */
  timeoutMs?: number;
}

export interface Typeahead {
  /** 处理一个可打印字符 */
  handle: (char: string) => void;
  /** 清理未到期的定时器（组件卸载时调用） */
  dispose: () => void;
}

/**
 * 菜单的"输入即跳转"：把连续输入攒成前缀，在窗口内匹配第一个以该前缀开头的候选。
 *
 * 行为对齐 Base UI：大小写不敏感；窗口到期后缓冲清空、重新开始；
 * 命中后缓冲**不**清空（继续输入会匹配更长的前缀）。
 */
export function createTypeahead(options: CreateTypeaheadOptions): Typeahead {
  const timeoutMs = options.timeoutMs ?? TYPEAHEAD_TIMEOUT;
  let buffer = "";
  let timer: number | undefined;

  const clearTimer = () => {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timer = undefined;
    }
  };

  return {
    handle(char) {
      buffer += char.toLowerCase();
      clearTimer();
      timer = window.setTimeout(() => {
        buffer = "";
        timer = undefined;
      }, timeoutMs);

      const match = options
        .candidates()
        .find((candidate) => candidate.label.toLowerCase().startsWith(buffer));
      if (match) options.onMatch(match.id);
    },
    dispose: clearTimer,
  };
}
