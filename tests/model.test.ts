import { test } from 'node:test';
import assert from 'node:assert/strict';
import { periodLabel, percentText, resetLabel } from '../src/model.ts';
test('quota periods follow the actual service window', () => {
  assert.equal(periodLabel(300, ''), '5 小时额度');
  assert.equal(periodLabel(10080, ''), '每周额度');
  assert.equal(periodLabel(15, ''), '15 分钟额度');
  assert.equal(periodLabel(null, '短周期额度'), '短周期额度');
});
test('unknown percentage is distinct from exhausted quota', () => {
  assert.equal(percentText(null), '—'); assert.equal(percentText(0), '0');
  assert.equal(percentText(120), '100'); assert.equal(percentText(-20), '0');
});
test('reset timestamp is Unix seconds and never guesses a quota reset', () => {
  assert.equal(resetLabel(null, 0), '重置时间未知');
  assert.equal(resetLabel(0, 0), '等待额度更新');
  assert.equal(resetLabel(3600, 0), '1 小时后重置');
  assert.equal(resetLabel(90000, 0), '1 天 1 小时后重置');
});
