// Node 22+ 可直接运行：node --experimental-strip-types scripts/check-touch.mjs
import assert from 'node:assert/strict';
import { screenPoint } from '../web/src/lib/touch.ts';
import { ScrcpyOptions3_1, ScrcpyControlMessageSerializer } from '../web/node_modules/@yume-chan/scrcpy/esm/index.js';

const box = { left: 100, top: 50, width: 400, height: 400 };
assert.equal(screenPoint(box, 400, 800, 150, 250), undefined);
assert.deepEqual(screenPoint(box, 400, 800, 300, 250), { x: 200, y: 400, width: 400, height: 800 });
assert.deepEqual(screenPoint(box, 400, 800, 500, 500, true), { x: 399, y: 799, width: 400, height: 800 });
assert.equal(screenPoint(box, 800, 400, 300, 60), undefined);
assert.deepEqual(screenPoint(box, 800, 400, 300, 250), { x: 400, y: 200, width: 800, height: 400 });
assert.equal(screenPoint({ left: 0, top: 0, width: 0, height: 0 }, 400, 800, 0, 0), undefined);

const serializer = new ScrcpyControlMessageSerializer(new ScrcpyOptions3_1({ control: true }));
const down = serializer.injectTouch({ action: 0, pointerId: 0n, pointerX: 100, pointerY: 200,
  screenWidth: 400, screenHeight: 800, pressure: 1, actionButton: 1, buttons: 1 });
assert.equal(Buffer.from(down).toString('hex'), '0200000000000000000000000064000000c801900320ffff0000000100000001');
for (const keyCode of [3, 4, 187]) {
  for (const action of [0, 1]) {
    const key = serializer.injectKeyCode({ action, keyCode, repeat: 0, metaState: 0 });
    assert.equal(key.length, 14);
    assert.equal(key[0], 0);
    assert.equal(key[1], action);
    assert.equal(new DataView(key.buffer, key.byteOffset).getUint32(2), keyCode);
  }
}
assert.deepEqual([...serializer.setDisplayPower(0)], [10, 0]);
assert.deepEqual([...serializer.setDisplayPower(2)], [10, 2]);
console.log('通过：横竖画面映射、留白拒绝、越界收敛及底层库触控／三键协议');
