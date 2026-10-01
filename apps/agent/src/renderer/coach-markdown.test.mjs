import test from 'node:test';
import assert from 'node:assert/strict';
import { renderCoachMarkdown } from './coach-markdown.mjs';

test('saved plain Coach replies render without calling the HTML text as a function', () => {
  assert.equal(renderCoachMarkdown('A saved Coach reply.'), '<p>A saved Coach reply.</p>');
  assert.equal(renderCoachMarkdown('Revisa PLE y descansa 15 minutos.'), '<p>Revisa PLE y descansa 15 minutos.</p>');
  assert.equal(renderCoachMarkdown(null), '');
});

test('Coach headings, lists and emphasis survive loading and rendering', () => {
  const rendered = renderCoachMarkdown('# Plan\n\n**PLE first**\n\n## Next\n\n1. Work on PLE\n2. Take a break\n\n### Review\n\n- Keep all four tasks\n- Allow 15 minutes');
  assert.match(rendered, /<h1>Plan<\/h1>/);
  assert.match(rendered, /<h2>Next<\/h2>/);
  assert.match(rendered, /<h3>Review<\/h3>/);
  assert.match(rendered, /<strong>PLE first<\/strong>/);
  assert.match(rendered, /<ul><li>Work on PLE<\/li>\n<li>Take a break<\/li>/);
  assert.match(rendered, /<ul><li>Keep all four tasks<\/li>\n<li>Allow 15 minutes<\/li>/);
});

test('Coach content cannot introduce executable markup and retains original names', () => {
  const rendered = renderCoachMarkdown('Settings · Review · 学習\n\n<script>alert("bad")</script> <img src=x onerror="bad"> & **PLE**');
  assert.match(rendered, /Settings · Review · 学習/);
  assert.doesNotMatch(rendered, /<script|<img/);
  assert.match(rendered, /&lt;script&gt;/);
  assert.match(rendered, /&amp; <strong>PLE<\/strong>/);
});
