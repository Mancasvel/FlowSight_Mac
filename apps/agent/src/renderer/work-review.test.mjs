import test from 'node:test';
import assert from 'node:assert/strict';
import { localDate,nextReviewDate,decisionDue,reviewMarkup } from './work-review.mjs';
import { setLanguagePreference } from './i18n.mjs';
import { createStatusReportViewModel,renderStatusReportHtml } from './status-report.mjs';
import { renderStatusReportPdf } from './status-report-pdf.mjs';
import { jsPDF } from 'jspdf';

test('review dates use calendar days, including month and year boundaries',()=>{
  assert.equal(localDate(new Date(2026,11,31,23,55)),'2026-12-31');
  assert.equal(nextReviewDate(new Date(2026,11,31,23,55)),'2027-01-07');
  assert.equal(decisionDue({status:'planned',reviewDate:'2026-10-10'},'2026-10-10'),true);
  assert.equal(decisionDue({status:'tried',reviewDate:'2026-10-09'},'2026-10-10'),false);
});
test('saved user content remains escaped and is never translated as interface copy',()=>{
  setLanguagePreference('es',{persist:false});
  try {
    const out=reviewMarkup([{id:'abc',text:'Keep my current plan <script>x</script>',reviewDate:'2026-10-10',status:'planned',note:'Review · 東京'}],{});
    assert.match(out,/Elige un cambio/);assert.match(out,/Keep my current plan &lt;script&gt;/);assert.match(out,/Review · 東京/);assert.doesNotMatch(out,/<script>/);
  }finally{setLanguagePreference('en',{persist:false});}
});
test('origin and coverage agree across screen and PDF and private choices stay out of export',()=>{
  const payload={ai_powered:false,local_data:{period_start:'2026-10-10',period_end:'2026-10-10',period_days:1,total_seconds:600,active_days:1},report:{recommendations:['Review one block']},decisions:[{text:'PRIVATE DECISION'}]};
  const model=createStatusReportViewModel(payload);
  assert.equal(model.periodStart,'2026-10-10');assert.equal(model.periodDays,1);
  const output=renderStatusReportHtml(model);assert.match(output,/Rule-based report/);assert.match(output,/1 of 1 days with activity/);assert.doesNotMatch(output,/PRIVATE DECISION/);
  const doc=new jsPDF(),drawn=[];const original=doc.text.bind(doc);doc.text=(value,...args)=>{drawn.push(String(value));return original(value,...args);};renderStatusReportPdf(doc,model);
  assert.ok(drawn.some(line=>line.includes('Rule-based report')));assert.ok(drawn.some(line=>line.includes('1 of 1 days with activity')));assert.ok(!drawn.some(line=>line.includes('PRIVATE DECISION')));
});
test('thirty-day reports use a period title and empty reports suppress unsupported actions',()=>{
  const payload={local_data:{period_start:'2026-09-11',period_end:'2026-10-10',period_days:30,total_seconds:0},report:{recommendations:['Unsupported suggestion']}};
  const model=createStatusReportViewModel(payload);
  assert.equal(model.periodDays,30);assert.equal(model.title,'Work review');assert.deepEqual(model.actions,[]);
});
