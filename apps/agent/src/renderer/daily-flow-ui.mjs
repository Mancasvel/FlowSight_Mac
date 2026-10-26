import { t, getLocale, getLanguage } from './i18n.mjs';
import { dailyFlowView, MILESTONES, WEEKLY_WIN_TARGET } from './daily-flow.mjs';

const PREFERENCE_KEY = 'flowsight_daily_flow_visible_v1';
const CELEBRATION_KEY = 'flowsight_daily_flow_celebrated_v1';
const check = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m5 12 4 4 10-10"/></svg>';

export function mountDailyFlow({ root, preference, statusElement, invoke }) {
  let progress = null, pending = null, renderedKey = '', visible = true, failed = false;
  try { visible = localStorage.getItem(PREFERENCE_KEY) !== 'false'; } catch { /* session preference */ }
  preference.checked = visible;

  function render() {
    root.hidden = !visible;
    if (!visible) return;
    const view = dailyFlowView(progress);
    const dateLabel = `${t('Today')}, ${new Intl.DateTimeFormat(getLocale(), { month: 'long', day: 'numeric' }).format(new Date())}`;
    const streakLabel = progress && !failed ? t(view.streak === 1 ? '{count} day' : '{count} days', { count: view.streak }) : '—';
    const disclosureFocused = document.activeElement === root.querySelector('#dailyFlowToggle');
    const dayLabel = day => {
      const date = new Intl.DateTimeFormat(getLocale(), { weekday: 'long', month: 'short', day: 'numeric' }).format(day.date);
      return progress ? t(day.done ? '{date}: daily win earned' : day.future ? '{date}: upcoming' : day.today ? '{date}: in progress' : '{date}: rest day', { date }) : date;
    };
    const week = `<span class="daily-flow-week" role="list" aria-label="${t('This week')}">${view.week.map(day => `<span role="listitem" class="daily-flow-day${day.done ? ' is-done' : ''}${day.today ? ' is-today' : ''}${day.future ? ' is-future' : ''}" aria-label="${dayLabel(day)}"${day.today ? ' aria-current="date"' : ''}><span>${new Intl.DateTimeFormat(getLocale(), { weekday: 'short' }).format(day.date)}</span><span class="daily-flow-day-mark" aria-hidden="true"></span></span>`).join('')}</span>`;
    const summary = `<summary class="daily-flow-summary" id="dailyFlowToggle"><span class="daily-flow-summary-header"><time id="dailyFlowDate" datetime="${view.today}">${dateLabel}</time><span class="daily-flow-streak"><span>${t('Streak')}</span><strong id="streakText">${streakLabel}</strong></span><svg class="daily-flow-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m8 10 4 4 4-4"/></svg></span>${week}</summary>`;
    if (!progress || failed) {
      const message = failed ? t('Daily Flow is unavailable. Your tracking still works.') : t('Loading your Daily Flow…');
      renderedKey = '';
      root.classList.remove('daily-flow--complete');
      root.innerHTML = `${summary}<div class="daily-flow-detail"><h2>${t('Daily Flow')}</h2><p>${message}</p>${failed ? `<button type="button" class="button button-ghost" data-flow-retry>${t('Retry')}</button>` : ''}</div>`;
      if (disclosureFocused) root.querySelector('#dailyFlowToggle')?.focus({ preventScroll: true });
      root.querySelector('[data-flow-retry]')?.addEventListener('click', refresh);
      return;
    }
    const key = JSON.stringify([view.today, view.percent, view.totalWins, view.streak, view.remainingMinutes, view.weekWins, getLanguage()]);
    if (key === renderedKey) return;
    renderedKey = key;
    const done = view.todayDone;
    root.classList.toggle('daily-flow--complete', done);
    const note = done ? t('Daily win earned. Come back when you are ready.')
      : t('Record 15 minutes today. Pauses and breaks do not count.');
    root.innerHTML = `
      ${summary}<div class="daily-flow-detail">
      <div class="daily-flow-heading"><h2>${t('Daily Flow')}</h2><span class="daily-flow-private">${t('Only on this device')}</span></div>
      <div class="daily-flow-mission"><strong>${done ? t('You showed up today') : t('A small step, every day')}</strong><span class="daily-flow-count">${done ? check : ''}${done ? t('Daily win') : t('{minutes} min to go', { minutes: view.remainingMinutes })}</span></div>
      <p class="daily-flow-note">${note} ${t('Your streak counts consecutive days with saved tracking time.')}</p>
      <div class="daily-flow-track" role="progressbar" aria-label="${t('Daily Flow progress')}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${view.percent}">
        <svg viewBox="0 0 100 4" preserveAspectRatio="none" aria-hidden="true"><rect width="${view.percent}" height="4" rx="2"/></svg>
      </div>
      <div class="daily-flow-week-heading"><strong>${t('This week')}</strong><span>${t('{count} / {target} days', { count: view.weekWins, target: WEEKLY_WIN_TARGET })}${view.weekWins >= WEEKLY_WIN_TARGET ? ` · ${t('Goal reached')}` : ''}</span></div>
      <p class="daily-flow-rest">${t('Aim for 3 days a week. Rest days keep your milestones.')}</p>
      <section class="daily-flow-milestones"><div class="daily-flow-milestones-heading"><h3>${t('Milestones')}</h3><span>${view.milestone ? t('Next: {name} · {count}/{target} wins', { name: t(view.milestone.name), count: view.totalWins, target: view.milestone.days }) : t('All milestones earned')}</span></div>
        <ul>${MILESTONES.map(item => `<li${view.totalWins >= item.days ? ' class="is-earned"' : ''}><span>${t(item.name)}</span><span>${view.totalWins >= item.days ? t('Earned') : t(item.days === 1 ? '{count} daily win' : '{count} daily wins', { count: item.days })}</span></li>`).join('')}</ul>
        <p>${t('Wins come from saved tracking time, not an assessment of work quality. Changing your hours goal does not change Daily Flow.')}</p>
      </section></div>`;
    if (disclosureFocused) root.querySelector('#dailyFlowToggle')?.focus({ preventScroll: true });
  }

  function celebrate(previous, next) {
    const before = previous && dailyFlowView(previous);
    const after = dailyFlowView(next);
    // Initial loads display the earned state quietly; only a live transition celebrates.
    if (!before || before.today !== after.today || before.todayDone || !after.todayDone || !visible) return;
    try {
      if (localStorage.getItem(CELEBRATION_KEY) === after.today) return;
      localStorage.setItem(CELEBRATION_KEY, after.today);
    } catch { /* the in-memory transition still deduplicates */ }
    const milestone = MILESTONES.find(item => item.days === after.totalWins);
    root.classList.remove('daily-flow--celebrate');
    void root.offsetWidth;
    root.classList.add('daily-flow--celebrate');
    statusElement.textContent = t('Daily win earned. Well done showing up.');
    if (milestone) statusElement.textContent += ` ${t('Milestone earned: {name}.', { name: t(milestone.name) })}`;
  }

  function refresh() {
    if (!visible) return Promise.resolve();
    if (pending) return pending;
    pending = invoke('get_daily_flow_progress').then(next => {
      if (!next || !Array.isArray(next.completed_dates)) throw new Error('Daily Flow unavailable');
      const previous = progress;
      progress = next;
      failed = false;
      render();
      celebrate(previous, next);
    }).catch(() => { failed = true; render(); }).finally(() => { pending = null; });
    return pending;
  }

  preference.addEventListener('change', () => {
    visible = preference.checked;
    try { localStorage.setItem(PREFERENCE_KEY, String(visible)); }
    catch { statusElement.textContent = t('This preference applies for this session. Could not save it on this device.'); }
    render();
    if (visible) refresh();
  });
  document.addEventListener('flowsight:languagechange', () => { render(); statusElement.textContent = ''; });
  root.addEventListener('animationend', () => root.classList.remove('daily-flow--celebrate'));
  render();
  return { refresh, render, attach(slot) { if (slot) slot.replaceWith(root); } };
}
