import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const site = readFileSync(new URL('../assets/bluey-site.js', import.meta.url), 'utf8');
const styles = readFileSync(new URL('../assets/bluey-site.css', import.meta.url), 'utf8');

function sourceBetween(start, end) {
  const startIndex = site.indexOf(start);
  const endIndex = site.indexOf(end, startIndex + start.length);
  assert.notEqual(startIndex, -1, `missing source marker: ${start}`);
  assert.notEqual(endIndex, -1, `missing source marker: ${end}`);
  return site.slice(startIndex, endIndex);
}

test('manual device code entry is absent from signed-in navigation', () => {
  assert.doesNotMatch(html, /class="nav-connect"/);
  assert.doesNotMatch(html, /class="account-top-connect"/);
  assert.doesNotMatch(html, /id="accountTopJoinForm"/);
  assert.equal(html.match(/name="user_code"/g)?.length, 1);
});

test('the public fallback form stays behind an explicit troubleshooting disclosure', () => {
  assert.match(
    html,
    /<details class="hero-connect">\s*<summary>Having trouble connecting\?<\/summary>[\s\S]*?id="productJoinForm"/,
  );
  assert.match(html, /<button type="submit">Continue in browser<\/button>/);
  assert.match(html, /Use this only if Bluey could not open the browser with its one-time code\./);
});

test('entering a public fallback code continues to the confirmation page without approving', () => {
  const joinFlow = sourceBetween(
    'function initProductJoinForm()',
    'async function refreshAccountToken()',
  );
  assert.match(joinFlow, /const route = accountToken\(\) \? '\/account' : '\/login';/);
  assert.match(joinFlow, /user_code=\$\{encodeURIComponent\(code\)\}/);
  assert.doesNotMatch(joinFlow, /auth\/device\/approve/);
});

test('a carried code is not shown or requested again on the primary path', () => {
  const rendering = sourceBetween(
    'function renderDeviceLinkHint()',
    'async function approvePendingDevice()',
  );
  assert.match(rendering, /document\.createElement\('details'\)/);
  assert.match(rendering, /title\.textContent = 'Continue in browser';/);
  assert.match(rendering, /title\.textContent = 'Connect this Bluey\?';/);
  assert.match(rendering, /You do not need to re-enter a code\./);
  assert.match(rendering, /if \(!code\) \{\s*if \(accountToken\(\)\)/);
  assert.doesNotMatch(rendering, /Fallback code: \$\{code\}/);
  assert.doesNotMatch(rendering, /aria-label[^\n]*\$\{code\}/);
});

test('closed troubleshooting disclosures conceal their content from rendering', () => {
  const closedDisclosureRule = new RegExp(
    '#accountApp \\.device-troubleshooting:not\\(\\[open\\]\\) > :not\\(summary\\),'
      + '\\s*#productSite \\.hero-connect:not\\(\\[open\\]\\) > :not\\(summary\\)'
      + ' \\{\\s*display: none;',
  );
  assert.match(
    styles,
    closedDisclosureRule,
  );
});

test('device-link cards remain readable in the light theme', () => {
  const lightInputRule = new RegExp(
    'html\\[data-bluey-theme="light"\\] #accountApp \\.device-code-form input \\{'
      + '[\\s\\S]*?background: #ffffff;[\\s\\S]*?color: #102130;',
  );
  assert.match(
    styles,
    /html\[data-bluey-theme="light"\] #accountApp \.device-link-hint strong,[\s\S]*?color: #102130;/,
  );
  assert.match(styles, lightInputRule);
});

test('empty computer guidance keeps carried-code linking as the normal path', () => {
  assert.match(site, /Open Bluey and continue in the browser it opens\./);
  assert.doesNotMatch(site, /Open the host overlay and enter its code above\./);
});

test('device approval remains gated by the explicit Connect this Bluey action', () => {
  const rendering = sourceBetween(
    'function renderDeviceLinkHint()',
    'async function approvePendingDevice()',
  );
  const approval = sourceBetween(
    'async function approvePendingDevice()',
    'async function openDesktopDeepLinkIfNeeded()',
  );
  const accountLoad = sourceBetween('async function loadAccount()', 'async function startReload()');
  assert.match(rendering, /markPendingDeviceConfirmed\(code\);/);
  assert.match(approval, /if \(!isPendingDeviceConfirmed\(code\)\)/);
  assert.match(approval, /apiJson\('\/auth\/device\/approve'/);
  assert.doesNotMatch(accountLoad, /approvePendingDevice\(\)/);
  assert.doesNotMatch(site, /console\.(?:log|info|debug|warn|error)\([^\n]*\bcode\b/);
});

test('the /link deep-link mint requires a fresh explicit browser action', () => {
  const rendering = sourceBetween(
    'function renderDeviceLinkHint()',
    'async function approvePendingDevice()',
  );
  const launcher = sourceBetween(
    'async function openDesktopDeepLinkIfNeeded()',
    'async function accountAuth(mode)',
  );
  const accountLoad = sourceBetween('async function loadAccount()', 'async function startReload()');
  assert.match(rendering, /currentPath === '\/link'/);
  assert.match(rendering, /markDesktopDeepLinkConfirmed\(\);/);
  assert.match(rendering, /openDesktopDeepLinkIfNeeded\(\)/);
  assert.match(launcher, /if \(!isDesktopDeepLinkConfirmed\(\)\)/);
  assert.match(launcher, /apiJson\('\/auth\/link\/mint'/);
  assert.doesNotMatch(accountLoad, /openDesktopDeepLinkIfNeeded\(\)/);
});

test('device handoff remains functional when sessionStorage is unavailable', () => {
  const helpers = sourceBetween('function normalizeDeviceCode(value)', 'function pendingSessionId()');
  const location = { pathname: '/login', search: '?user_code=ABCD-EFGH', hash: '' };
  const sandbox = {
    Date,
    JSON,
    URLSearchParams,
    location,
    history: {
      state: null,
      replaceState(_state, _title, nextUrl) {
        location.search = new URL(nextUrl, 'https://bluey.sh').search;
      },
    },
    sessionStorage: {
      getItem() { throw new Error('storage disabled'); },
      setItem() { throw new Error('storage disabled'); },
      removeItem() { throw new Error('storage disabled'); },
    },
    result: null,
  };
  runInNewContext(`
    const DEVICE_LINK_TTL_MS = 10 * 60 * 1000;
    const DEVICE_APPROVAL_WAIT_MS = 45 * 1000;
    const DEVICE_LINK_STORAGE_KEY = 'bluey_pending_device_code';
    const DESKTOP_DEEP_LINK_CONFIRM_KEY = 'bluey_desktop_deep_link_confirmed';
    const DESKTOP_DEEP_LINK_STARTED_KEY = 'bluey_desktop_deep_link_started';
    let pendingDeviceMemoryRecord = null;
    let pendingDeviceConfirmationMemory = '';
    let pendingDeviceApprovalMemory = null;
    let desktopDeepLinkConfirmedMemory = false;
    let desktopDeepLinkStartedMemory = false;
    ${helpers}
    const firstCode = pendingDeviceCode();
    const scrubbedQuery = location.search;
    const rememberedCode = pendingDeviceCode();
    markPendingDeviceConfirmed(firstCode);
    markPendingDeviceApproved(firstCode);
    markDesktopDeepLinkConfirmed();
    result = {
      firstCode,
      scrubbedQuery,
      rememberedCode,
      deviceConfirmed: isPendingDeviceConfirmed(firstCode),
      deviceApproved: pendingDeviceApprovedAt(firstCode) > 0,
      deepLinkConfirmed: isDesktopDeepLinkConfirmed(),
    };
  `, sandbox);
  assert.deepEqual(
    { ...sandbox.result },
    {
      firstCode: 'ABCD-EFGH',
      scrubbedQuery: '',
      rememberedCode: 'ABCD-EFGH',
      deviceConfirmed: true,
      deviceApproved: true,
      deepLinkConfirmed: true,
    },
  );
});
