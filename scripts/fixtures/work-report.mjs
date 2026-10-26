// Fictional work data for production-renderer E2E. No account, personal DB or network.
export const syntheticWorkReport = {
  ai_powered: false,
  generated_at: '2026-10-19T10:00:00Z',
  local_data: {
    period_start: '2026-10-12', period_end: '2026-10-18', period_days: 7,
    total_seconds: 36000, focus_eligible_seconds: 18000, deep_focus_seconds: 10800,
    distraction_seconds: 1800, deep_focus_sessions: 4, active_days: 5,
    focus_semantics: { longest_focus_seconds: 2700 },
    prior_period: { period_start: '2026-10-05', period_end: '2026-10-11', total_seconds: 36000,
      focus_eligible_seconds: 18000, deep_focus_seconds: 7200, distraction_seconds: 3600, longest_focus_seconds: 1800 },
    daily_totals: [3600, 5400, 7200, 9000, 10800].map((total_seconds, index) => ({ date: `2026-10-${index + 12}`, total_seconds })),
    category_breakdown: [
      ['Coding', 12600], ['Research', 8400], ['Analysis', 5400], ['Communication', 3600],
      ['Admin', 2400], ['Documentation', 1800], ['General', 1200], ['Browsing', 600],
    ].map(([category, total_seconds]) => ({ category, total_seconds })),
    distraction_app_analysis: { detours: [
      { label: 'Synthetic work tool', kind: 'other', seconds: 4000, visits: 20, days: 4 },
      { label: 'Synthetic team chat', kind: 'communication', seconds: 1200, visits: 10, days: 3 },
      { label: 'Synthetic video', kind: 'video', seconds: 600, visits: 5, days: 2 },
    ] },
  },
  report: {
    executive_overview: 'Synthetic recorded activity. RAW_SYNTHETIC_SCREEN_CONTEXT '.repeat(12),
    overall_health: 'Sustained blocks observed',
    recommendations: ['RAW_SYNTHETIC_RECOMMENDATION: inspect technical details. '.repeat(20)],
    health_notes: 'RAW_SYNTHETIC_SCREEN_CONTEXT: these details belong in the disclosure.',
    observed_work: ['RAW_SYNTHETIC_SCREEN_CONTEXT: illustrative foreground observation.'],
    lessons_learned: [{ title: 'Synthetic full-analysis lesson', body: 'RAW_SYNTHETIC_SCREEN_CONTEXT' }],
  },
  decisions: [{ text: 'SYNTHETIC_PRIVATE_PAYLOAD_DECISION' }],
};

// Passed directly to addInitScript; keep its browser state self-contained.
export function installSyntheticWorkReportNative({ language, report }) {
  localStorage.setItem('flowsight_language_preference', language);
  const callbacks = new Map(); let callbackId = 1;
  window.reportFixture = structuredClone(report);
  window.reportCalls = []; window.reportExports = []; window.reportFailExport = false;
  window.reportHoldExport = false;
  const prefs = { onboardingCompleted: true, displayName: 'Synthetic report reader', workRoles: [], workActivities: [], improvementGoals: [], dailyGoalHours: 0 };
  const privacy = { monitoringNoticeAcknowledged: true, noticeVersion: '2026-09-28', cloudSyncEnabled: false,
    cloudAiEnabled: false, storeWindowTitles: false, excludedApplications: [], retentionDays: 30 };
  const data = { date: '2026-10-18', total_seconds: 1800, entries: [{ time: '2026-10-18T09:00:00',
    duration_seconds: 1800, category: 'Coding', description: 'Synthetic work only' }], category_breakdown: [{ category: 'Coding', total_seconds: 1800 }], ticket_breakdown: [], focus: {} };
  const decisions = () => JSON.parse(localStorage.getItem('synthetic-report-decisions') || '[]');
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(fn) { const id = callbackId++; callbacks.set(id, fn); return id; },
    unregisterCallback(id) { callbacks.delete(id); }, convertFileSrc(path) { return path; },
    async invoke(command, args = {}) {
      window.reportCalls.push({ command, args });
      if (command === 'plugin:event|listen') return args.handler;
      if (command.startsWith('plugin:window|')) return command.endsWith('is_maximized') ? false : null;
      if (command.startsWith('plugin:event|') || command.startsWith('plugin:updater|')) return null;
      if (command === 'plugin:app|version') return '6.5.4';
      if (command === 'get_work_review_decisions') return decisions();
      if (command === 'save_work_review_decision') {
        const saved = { ...args.input, id: args.input.id || 'synthetic-private-choice', createdAt: '2026-10-19', updatedAt: '2026-10-19' };
        localStorage.setItem('synthetic-report-decisions', JSON.stringify([saved, ...decisions().filter(item => item.id !== saved.id)]));
        return saved;
      }
      if (command === 'delete_work_review_decision') {
        localStorage.setItem('synthetic-report-decisions', JSON.stringify(decisions().filter(item => item.id !== args.id))); return null;
      }
      if (command === 'get_user_preferences' || command === 'save_user_preferences_command') return command === 'get_user_preferences' ? prefs : args.prefs;
      if (command === 'get_privacy_settings') return privacy;
      if (command === 'get_today_history') return data;
      if (command === 'generate_local_status_report') return structuredClone(window.reportFixture);
      if (command === 'save_pdf_to_downloads') {
        const attempt = { ...args, synthetic: true, status: 'pending' };
        window.reportExports.push(attempt);
        if (window.reportFailExport) { attempt.status = 'failed'; throw Error('Synthetic Downloads folder failure'); }
        if (window.reportHoldExport) await new Promise(resolve => { window.releaseReportExport = resolve; });
        attempt.status = 'saved'; return `/synthetic/downloads/${args.filename}`;
      }
      if (command === 'open_path_in_file_manager') return null;
      if (command === 'get_local_agent_data') return { events: [], preferences: {}, tasks: [] };
      if (command === 'get_total_focus') return { preferences: { patterns: [], exceptions: [], durationMinutes: 50, quietNotifications: false }, session: null, browser: {}, digest: [] };
      const values = {
        initialize_agent: null, get_config: { dailyGoalHours: 0, captureInterval: 60000 }, get_auth_session: null, get_current_user: null,
        get_entitlements: { plan: 'free', status: 'active', can_sync: false, can_cloud_ai: false, can_integrations: false, team_ids: [] },
        get_analytics_consent: { decided: true, consented: false }, get_status: { isRunning: false },
        get_tracking_clock: { date: '2026-10-18', total_seconds: 1800, total_milliseconds: 1800000, is_running: false },
        check_installation_health: { healthy: true }, check_local_server: { online: false }, get_week_summary: { days: [] },
        get_desktop_preferences: { promptDecided: true, focusAlertsEnabled: false, contextualFocusAlertsEnabled: false },
        get_weekly_report_schedule: { enabled: false, weekday: 5, time: '17:00', folder: '', revision: 0 },
        get_calendar_companion_status: {}, get_browser_pairing: {}, get_notion_status: { connected: false }, get_coach_chat_messages: [],
      };
      return values[command] ?? null;
    },
  };
}
