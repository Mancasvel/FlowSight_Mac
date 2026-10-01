const KEY = 'flowsight_timer_modes_v1';
const DEFAULTS = { mode: 'normal', work: 25, shortBreak: 5, longBreak: 15 };

export function loadTimerPreferences(storage) {
  try {
    const saved = JSON.parse(storage.getItem(KEY));
    return {
      mode: saved?.mode === 'pomodoro' ? 'pomodoro' : 'normal',
      work: [15, 25, 30, 45, 50, 60].includes(saved?.work) ? saved.work : 25,
      shortBreak: [5, 10, 15].includes(saved?.shortBreak) ? saved.shortBreak : 5,
      longBreak: [15, 20, 30].includes(saved?.longBreak) ? saved.longBreak : 15,
    };
  } catch { return { ...DEFAULTS }; }
}

export function saveTimerPreferences(storage, preferences) {
  try { storage.setItem(KEY, JSON.stringify(preferences)); } catch { /* in-memory still works */ }
}

/** A phase completes once, even if its callback is delayed or a pause races it. */
export class PomodoroTimer {
  constructor(preferences = DEFAULTS) {
    this.configure(preferences);
    this.reset();
  }

  configure(preferences) {
    this.preferences = { ...DEFAULTS, ...preferences };
  }

  reset() {
    this.completed = 0;
    this.phase = 'work';
    this.remainingMs = this.preferences.work * 60000;
    this.anchor = null;
    this.ready = false;
  }

  remaining(tick = performance.now()) {
    return Math.max(0, this.remainingMs - (this.anchor === null ? 0 : Math.max(0, tick - this.anchor)));
  }

  start(tick = performance.now()) {
    if (this.ready) {
      this.phase = 'work';
      this.remainingMs = this.preferences.work * 60000;
      this.ready = false;
    }
    if (this.anchor === null) this.anchor = tick;
  }

  pause(tick = performance.now()) {
    this.remainingMs = this.remaining(tick);
    this.anchor = null;
  }

  finishWork() {
    this.completed++;
    this.phase = this.completed % 4 === 0 ? 'long-break' : 'break';
    this.remainingMs = (this.phase === 'long-break' ? this.preferences.longBreak : this.preferences.shortBreak) * 60000;
    this.anchor = null;
  }

  update(tick = performance.now()) {
    if (this.anchor === null || this.remaining(tick) > 0) return null;
    this.remainingMs = 0;
    this.anchor = null;
    if (this.phase === 'work') return 'work-complete';
    this.ready = true;
    return 'break-complete';
  }
}
