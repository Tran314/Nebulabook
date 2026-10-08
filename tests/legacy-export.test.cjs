// Tests the one-time browser migration helper; Node is not an app dependency.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../scripts/export-legacy-data.js'), 'utf8');

async function runExport({ exists = true, stores = { notes: [] }, failTransaction = false, creationRace = false } = {}) {
  const calls = { opens: 0, mode: null, closed: 0, clicks: 0, blobs: [], errors: [], aborted: 0 };
  const context = vm.createContext({
    Blob,
    console: { info() {}, warn() {}, error(...args) { calls.errors.push(args.map(String).join(' ')); } },
    setTimeout(callback) { callback(); },
    URL: { createObjectURL(blob) { calls.blobs.push(blob); return 'blob:local-test'; }, revokeObjectURL() {} },
    document: {
      createElement() { return { click() { calls.clicks += 1; }, remove() {} }; },
      documentElement: { appendChild() {} },
    },
  });
  const database = {
    name: 'NebulaLocalDB', version: 10, objectStoreNames: Object.keys(stores),
    close() { calls.closed += 1; },
    transaction(names, mode) {
      calls.mode = mode;
      let completed = 0;
      const tx = {
        objectStore(name) {
          assert(names.includes(name));
          return {
            getAll() {
              const request = {};
              queueMicrotask(() => {
                request.result = vm.runInContext(`JSON.parse(${JSON.stringify(JSON.stringify(stores[name]))})`, context);
                request.onsuccess();
                completed += 1;
                if (completed === names.length) {
                  if (failTransaction) tx.onabort();
                  else tx.oncomplete();
                }
              });
              return request;
            },
          };
        },
      };
      return tx;
    },
  };
  context.indexedDB = {
    async databases() { return exists ? [{ name: 'NebulaLocalDB', version: 10 }] : []; },
    open(...args) {
      calls.opens += 1;
      assert.deepEqual(args, ['NebulaLocalDB'], 'must never request a schema version upgrade');
      const request = { result: database, transaction: { abort() { calls.aborted += 1; } } };
      queueMicrotask(() => {
        if (creationRace) {
          request.onupgradeneeded();
          request.onerror();
        } else request.onsuccess();
      });
      return request;
    },
  };
  await vm.runInContext(source, context);
  return calls;
}

test('downloads a complete same-transaction snapshot without modifying database', async () => {
  const stores = {
    notes: [{ id: 'old', title: '中文', content: '<script>inert()</script>', isDeleted: true }],
    folders: [{ id: 'folder', name: '文件夹' }], tags: [{ id: 'tag' }], settings: [{ userId: 'anonymous-user' }],
  };
  const calls = await runExport({ stores });
  assert.equal(calls.mode, 'readonly');
  assert.equal(calls.opens, 1);
  assert.equal(calls.clicks, 1);
  assert.equal(calls.closed, 1);
  assert.deepEqual(calls.errors, []);
  const downloaded = JSON.parse(await calls.blobs[0].text());
  assert.equal(downloaded.format, 'nebula-legacy-indexeddb');
  assert.equal(downloaded.schemaVersion, 1);
  assert.equal(downloaded.database.version, 10);
  assert.deepEqual(downloaded.stores, stores);
});

test('does not create an empty database at the wrong origin', async () => {
  const calls = await runExport({ exists: false });
  assert.equal(calls.opens, 0);
  assert.equal(calls.clicks, 0);
  assert(calls.errors[0].includes('not found'));
});

test('aborts a creation race instead of leaving an empty replacement', async () => {
  const calls = await runExport({ creationRace: true });
  assert.equal(calls.aborted, 1);
  assert.equal(calls.clicks, 0);
  assert.equal(calls.errors.length, 1);
});

test('an interrupted transaction never downloads a partial export', async () => {
  const calls = await runExport({ stores: { notes: [{ id: 'one' }], tags: [] }, failTransaction: true });
  assert.equal(calls.mode, 'readonly');
  assert.equal(calls.closed, 1);
  assert.equal(calls.clicks, 0);
  assert.equal(calls.errors.length, 1);
});
