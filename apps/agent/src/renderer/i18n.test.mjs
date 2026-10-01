import assert from 'node:assert/strict';
import test from 'node:test';
import { spanish } from './locales/es.mjs';
import { categoryLabel, html, message, normalizeLanguage, resolveLanguage, setLanguagePreference, t } from './i18n.mjs';

test('language selection falls back to English and explicit choices take precedence', () => {
  assert.equal(normalizeLanguage('es-MX'),'es');
  assert.equal(resolveLanguage('system',['es-ES']),'es');
  assert.equal(resolveLanguage('system',['fr-FR']),'en');
  assert.equal(resolveLanguage('en',['es-ES']),'en');
  assert.equal(resolveLanguage('es',['en-GB']),'es');
});

test('every Spanish template retains all substitution parameters', () => {
  const parameters=value=>[...value.matchAll(/\{(\w+)\}/g)].map(match=>match[1]).sort();
  for(const [source,translation]of Object.entries(spanish)) {
    assert.ok(translation.length,source);
    assert.deepEqual(parameters(translation),parameters(source),source);
    assert.doesNotMatch(translation,/<script|<!--|data-i18n/i,source);
  }
});

test('literal markup is translated before interpolation and user content remains exact', () => {
  setLanguagePreference('es',{persist:false});
  const userTitle='Review',description='Settings · 学習';
  const output=html`<p>Settings</p><strong>${userTitle}</strong><span>${description}</span>`;
  assert.match(output,/Ajustes/);assert.match(output,/<strong>Review<\/strong>/);
  assert.match(output,/<span>Settings · 学習<\/span>/);
  assert.equal(message`Owner: ${userTitle}`,'Responsable: Review');
  assert.equal(categoryLabel('Coding'),'Programación');
  assert.equal(categoryLabel('Review'),'Review');
  setLanguagePreference('en',{persist:false});
  assert.equal(t('Settings'),'Settings');
});
