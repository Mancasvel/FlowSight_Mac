"""Actual production renderer, with explicitly synthetic native IPC and work data."""
import functools
import http.server
import json
import os
from pathlib import Path
import threading
from urllib.parse import urlsplit
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('FLOWSIGHT_REPORT_EVIDENCE', ROOT / '.impeccable/review/report-progress'))
OUT.mkdir(parents=True, exist_ok=True)
metrics = dict(total_seconds=36000, focus_eligible_seconds=18000, deep_focus_seconds=10800,
               distraction_seconds=1800, active_days=5, deep_focus_sessions=4,
               focus_semantics=dict(longest_focus_seconds=2700))
report = dict(ai_powered=False, generated_at='2026-10-19 10:00', report=dict(
    executive_overview='Recorded work across five days.', recommendations=['Protect a 30-minute writing block.'],
    overall_health='Sustained blocks observed'), local_data=dict(**metrics, period_start='2026-10-12',
    period_end='2026-10-18', period_days=7, daily_totals=[dict(date=f'2026-10-{day}', total_seconds=7200) for day in range(12,17)],
    category_breakdown=[dict(category='Coding', total_seconds=18000), dict(category='Writing', total_seconds=18000)],
    prior_period=dict(total_seconds=36000, focus_eligible_seconds=18000, deep_focus_seconds=7200,
        distraction_seconds=3600, longest_focus_seconds=1800, period_start='2026-10-05', period_end='2026-10-11')))

class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args): pass

server = None
renderer_url = os.environ.get('FLOWSIGHT_RENDERER_URL')
if not renderer_url:
    renderer_dist = Path(os.environ.get('FLOWSIGHT_RENDERER_DIST', ROOT/'apps/agent/dist'))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=renderer_dist))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    renderer_url = f'http://127.0.0.1:{server.server_port}'
renderer_origin = '{0.scheme}://{0.netloc}'.format(urlsplit(renderer_url))
source = (ROOT/'scripts/verify-work-review.mjs').read_text(encoding='utf-8')
begin = source.index('await context.addInitScript(') + len('await context.addInitScript(')
end = source.index('},{language:view.language});', begin) + 1
stub = source[begin:end]
checks = []
try:
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        for name, width, height, language, theme in [('wide-es',900,1000,'es','light'), ('compact-es',370,900,'es','light'), ('minimum-es',340,400,'es','light'), ('wide-dark',900,1000,'en','dark')]:
            context = browser.new_context(viewport=dict(width=width,height=height), locale='es-ES' if language=='es' else 'en-GB', color_scheme=theme, reduced_motion='reduce')
            context.route('**/*', lambda route: route.continue_() if route.request.url.startswith(renderer_origin + '/') else route.abort())
            context.add_init_script(f'({stub})({json.dumps(dict(language=language))});')
            context.add_init_script('''const originalInvoke=window.__TAURI_INTERNALS__.invoke;
                window.reportFixture='''+json.dumps(report)+''';
                window.__TAURI_INTERNALS__.invoke=async(command,args)=>command==='generate_local_status_report'?window.reportFixture:originalInvoke(command,args);''')
            page = context.new_page()
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.goto(renderer_url, wait_until='networkidle')
            page.locator('.nav-item[data-tab="tabSummary"]').click()
            assert page.locator('#workReportPeriod').input_value() == '7'
            page.locator('#generateReportBtn').click()
            page.locator('.sr-grade').wait_for()
            page.evaluate('document.fonts.ready')
            assert page.locator('.sr-grade-number strong').inner_text() == ('6,9' if language=='es' else '6.9')
            assert page.locator('.sr-progress-improved li').count() == 2
            assert page.locator('.sr-comparison-previous').count() == 3
            assert page.locator('.sr-comparison-current').count() == 3
            assert page.locator('.sr-day').count() == 7
            assert page.locator('.sr-category-row').count() == 2
            assert page.locator('.sr-report-details').get_attribute('open') is None
            assert page.locator('#srEvidenceTitle').evaluate("node => !!(node.compareDocumentPosition(document.querySelector('#srNextTitle')) & Node.DOCUMENT_POSITION_FOLLOWING)")
            assert page.locator('#statusReportModalBody').evaluate('(node)=>node.scrollWidth<=node.clientWidth+1')
            page.locator('#statusReportModalBody').evaluate('(node)=>node.scrollTop=0')
            page.screenshot(path=str(OUT/f'{name}.png'))
            # The main capture proves the minimum window. An isolated component
            # capture uses the same layout width with enough height to show the
            # entire section without the modal's sticky header obscuring it.
            if height < 900:
                page.set_viewport_size(dict(width=width, height=900))
            page.locator('.sr-progress-comparison').screenshot(path=str(OUT/f'{name}-comparison.png'))
            page.locator('.sr-grade-method summary').click()
            assert page.locator('.sr-grade-method').get_attribute('open') is not None
            if name == 'wide-es':
                assert 'En qué has mejorado' in page.locator('.sr-progress').inner_text()
                assert 'En qué puedes mejorar' in page.locator('.sr-progress').inner_text()
                assert 'Previous' not in page.locator('.sr-progress').inner_text()
                page.locator('#closeStatusReportModal').click()
                page.evaluate('window.reportFixture.local_data.prior_period.total_seconds=0')
                page.locator('#generateReportBtn').click()
                page.locator('.sr-grade').wait_for()
                assert page.locator('.sr-grade-delta').count() == 0
                assert page.locator('.sr-comparison-previous').count() == 0
                assert page.locator('.sr-grade-number strong').inner_text() == '6,9'
                page.locator('#closeStatusReportModal').click()
                page.evaluate('''Object.assign(window.reportFixture.local_data, {total_seconds:0, deep_focus_seconds:0,
                    focus_eligible_seconds:0, distraction_seconds:0, active_days:0, daily_totals:[], category_breakdown:[]})''')
                page.locator('#generateReportBtn').click()
                page.locator('.sr-grade').wait_for()
                assert page.locator('.sr-grade-number strong').inner_text() == '—'
                assert page.locator('.sr-progress-insights li').count() == 0
                assert page.locator('[data-review-suggestion]').count() == 0
                checks.extend(['Missing baseline has no invented delta or comparison bars', 'Empty period has no grade, improvement or unsupported action'])
            assert not errors, errors
            checks.append(f'{name}: grade, comparison, leading day/category charts, closed analysis, localization, disclosure, no horizontal overflow, no renderer errors')
            context.close()
        browser.close()
    (OUT/'verification.json').write_text(json.dumps(dict(passed=True, synthetic=True, checks=checks),indent=2),encoding='utf-8')
    print(json.dumps(dict(passed=True, checks=checks)))
finally:
    if server:
        server.shutdown()
        server.server_close()
