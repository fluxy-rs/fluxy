export interface CreateLongPressOptions {
  /** 按住多久算长按（毫秒） */
  delayMs: number;
  /** 允许的手指抖动距离（像素） */
  tolerancePx: number;
  /** 计时到点且未抖出容差时触发 */
  onTrigger: () => void;
}

export interface LongPressGesture {
  /** 手指按下：记录起点并开始计时（重复调用会重置计时） */
  start: (x: number, y: number) => void;
  /** 手指移动：抖出容差即取消 */
  move: (x: number, y: number) => void;
  /** 抬起 / 取消 / 卸载：停止计时 */
  cancel: () => void;
}

/**
 * 长按手势。
 *
 * 单一职责：一个"按住 delayMs 且抖动不超过 tolerancePx 才触发"的计时器状态机。
 * 不碰 DOM、不读事件——起点坐标由调用方传入，因此可以用假定时器精确断言
 * 计时、抖动取消与重复起点接管。
 */
export function createLongPress(options: CreateLongPressOptions): LongPressGesture {
  let timer: number | undefined;
  let origin: { x: number; y: number } | undefined;

  const clear = () => {
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timer = undefined;
    }
  };

  return {
    start(x, y) {
      origin = { x, y };
      clear();
      timer = window.setTimeout(() => {
        timer = undefined;
        options.onTrigger();
      }, options.delayMs);
    },
    move(x, y) {
      if (!origin || timer === undefined) return;
      if (Math.hypot(x - origin.x, y - origin.y) > options.tolerancePx) clear();
    },
    cancel() {
      origin = undefined;
      clear();
    },
  };
}
