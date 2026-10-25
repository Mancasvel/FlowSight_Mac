import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./music.js', import.meta.url), 'utf8');
function harness() {
  let listener, mutation;
  const media = [];
  runInNewContext(source, {
    document: { querySelectorAll: () => media },
    MutationObserver: class {
      constructor(callback) { mutation = callback; }
      observe() {}
      disconnect() { mutation = null; }
    },
    chrome: { runtime: { id: 'test', onMessage: { addListener(callback) { listener = callback; } },
      sendMessage: async () => ({}) } },
    WeakSet, WeakMap, Number, Array,
  });
  const command = (playback, revision, sender = 'test') => new Promise(resolve => {
    if (!listener({ type: 'focus-music-playback', playback, revision }, { id: sender }, resolve)) resolve({});
  });
  return { media, command, mutation: () => mutation?.() };
}
function player() {
  const events = {};
  return { paused: true, currentTime: 42, plays: 0, pauses: 0,
    addEventListener(name, callback) { events[name] = callback; },
    async play() { this.plays++; this.paused = false; events.play?.(); },
    pause() { this.pauses++; this.paused = true; },
    events,
  };
}
test('music pauses and resumes at the same position, preserving manual controls while active', async () => {
  const h = harness(), video = player(); h.media.push(video);
  await h.command('play', 1); assert.equal(video.paused, false);
  await h.command('pause', 2); assert.equal(video.paused, true);
  await h.command('play', 3); assert.equal(video.paused, false);
  assert.equal(video.currentTime, 42);
  video.pause(); h.mutation(); await Promise.resolve();
  assert.equal(video.paused, true, 'DOM mutations do not override manual pause');
  await h.command('pause', 2); assert.equal(video.plays, 2, 'Old commands are ignored');
  await h.command('pause', 4);
  await h.command('release', 5);
  await video.play(); assert.equal(video.paused, false, 'Ending returns control to the user');
});
test('pause received before loading stops late autoplay and replacement players', async () => {
  const h = harness();
  assert.equal((await h.command('pause', 2)).pending, true);
  const video = player(); h.media.push(video); h.mutation();
  await video.play(); assert.equal(video.paused, true);
  const replacement = player(); h.media.splice(0, 1, replacement); h.mutation();
  await replacement.play(); assert.equal(replacement.paused, true);
  await h.command('play', 3); assert.equal(replacement.paused, false);
});
test('autoplay rejection is reported and a later pause wins over an unfinished play', async () => {
  const h = harness(), video = player(); h.media.push(video);
  video.play = async () => { throw new Error('NotAllowedError'); };
  assert.match((await h.command('play', 1)).error, /Allow autoplay/);
  let finish;
  video.play = () => new Promise(resolve => { finish = () => { video.paused = false; resolve(); }; });
  const playing = h.command('play', 2);
  await h.command('pause', 3); finish(); await playing;
  assert.equal(video.paused, true);
  await h.command('play', 4, 'other-extension'); assert.equal(video.paused, true);
});
