'use strict';

const assert = require('node:assert/strict');
const http = require('node:http');
const test = require('node:test');
const { run } = require('./bluey-account-delete.js');

async function withDeleteResponse(status, body, callback) {
  const server = http.createServer((request, response) => {
    assert.equal(request.method, 'POST');
    assert.equal(request.url, '/account/delete');
    response.writeHead(status, { 'content-type': 'application/json' });
    response.end(JSON.stringify(body));
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const address = server.address();
    assert(address && typeof address === 'object');
    await callback(`http://127.0.0.1:${address.port}/account/delete`);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

async function requestDelete(url) {
  const response = await fetch(url, { method: 'POST' });
  return { status: response.status, body: await response.json() };
}

test('202 pending deletion keeps browser credentials', async () => {
  await withDeleteResponse(202, {
    deleted: false,
    state: 'pending_runner_volume_purge',
    note: 'Runner cleanup is pending.',
  }, async (url) => {
    let token = 'access-token';
    let pendingMessage = '';
    const result = await run({
      request: () => requestDelete(url),
      onPending: (message) => { pendingMessage = message; },
      onDeleted: () => { token = ''; },
    });

    assert.equal(result.kind, 'pending');
    assert.equal(pendingMessage, 'Runner cleanup is pending.');
    assert.equal(token, 'access-token');
  });
});

test('200 verified hard deletion clears browser credentials', async () => {
  await withDeleteResponse(200, {
    deleted: true,
    state: 'deleted',
    deleted_at: '2026-08-05T12:00:00Z',
  }, async (url) => {
    let token = 'access-token';
    const result = await run({
      request: () => requestDelete(url),
      onPending: () => {},
      onDeleted: () => { token = ''; },
    });

    assert.equal(result.kind, 'deleted');
    assert.equal(token, '');
  });
});

test('202 deletion claim cannot clear browser credentials', async () => {
  await withDeleteResponse(202, {
    deleted: true,
    state: 'deleted',
    deleted_at: '2026-08-05T12:00:00Z',
  }, async (url) => {
    let token = 'access-token';
    await assert.rejects(
      run({
        request: () => requestDelete(url),
        onPending: () => {},
        onDeleted: () => { token = ''; },
      }),
      /inconsistent completion response/,
    );
    assert.equal(token, 'access-token');
  });
});
