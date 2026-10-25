import test from 'node:test';
import assert from 'node:assert/strict';
import {contentLength,drawContentImage} from './build-in-public.mjs';

test('X counts shortened URLs and weighted non-Latin characters',()=>{
  assert.equal(contentLength('Visit https://example.com/a/long/link','x'),29);
  assert.equal(contentLength('你好','x'),4);
  assert.equal(contentLength('A short post','linkedin'),12);
});
test('Instagram export uses portrait dimensions and renders the actual user copy',()=>{
  const text=[];
  const ctx={fillRect(){},measureText(value){return {width:value.length*22};},fillText(value,x,y){text.push({value,x,y});}};
  const canvas={getContext(){return ctx;},toDataURL(){return 'data:image/png;base64,test';}};
  drawContentImage({title:'A smaller local model',detail:'The model runs on my device.',footer:'FlowSight'},canvas);
  assert.equal(canvas.width,1080);assert.equal(canvas.height,1350);
  assert.ok(text.some(item=>item.value==='A smaller local model'));
  assert.ok(text.some(item=>item.value==='The model runs on my device.'));
  assert.ok(text.every(item=>item.y<1350));
});
