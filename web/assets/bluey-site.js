if (!window.__BLUEY_SITE_BOOTED__) {
  window.__BLUEY_SITE_BOOTED__ = true;

    const policyApp = document.getElementById('policyApp');
    const accountApp = document.getElementById('accountApp');
    const downloadApp = document.getElementById('downloadApp');
    const productSite = document.getElementById('productSite');
    const downloadRoutes = new Set(['/download', '/install']);
    const accountRoutes = new Set(['/account', '/reload', '/link', '/login', '/device', '/verify-email', '/password-reset']);
    const policyRoutes = new Set(['/privacy', '/terms', '/docs/privacy', '/docs/terms', '/docs/disguise']);
    function normalizeRoutePath(path) {
      const value = String(path || '').trim();
      if (!value || !value.startsWith('/')) return '';
      const pathOnly = value.split('#')[0].split('?')[0];
      return pathOnly.replace(/\/+$/, '') || '/';
    }

    const filePreviewRoute = window.location.protocol === 'file:'
      ? normalizeRoutePath(new URLSearchParams(window.location.search).get('route'))
      : '';
    const currentPath = window.location.protocol === 'file:'
      ? filePreviewRoute || '/'
      : normalizeRoutePath(window.location.pathname) || '/';
    const SITE_THEME_STORAGE_KEY = 'bluey_site_theme';
    const isAccountRoute = accountRoutes.has(currentPath);
    const isPolicyRoute = policyRoutes.has(currentPath);
    const isDownloadRoute = downloadRoutes.has(currentPath);
    let pendingSignupEmail = '';
    let pendingTrialConvertEmail = '';
    let trialCredentials = null;
    let accountAuthMode = 'login';
    let squareCard = null;
    let squareCardEnvironment = '';
    let squareCardSetupId = '';
    let squareCardContainerId = '';
    let squareCardSetupPromise = null;
    let squareCardAttached = false;
    let refreshAccountTokenPromise = null;
    let currentAccountEmail = '';
    let currentAccountIsAdmin = false;
    let adminAbuseLoaded = false;
    let accountSignOutInProgress = false;
    let accountAuthEpoch = 0;
    let latestAccountForBilling = null;
    let confirmActionResolve = null;
    let lastLinkedComputerCount = 0;
    let accountBalancePollTimer = null;
    const AUTO_RELOAD_MIN_CENTS = 1500;
    const AUTO_RELOAD_MAX_CENTS = 50000;
    const AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 500;
    const AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 1500;
    const LEGACY_AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 1000;
    const LEGACY_AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 3000;
    const MANUAL_RELOAD_MIN_CENTS = 1500;
    const MANUAL_RELOAD_MAX_CENTS = 50000;
    const MANUAL_RELOAD_AMOUNT_CENTS = 1500;
    const MIXED_USE_LOW_CENTS_PER_HOUR = 300;
    const MIXED_USE_HIGH_CENTS_PER_HOUR = 750;
    const AUTO_ROUTING_USAGE_HINT = 'Bluey routes each request by task and context, so audio, screen, files, and deeper routes can spend faster.';
    let captchaConfigPromise = null;
    let captchaConfig = { provider: null, site_key: null };
    let signupTurnstileWidgetId = null;
    let signupTurnstileToken = '';
    let trialTurnstileWidgetId = null;
    let trialTurnstileToken = '';
    let pendingTrialButton = null;
    const DEVICE_LINK_TTL_MS = 10 * 60 * 1000;
    const DEVICE_APPROVAL_WAIT_MS = 45 * 1000;
    const DEVICE_LINK_STORAGE_KEY = 'bluey_pending_device_code';
    const AUTO_RELOAD_SETUP_OPT_OUT_PREFIX = 'bluey_auto_reload_setup_opt_out:';
    const ACCESS_TOKEN_KEY = 'bluey_access_token';
    const REFRESH_TOKEN_KEY = 'bluey_refresh_token';
    const AUTH_PERSISTENCE_KEY = 'bluey_auth_persistence';

    function money(cents) {
      return `$${(Number(cents || 0) / 100).toFixed(2)}`;
    }

    function shortMoney(cents) {
      const value = Number(cents || 0) / 100;
      return Number.isInteger(value) ? `$${value.toFixed(0)}` : `$${value.toFixed(2)}`;
    }

    function paymentRequestId() {
      if (window.crypto?.randomUUID) return window.crypto.randomUUID();
      return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    }

    function formatApproxDays(days) {
      const value = Number(days || 0);
      if (!Number.isFinite(value) || value <= 0) return '';
      if (value < 1) return '<1 day';
      if (value < 14) return `~${Math.max(1, Math.round(value))} days`;
      if (value < 60) return `~${Math.max(2, Math.round(value / 7))} weeks`;
      if (value < 90) return `~${Math.round(value)} days`;
      return '90+ days';
    }

    function formatApproxHoursRange(cents) {
      const amount = Math.max(MANUAL_RELOAD_MIN_CENTS, Number(cents || MANUAL_RELOAD_AMOUNT_CENTS));
      const low = Math.max(1, Math.floor(amount / MIXED_USE_HIGH_CENTS_PER_HOUR));
      const high = Math.max(low, Math.ceil(amount / MIXED_USE_LOW_CENTS_PER_HOUR));
      return `~${low}-${high} hrs`;
    }

    function formatAverageCost(cents) {
      const value = Number(cents || 0);
      if (!Number.isFinite(value) || value <= 0) return '';
      if (value < 1) return '<$0.01 avg per paid request';
      return `~${money(value)} avg per paid request`;
    }

    function usageEstimate(usage, me) {
      const periodDays = Math.max(1, Number(usage?.period_days || 7));
      const totalCues = Math.max(0, Number(usage?.total_cues || 0));
      const spentCents = Math.max(0, Number(usage?.total_cents_spent || 0));
      const balanceCents = Math.max(0, Number(me?.balance_cents || 0));
      const reloadCents = Math.max(MANUAL_RELOAD_MIN_CENTS, Number(me?.auto_topup_amount_cents || MANUAL_RELOAD_AMOUNT_CENTS));
      const thresholdCents = Math.max(100, Number(me?.auto_topup_threshold_cents || AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS));
      const avgCostCents = totalCues > 0 && spentCents > 0
        ? spentCents / totalCues
        : 0;
      const centsPerDay = spentCents > 0 ? spentCents / periodDays : 0;
      const balanceDays = centsPerDay > 0 ? balanceCents / centsPerDay : 0;
      const reloadDays = centsPerDay > 0 ? reloadCents / centsPerDay : 0;
      const hasRecentPace = totalCues >= 3 && spentCents > 0;
      return {
        periodDays,
        totalCues,
        spentCents,
        balanceCents,
        reloadCents,
        thresholdCents,
        avgCostCents,
        defaultUseCopy: formatApproxHoursRange(MANUAL_RELOAD_AMOUNT_CENTS),
        reloadUseCopy: formatApproxHoursRange(reloadCents),
        balanceDays,
        reloadDays,
        hasRecentPace,
        autoReloadOn: Boolean(me?.auto_topup_enabled),
      };
    }

    function centsToDollars(cents) {
      return String(Math.round(Number(cents || 0) / 100));
    }

    function dollarsToCents(value) {
      const dollars = Number(value);
      if (!Number.isFinite(dollars)) return NaN;
      return Math.round(dollars * 100);
    }

    function cleanDollarInput(value) {
      const raw = String(value || '');
      const numeric = raw.replace(/[^\d.]/g, '');
      const parts = numeric.split('.');
      if (parts.length <= 1) return parts[0] || '';
      return `${parts[0]}.${parts.slice(1).join('')}`;
    }

    function billingMoneyFieldLimits(inputId) {
      if (inputId === 'manualReloadAmount' || inputId === 'modalReloadAmount') {
        return {
          minCents: MANUAL_RELOAD_MIN_CENTS,
          maxCents: MANUAL_RELOAD_MAX_CENTS,
          fallbackCents: MANUAL_RELOAD_AMOUNT_CENTS,
        };
      }
      if (inputId === 'autoReloadThreshold' || inputId === 'modalAutoReloadThreshold') {
        return {
          minCents: 100,
          maxCents: 5000,
          fallbackCents: AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS,
        };
      }
      if (inputId === 'autoReloadAmount' || inputId === 'modalAutoReloadAmount') {
        return {
          minCents: AUTO_RELOAD_MIN_CENTS,
          maxCents: AUTO_RELOAD_MAX_CENTS,
          fallbackCents: AUTO_RELOAD_DEFAULT_AMOUNT_CENTS,
        };
      }
      return null;
    }

    function clampCents(value, minCents, maxCents) {
      return Math.max(minCents, Math.min(maxCents, value));
    }

    function sanitizeDollarInput(input, minCents, maxCents, options = {}) {
      if (!input) return NaN;
      const cleaned = cleanDollarInput(input.value);
      if (input.value !== cleaned) input.value = cleaned;
      if (!cleaned) return NaN;
      const cents = dollarsToCents(cleaned);
      if (!Number.isFinite(cents)) return NaN;
      if (!options.clamp && cents >= minCents && (!Number.isFinite(maxCents) || cents <= maxCents)) {
        return cents;
      }
      const clamped = clampCents(cents, minCents, maxCents);
      input.value = centsToDollars(clamped);
      return clamped;
    }

    function normalizeBillingMoneyInput(inputId) {
      const input = document.getElementById(inputId);
      const limits = billingMoneyFieldLimits(inputId);
      if (!input || !limits) return;
      const cents = sanitizeDollarInput(input, limits.minCents, limits.maxCents, { clamp: true });
      if (Number.isFinite(cents)) return;
      let fallback = dollarsToCents(centsToDollars(limits.fallbackCents));
      if (!Number.isFinite(fallback)) fallback = limits.fallbackCents;
      input.value = centsToDollars(clampCents(fallback, limits.minCents, limits.maxCents));
    }

    function installBillingMoneyInputGuards() {
      [
        'manualReloadAmount',
        'modalReloadAmount',
        'autoReloadThreshold',
        'autoReloadAmount',
        'modalAutoReloadThreshold',
        'modalAutoReloadAmount',
      ].forEach((inputId) => {
        const input = document.getElementById(inputId);
        if (!input || input.dataset.billingMoneyGuard === '1') return;
        input.dataset.billingMoneyGuard = '1';
        input.addEventListener('beforeinput', (event) => {
          if (event.data && /[eE+-]/.test(event.data)) event.preventDefault();
        });
        input.addEventListener('keydown', (event) => {
          if (['e', 'E', '+', '-'].includes(event.key)) event.preventDefault();
        });
        input.addEventListener('paste', (event) => {
          event.preventDefault();
          const pasted = event.clipboardData?.getData('text') || '';
          input.value = cleanDollarInput(pasted);
          input.dispatchEvent(new Event('input', { bubbles: true }));
        });
        input.addEventListener('input', () => {
          const cleaned = cleanDollarInput(input.value);
          if (input.value !== cleaned) input.value = cleaned;
        });
      });
    }

    function accountToken() {
      return localStorage.getItem(ACCESS_TOKEN_KEY) || sessionStorage.getItem(ACCESS_TOKEN_KEY) || '';
    }

    function accountRefreshToken() {
      return localStorage.getItem(REFRESH_TOKEN_KEY) || sessionStorage.getItem(REFRESH_TOKEN_KEY) || '';
    }

    function accountTokenPersists() {
      return localStorage.getItem(AUTH_PERSISTENCE_KEY) !== 'session' && !sessionStorage.getItem(ACCESS_TOKEN_KEY);
    }

    function normalizeDeviceCode(value) {
      return String(value || '')
        .trim()
        .toUpperCase()
        .replace(/[^A-Z0-9-]/g, '')
        .slice(0, 32);
    }

    function wait(ms) {
      return new Promise((resolve) => setTimeout(resolve, ms));
    }

    function clearDeviceCodeUrlParams() {
      const params = new URLSearchParams(location.search);
      if (!params.has('user_code') && !params.has('device_code')) return;
      try {
        params.delete('user_code');
        params.delete('device_code');
        const query = params.toString();
        const nextUrl = `${location.pathname}${query ? `?${query}` : ''}${location.hash || ''}`;
        history.replaceState(history.state, '', nextUrl);
      } catch {
        // If history is unavailable, the stored handoff expiry still prevents stale prompts.
      }
    }

    function autoReloadSetupOptOutKey(me = latestAccountForBilling) {
      const email = String(me?.email || currentAccountEmail || '').trim().toLowerCase();
      return email ? `${AUTO_RELOAD_SETUP_OPT_OUT_PREFIX}${email}` : '';
    }

    function autoReloadSetupOptedOut(me = latestAccountForBilling) {
      const key = autoReloadSetupOptOutKey(me);
      return key ? localStorage.getItem(key) === '1' : false;
    }

    function setAutoReloadSetupOptOut(enabled, me = latestAccountForBilling) {
      const key = autoReloadSetupOptOutKey(me);
      if (!key) return;
      if (enabled) {
        localStorage.setItem(key, '1');
      } else {
        localStorage.removeItem(key);
      }
    }

    function rememberPendingDeviceCode(code) {
      if (!code) return;
      try {
        let savedAt = Date.now();
        const raw = sessionStorage.getItem(DEVICE_LINK_STORAGE_KEY);
        if (raw) {
          const parsed = JSON.parse(raw);
          const existingCode = normalizeDeviceCode(parsed?.code);
          const existingSavedAt = Number(parsed?.saved_at || 0);
          if (existingCode === code && existingSavedAt) {
            savedAt = existingSavedAt;
          }
        }
        sessionStorage.setItem(DEVICE_LINK_STORAGE_KEY, JSON.stringify({
          code,
          saved_at: savedAt,
        }));
      } catch {
        // Browser storage can be disabled; the query-param path still works.
      }
    }

    function storedPendingDeviceRecord() {
      try {
        const raw = sessionStorage.getItem(DEVICE_LINK_STORAGE_KEY);
        if (!raw) return null;
        const parsed = JSON.parse(raw);
        const code = normalizeDeviceCode(parsed?.code);
        const savedAt = Number(parsed?.saved_at || 0);
        if (!code || !savedAt || Date.now() - savedAt > DEVICE_LINK_TTL_MS) {
          sessionStorage.removeItem(DEVICE_LINK_STORAGE_KEY);
          return null;
        }
        return { code, savedAt };
      } catch {
        sessionStorage.removeItem(DEVICE_LINK_STORAGE_KEY);
        return null;
      }
    }

    function storedPendingDeviceCode() {
      return storedPendingDeviceRecord()?.code || '';
    }

    function deviceApprovalStorageKey(code) {
      return `bluey_device_approved_${normalizeDeviceCode(code)}`;
    }

    function deviceConfirmStorageKey(code) {
      return `bluey_device_confirmed_${normalizeDeviceCode(code)}`;
    }

    function markPendingDeviceApproved(code) {
      const normalized = normalizeDeviceCode(code);
      if (!normalized) return;
      try {
        sessionStorage.setItem(deviceApprovalStorageKey(normalized), String(Date.now()));
      } catch {
        // Storage is optional; the server-side approval is the source of truth.
      }
    }

    function pendingDeviceApprovedAt(code) {
      const normalized = normalizeDeviceCode(code);
      if (!normalized) return 0;
      try {
        const raw = sessionStorage.getItem(deviceApprovalStorageKey(normalized));
        if (!raw || raw === '1') return 0;
        const value = Number(raw);
        return Number.isFinite(value) && value > 0 ? value : 0;
      } catch {
        return 0;
      }
    }

    function isPendingDeviceApprovalFresh(code) {
      const approvedAt = pendingDeviceApprovedAt(code);
      return approvedAt > 0 && Date.now() - approvedAt <= DEVICE_APPROVAL_WAIT_MS;
    }

    function clearPendingDeviceCode(code = '') {
      const normalized = normalizeDeviceCode(code) || storedPendingDeviceCode();
      try {
        sessionStorage.removeItem(DEVICE_LINK_STORAGE_KEY);
        if (normalized) {
          sessionStorage.removeItem(deviceConfirmStorageKey(normalized));
          sessionStorage.removeItem(deviceApprovalStorageKey(normalized));
        }
      } catch {
        // Storage cleanup is best-effort.
      }
      clearDeviceCodeUrlParams();
    }

    function pendingDeviceCode() {
      const params = new URLSearchParams(location.search);
      const code = normalizeDeviceCode(params.get('user_code') || params.get('device_code') || '');
      if (code) {
        rememberPendingDeviceCode(code);
        clearDeviceCodeUrlParams();
        return code;
      }
      return storedPendingDeviceCode();
    }

    function pendingSessionId() {
      const id = new URLSearchParams(location.search).get('session') || '';
      return String(id).trim().slice(0, 160);
    }

    function setAccountToken(auth, remember = accountTokenPersists()) {
      accountSignOutInProgress = false;
      accountAuthEpoch += 1;
      const target = remember ? localStorage : sessionStorage;
      const other = remember ? sessionStorage : localStorage;
      other.removeItem(ACCESS_TOKEN_KEY);
      other.removeItem(REFRESH_TOKEN_KEY);
      target.setItem(ACCESS_TOKEN_KEY, auth.access_token);
      target.setItem(REFRESH_TOKEN_KEY, auth.refresh_token || '');
      localStorage.setItem(AUTH_PERSISTENCE_KEY, remember ? 'local' : 'session');
      syncAccountNav();
    }

    function clearAccountToken() {
      clearPendingDeviceCode();
      localStorage.removeItem(ACCESS_TOKEN_KEY);
      localStorage.removeItem(REFRESH_TOKEN_KEY);
      localStorage.removeItem(AUTH_PERSISTENCE_KEY);
      sessionStorage.removeItem(ACCESS_TOKEN_KEY);
      sessionStorage.removeItem(REFRESH_TOKEN_KEY);
      currentAccountEmail = '';
      currentAccountIsAdmin = false;
      adminAbuseLoaded = false;
      refreshAccountTokenPromise = null;
      syncAccountNav();
    }

    function browserTrialDeviceId() {
      const key = 'bluey_trial_device_id';
      let id = localStorage.getItem(key) || '';
      if (!id) {
        id = (window.crypto && window.crypto.randomUUID)
          ? window.crypto.randomUUID()
          : `browser-${Date.now()}-${Math.random().toString(16).slice(2)}`;
        localStorage.setItem(key, id);
      }
      return id;
    }

    async function revokeBrowserSession(accessToken = accountToken(), refreshToken = accountRefreshToken()) {
      if (!accessToken) return;
      try {
        await fetch('/auth/logout', {
          method: 'POST',
          headers: {
            'Authorization': `Bearer ${accessToken}`,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ refresh_token: refreshToken || null }),
        });
      } catch {
        // Always let local sign-out complete. Stale or already-revoked server
        // tokens are harmless once the browser copy is cleared.
      }
    }

    async function signOut() {
      accountSignOutInProgress = true;
      accountAuthEpoch += 1;
      const accessToken = accountToken();
      const refreshToken = accountRefreshToken();
      clearAccountToken();
      closeProfileMenu();
      if (isAccountRoute) {
        setAccountChrome(false);
        loadAccount().catch((error) => accountMessage(error.message));
      }
      await revokeBrowserSession(accessToken, refreshToken);
    }

    function syncAccountNav() {
      const authed = Boolean(accountToken());
      document.querySelectorAll('[data-guest-only]').forEach((el) => {
        el.hidden = authed;
      });
      document.querySelectorAll('[data-auth-only]').forEach((el) => {
        el.hidden = !authed;
      });
      if (!authed) {
        closeProfileMenu();
      }
    }

    function closeProfileMenu(exceptProfile = null) {
      const profiles = document.querySelectorAll('[data-account-profile]');
      if (!profiles.length) {
        const button = document.getElementById('accountProfileButton');
        const menu = document.getElementById('accountProfileMenu');
        if (button) button.setAttribute('aria-expanded', 'false');
        if (menu) menu.hidden = true;
        return;
      }
      profiles.forEach((profile) => {
        if (exceptProfile && profile === exceptProfile) return;
        const button = profile.querySelector('[data-account-profile-button]');
        const menu = profile.querySelector('[data-account-profile-menu]');
        if (button) button.setAttribute('aria-expanded', 'false');
        if (menu) menu.hidden = true;
      });
    }

    function toggleProfileMenu(profile = null) {
      const root = profile || document;
      const button = root.querySelector?.('[data-account-profile-button]') || document.getElementById('accountProfileButton');
      const menu = root.querySelector?.('[data-account-profile-menu]') || document.getElementById('accountProfileMenu');
      if (!button || !menu) return;
      const open = menu.hidden;
      closeProfileMenu(open ? profile : null);
      menu.hidden = !open;
      button.setAttribute('aria-expanded', open ? 'true' : 'false');
    }

    function handleSignOutClick(event) {
      event?.preventDefault?.();
      event?.stopPropagation?.();
      event?.stopImmediatePropagation?.();
      closeProfileMenu();
      signOut().catch(() => {
        clearAccountToken();
        if (isAccountRoute) setAccountChrome(false);
      });
    }

    function initProfileMenus() {
      document.querySelectorAll('[data-account-profile-button], #accountProfileButton').forEach((button) => {
        if (button.dataset.profileReady === '1') return;
        button.dataset.profileReady = '1';
        button.addEventListener('click', (event) => {
          event.preventDefault();
          event.stopPropagation();
          toggleProfileMenu(button.closest('[data-account-profile]'));
        });
      });
      document.querySelectorAll('[data-account-profile-menu], #accountProfileMenu').forEach((menu) => {
        if (menu.dataset.profileMenuReady === '1') return;
        menu.dataset.profileMenuReady = '1';
        menu.addEventListener('click', (event) => {
          const signOutButton = event.target.closest('[data-sign-out]');
          if (signOutButton) {
            handleSignOutClick(event);
            return;
          }
          event.stopPropagation();
        });
      });
      document.querySelectorAll('[data-sign-out]').forEach((button) => {
        if (button.dataset.signOutReady === '1') return;
        button.dataset.signOutReady = '1';
        button.addEventListener('click', handleSignOutClick);
      });
    }

    function filePreviewHref(route) {
      return `${window.location.pathname}?route=${encodeURIComponent(normalizeRoutePath(route) || '/')}`;
    }

    function accountSessionHref(sessionId) {
      const id = String(sessionId || '').trim();
      const base = window.location.protocol === 'file:' ? filePreviewHref('/account') : '/account';
      if (!id) return `${base}#history`;
      const separator = base.includes('?') ? '&' : '?';
      return `${base}${separator}session=${encodeURIComponent(id)}#history`;
    }

    function initFilePreview() {
      if (window.location.protocol !== 'file:') return;

      document.querySelectorAll('img[src^="/assets/"]').forEach((img) => {
        img.setAttribute('src', img.getAttribute('src').replace(/^\/assets\//, 'assets/'));
      });
      document.querySelectorAll('a[href^="/"]').forEach((link) => {
        const href = link.getAttribute('href');
        if (!href || href.startsWith('/assets/')) return;
        link.setAttribute('href', filePreviewHref(href));
      });
      document.querySelectorAll('form[action^="/"]').forEach((form) => {
        const action = normalizeRoutePath(form.getAttribute('action')) || '/';
        form.setAttribute('action', window.location.pathname);
        if (String(form.method || '').toLowerCase() !== 'get') return;
        let routeInput = form.querySelector('input[name="route"][data-file-preview-route]');
        if (!routeInput) {
          routeInput = document.createElement('input');
          routeInput.type = 'hidden';
          routeInput.name = 'route';
          routeInput.dataset.filePreviewRoute = '1';
          form.prepend(routeInput);
        }
        routeInput.value = action;
      });
    }

    function storedSiteTheme() {
      try {
        return localStorage.getItem(SITE_THEME_STORAGE_KEY) === 'light' ? 'light' : 'dark';
      } catch {
        return 'dark';
      }
    }

    function applySiteTheme(theme) {
      const normalized = theme === 'light' ? 'light' : 'dark';
      document.documentElement.dataset.blueyTheme = normalized;
      document.documentElement.style.colorScheme = normalized;
      document.querySelectorAll('.theme-toggle').forEach((button) => {
        const isLight = normalized === 'light';
        button.setAttribute('aria-pressed', isLight ? 'true' : 'false');
        button.setAttribute('aria-label', isLight ? 'Switch to dark theme' : 'Switch to light theme');
        button.title = isLight ? 'Switch to dark theme' : 'Switch to light theme';
      });
    }

    function setSiteTheme(theme) {
      const normalized = theme === 'light' ? 'light' : 'dark';
      try {
        localStorage.setItem(SITE_THEME_STORAGE_KEY, normalized);
      } catch {
        // Theme still applies for the current page even when storage is blocked.
      }
      applySiteTheme(normalized);
    }

    function initSiteTheme() {
      applySiteTheme(storedSiteTheme());
      document.querySelectorAll('.theme-toggle').forEach((button) => {
        if (button.dataset.themeReady === '1') return;
        button.dataset.themeReady = '1';
        button.addEventListener('click', () => {
          setSiteTheme(storedSiteTheme() === 'light' ? 'dark' : 'light');
        });
      });
    }

    function initProductJoinForm() {
      document.querySelectorAll('[data-connect-code-form], #productJoinForm').forEach((form) => {
        if (!form || form.dataset.joinReady === '1') return;
        form.dataset.joinReady = '1';
        form.addEventListener('submit', (event) => {
          const input = form.querySelector('input[name="user_code"]');
          const code = normalizeDeviceCode(input?.value || '');
          if (!code) {
            event.preventDefault();
            input?.focus();
            return;
          }
          input.value = code;
          rememberPendingDeviceCode(code);
          if (accountToken()) {
            event.preventDefault();
            approveProductConnectCode(code, form, form.dataset.statusTarget || '');
          }
        });
      });
    }

    function connectCodeStatus(targetId, text, tone = '') {
      const el = targetId ? document.getElementById(targetId) : null;
      if (el) {
        el.textContent = text || '';
        el.dataset.tone = tone || '';
        return;
      }
      if (isAccountRoute) {
        accountMessage(text, false, tone);
      }
    }

    async function approveProductConnectCode(code, form, statusTargetId = '') {
      const button = form?.querySelector('button[type="submit"]');
      const previousText = button?.textContent || 'Connect';
      if (button) {
        button.disabled = true;
        button.textContent = 'Connecting...';
      }
      connectCodeStatus(statusTargetId, 'Connecting this account to Bluey desktop...');
      try {
        await apiJson('/auth/device/approve', {
          method: 'POST',
          body: JSON.stringify({ user_code: code }),
        });
        markPendingDeviceApproved(code);
        connectCodeStatus(statusTargetId, 'Connected. Return to Bluey desktop; it will finish automatically.', 'success');
      } catch (error) {
        if (!accountToken()) {
          window.location.href = `/login?user_code=${encodeURIComponent(code)}`;
          return;
        }
        connectCodeStatus(statusTargetId, `Could not connect: ${error.message}`, 'error');
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previousText;
        }
      }
    }

    function bootBlueyTerminal() {
      const term = document.getElementById('blueyTerminal');
      if (!term || term.dataset.booted === '1') return;
      term.dataset.booted = '1';

      const scenes = [
        {
          cmd: 'bluey on',
          lines: [
            { t: '', d: 180 },
            { t: '<span class="blue">[bluey]</span> starting desktop overlay', d: 220 },
            { t: '<span class="green">[ok]</span> overlay pill ready', d: 300 },
            { t: '<span class="green">[ok]</span> private session ready', d: 300 },
            { t: '<span class="green">[ok]</span> running in background', d: 320 },
            { t: '<span class="dim">Close this terminal. Bluey continues in the background.</span>', d: 360 },
          ],
        },
        {
          cmd: 'bluey listen',
          lines: [
            { t: '', d: 160 },
            { t: '<span class="label">system</span> <span class="value">transcript ready</span>', d: 260 },
            { t: '<span class="label">mic</span> <span class="value">transcript ready</span>', d: 260 },
            { t: '<span class="green">[ok]</span> stops after silence to control cost', d: 300 },
          ],
        },
        {
          cmd: 'bluey ask "what is the plan?"',
          lines: [
            { t: '', d: 160 },
            { t: '<span class="label">context</span> transcript + docs + screen', d: 260 },
            { t: '<span class="green">[stream]</span> answer appears in Bluey', d: 320 },
          ],
        },
      ];

      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const typeText = async (text, el) => {
        for (let i = 0; i < text.length; i += 1) {
          el.textContent += text[i];
          await sleep(28 + Math.random() * 18);
        }
      };
      const renderCommandLine = (cmd, cursor = false) => {
        term.innerHTML = `<span class="prompt">$ </span><span class="cmd">${cmd}</span>${cursor ? '<span class="terminal-cursor"></span>' : ''}`;
      };
      const playScene = async (scene) => {
        if (!term.isConnected) return;
        term.innerHTML = '<span class="prompt">$ </span><span class="cmd" id="blueyTyping"></span><span class="terminal-cursor"></span>';
        await sleep(280);
        await typeText(scene.cmd, document.getElementById('blueyTyping'));
        await sleep(260);
        renderCommandLine(scene.cmd);
        for (const line of scene.lines) {
          await sleep(line.d);
          term.innerHTML += `<br>${line.t}`;
        }
        await sleep(2100);
        renderCommandLine(scene.cmd, true);
        await sleep(350);
      };

      (async () => {
        let index = 0;
        while (term.isConnected) {
          await playScene(scenes[index % scenes.length]);
          index += 1;
          await sleep(500);
        }
      })();
    }

    async function refreshAccountToken() {
      if (refreshAccountTokenPromise) return refreshAccountTokenPromise;
      refreshAccountTokenPromise = (async () => {
        const epoch = accountAuthEpoch;
        const refreshToken = accountRefreshToken();
        if (!refreshToken || accountSignOutInProgress) return '';
        const response = await fetch('/auth/refresh', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ refresh_token: refreshToken }),
        });
        let body = null;
        try {
          body = await response.json();
        } catch {
          body = null;
        }
        if (!response.ok || !body?.access_token) {
          clearAccountToken();
          return '';
        }
        if (accountSignOutInProgress || epoch !== accountAuthEpoch || !accountRefreshToken()) {
          return '';
        }
        setAccountToken(body, accountTokenPersists());
        return body.access_token;
      })();
      try {
        return await refreshAccountTokenPromise;
      } finally {
        refreshAccountTokenPromise = null;
      }
    }

    async function apiJson(path, options = {}, hasRetriedAuth = false) {
      const { skipAuthRefresh, ...fetchOptions } = options;
      const headers = new Headers(options.headers || {});
      headers.set('Content-Type', 'application/json');
      const token = accountToken();
      if (token) headers.set('Authorization', `Bearer ${token}`);
      const response = await fetch(path, { ...fetchOptions, headers });
      let body = null;
      try {
        body = await response.json();
      } catch {
        body = null;
      }
      if (response.status === 401 && !hasRetriedAuth && !skipAuthRefresh) {
        const refreshed = await refreshAccountToken();
        if (refreshed) {
          return apiJson(path, options, true);
        }
      }
      if (!response.ok) {
        throw new Error(body?.error || `${response.status} ${response.statusText}`);
      }
      return body;
    }

    function setTrialModal(open) {
      const modal = document.getElementById('blueyTrialModal');
      if (!modal) return;
      modal.hidden = !open;
    }

    function renderTrialModal({
      title,
      copy,
      email,
      password,
      expires,
      note,
      canCopy = true,
      showCredentials = true,
      accountActionText = 'Dashboard',
      accountActionHref = '/account',
      downloadActionText = 'Download Bluey',
    }) {
      const titleEl = document.getElementById('trialModalTitle');
      const copyEl = document.getElementById('trialModalCopy');
      const emailEl = document.getElementById('trialEmail');
      const passwordEl = document.getElementById('trialPassword');
      const expiresEl = document.getElementById('trialExpires');
      const noteEl = document.getElementById('trialNote');
      const gridEl = document.querySelector('#blueyTrialModal .trial-grid');
      const turnstileEl = document.getElementById('trialTurnstile');
      const copyButton = document.getElementById('trialCopyLogin');
      const accountAction = document.getElementById('trialAccountAction');
      const downloadAction = document.getElementById('trialDownloadAction');
      if (titleEl) titleEl.textContent = title || 'Bluey trial';
      if (copyEl) copyEl.textContent = copy || '';
      if (emailEl) emailEl.textContent = email || '-';
      if (passwordEl) passwordEl.textContent = password || '-';
      if (expiresEl) expiresEl.textContent = expires ? formatDeviceTime(expires) : '24 hours';
      if (noteEl) noteEl.textContent = note || '';
      if (gridEl) gridEl.hidden = !showCredentials;
      if (turnstileEl) turnstileEl.hidden = true;
      if (copyButton) {
        copyButton.disabled = !canCopy;
        copyButton.hidden = !canCopy;
      }
      if (accountAction) {
        accountAction.textContent = accountActionText;
        accountAction.setAttribute('href', accountActionHref);
      }
      if (downloadAction) downloadAction.textContent = downloadActionText;
    }

    function isPhoneTrialBrowser() {
      const ua = navigator.userAgent || '';
      if (/Android|iPhone|iPod|Mobile/i.test(ua)) return true;
      const coarseNarrow = window.matchMedia
        && window.matchMedia('(pointer: coarse)').matches
        && Math.min(window.innerWidth || 0, window.innerHeight || 0) < 768;
      return Boolean(coarseNarrow);
    }

    function showDesktopTrialRequired() {
      renderTrialModal({
        title: 'Open Try Us on Mac or Windows',
        copy: 'Bluey starts from the desktop overlay. Open this page on the computer where you want to run Bluey, then click Try Us there.',
        email: 'Use a desktop browser',
        password: 'Trial login is created there',
        expires: '',
        note: 'After the trial is created, copy the one-time username and password if you need another browser. The trial includes 15 free minutes.',
        canCopy: false,
      });
      setTrialModal(true);
    }

    function showTrialStatus() {
      if (trialCredentials?.email && trialCredentials?.password) {
        renderTrialModal({
          title: 'Trial is active',
          copy: 'This browser is signed in to your Bluey trial. Copy the login before closing this page, then download Bluey and run bluey on.',
          email: trialCredentials.email,
          password: trialCredentials.password,
          expires: trialCredentials.expires,
          note: 'The password is shown only once. Save it if you may use another browser.',
          canCopy: true,
          showCredentials: true,
        });
      } else {
        renderTrialModal({
          title: 'Already signed in',
          copy: 'This browser already has a Bluey account. Download Bluey and run bluey on, or open Dashboard to manage balance and saved sessions.',
          email: currentAccountEmail || 'Signed in',
          password: 'Not needed',
          expires: '',
          note: 'Temporary trials are for new browsers that have not signed in yet.',
          canCopy: false,
          showCredentials: false,
        });
      }
      setTrialModal(true);
    }

    function trialStartErrorState(error) {
      const message = String(error?.message || '').toLowerCase();
      if (message.includes('404') || message.includes('501') || message.includes('unsupported method')) {
        return {
          title: 'Try Us is being connected',
          copy: 'Temporary trials are not available right now. You can still download Bluey or create a normal account now.',
          note: 'No trial minutes were created and nothing was charged.',
        };
      }
      if (message.includes('captcha') || message.includes('human check') || message.includes('turnstile')) {
        return {
          title: 'Human check needed',
          copy: 'Complete the Try Us human check, then Bluey will create the temporary trial automatically.',
          note: 'This keeps the free trial available for real users and blocks automated loops.',
        };
      }
      if (message.includes('signed in')) {
        return {
          title: 'Already signed in',
          copy: 'This browser already has a Bluey account. Use Dashboard, or sign out before creating a new temporary trial.',
          note: 'Temporary trials are only created for browsers that are not already linked.',
        };
      }
      if (message.includes('trial') || message.includes('velocity') || message.includes('already_used') || message.includes('rate')) {
        return {
          title: 'Trial already used here',
          copy: 'This browser or network has already used its temporary trial. Create an account to keep going.',
          note: 'Trials are limited per browser and network so the free 15 minutes cannot be looped indefinitely.',
        };
      }
      return {
        title: 'Try Us is temporarily unavailable',
        copy: 'Bluey could not create a temporary trial right now. Download Bluey or create an account, then try again in a minute.',
        note: 'No trial minutes were created and nothing was charged.',
      };
    }

    function resetTrialTurnstile() {
      trialTurnstileToken = '';
      if (window.turnstile && trialTurnstileWidgetId !== null) {
        window.turnstile.reset(trialTurnstileWidgetId);
      }
    }

    async function ensureTrialHumanCheck(button) {
      const config = await loadCaptchaConfig();
      if (config.provider !== 'turnstile' || !config.site_key) return true;
      if (trialTurnstileToken) return true;
      renderTrialModal({
        title: 'One quick human check',
        copy: 'Complete the check below. Bluey will create your 15-minute trial as soon as it passes.',
        email: 'Waiting for check',
        password: 'Shown once after the check passes',
        expires: '',
        note: 'This protects the free trial from automated abuse.',
        canCopy: false,
        showCredentials: false,
      });
      const mount = document.getElementById('trialTurnstile');
      if (mount) mount.hidden = false;
      setTrialModal(true);
      pendingTrialButton = button || null;
      await loadTurnstileScript();
      if (!mount || !window.turnstile) throw new Error('Human check could not start.');
      if (trialTurnstileWidgetId !== null) {
        window.turnstile.reset(trialTurnstileWidgetId);
        return false;
      }
      trialTurnstileWidgetId = window.turnstile.render(mount, {
        sitekey: config.site_key,
        theme: storedSiteTheme() === 'light' ? 'light' : 'dark',
        action: 'try_us',
        callback: (token) => {
          trialTurnstileToken = token || '';
          const resumeButton = pendingTrialButton;
          pendingTrialButton = null;
          if (trialTurnstileToken) startTrial(resumeButton);
        },
        'expired-callback': () => {
          trialTurnstileToken = '';
        },
        'error-callback': () => {
          trialTurnstileToken = '';
        },
      });
      return false;
    }

    async function startTrial(button = document.getElementById('tryUsButton')) {
      if (accountToken() || button?.dataset.trialState === 'active') {
        showTrialStatus();
        return;
      }
      if (isPhoneTrialBrowser()) {
        showDesktopTrialRequired();
        return;
      }

      const label = button?.querySelector('span') || button;
      const previous = label?.textContent || 'Try Us';
      if (button) button.disabled = true;
      if (label) label.textContent = 'Creating...';
      try {
        if (!await ensureTrialHumanCheck(button)) return;
        const auth = await apiJson('/auth/trial/start', {
          method: 'POST',
          body: JSON.stringify({
            device_fingerprint: browserTrialDeviceId(),
            turnstile_token: trialTurnstileToken || null,
          }),
        });
        resetTrialTurnstile();
        setAccountToken(auth);
        currentAccountEmail = auth?.account?.email || '';
        trialCredentials = {
          email: currentAccountEmail,
          password: auth?.password || '',
          expires: auth?.temporary_expires_at || auth?.account?.temporary_expires_at || '',
        };
        renderTrialModal({
          title: '15-minute trial ready',
          copy: 'This browser is signed in automatically. Save the password if you may use another browser, then download Bluey and run bluey on.',
          email: trialCredentials.email,
          password: trialCredentials.password || 'Already signed in on this browser',
          expires: trialCredentials.expires,
          note: 'Trial includes 15 free minutes for cloud work and expires after 24 hours unless you save it as an account.',
          canCopy: true,
          showCredentials: true,
        });
        if (button) button.dataset.trialState = 'active';
        setTrialModal(true);
      } catch (error) {
        resetTrialTurnstile();
        const state = trialStartErrorState(error);
        renderTrialModal({
          title: state.title,
          copy: state.copy,
          email: '',
          password: '',
          expires: '',
          note: state.note,
          canCopy: false,
          showCredentials: false,
          accountActionText: 'Create account',
          accountActionHref: '/login',
        });
        setTrialModal(true);
      } finally {
        if (button) button.disabled = false;
        if (label) label.textContent = button?.dataset.trialState === 'active' ? 'Trial Active' : previous;
      }
    }

    async function copyTrialLogin() {
      if (!trialCredentials?.email) return;
      const password = trialCredentials.password || '(already signed in on this browser)';
      const text = [
        'Bluey 15-minute trial',
        `Username: ${trialCredentials.email}`,
        `Password: ${password}`,
        '',
        'Save this now. The password is shown only once.',
        'Run on your laptop or desktop: bluey on',
        'The temporary account expires after 24 hours unless you save it as an account.',
      ].join('\n');
      await navigator.clipboard.writeText(text);
      const note = document.getElementById('trialNote');
      if (note) note.textContent = 'Copied. Keep this password somewhere safe if you may use another browser.';
    }

    function accountMessage(text, auth = false, tone = '') {
      const el = document.getElementById(auth ? 'accountAuthMessage' : 'accountMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function friendlyAuthMessage(message, fallback = 'Could not complete sign in. Try again.') {
      const value = String(message || '').trim();
      if (!value) return fallback;
      if (value === 'email_trial_already_used') {
        return 'This email already used its free trial. You can still create the account, but it will start without free trial minutes.';
      }
      if (value === 'device_trial_already_used') {
        return 'This device already used its free trial. Sign in or add credits to keep using Bluey.';
      }
      if (
        value === 'ip_trial_velocity'
        || value === 'ip_user_agent_trial_velocity'
        || value === 'email_domain_trial_velocity'
      ) {
        return 'Too many signup attempts right now. Please sign in or try again later.';
      }
      return value;
    }

    function friendlyBillingMessage(message, fallback = 'Billing is unavailable for this account right now.') {
      const value = String(message || '').trim();
      if (!value) return fallback;
      if (/temporary|internal|test account|test accounts|paid checkout|payment methods?|auto reload|card saving/i.test(value)) {
        return 'This account cannot add balance yet. Sign into a regular Bluey account to continue.';
      }
      if (/checkout is unavailable|checkout could not start/i.test(value)) {
        return 'Checkout is unavailable right now. Try again in a moment.';
      }
      return value;
    }

    function requireSignupTerms() {
      const terms = document.getElementById('signupTerms');
      if (!terms || terms.checked) return true;
      accountMessage('Accept Terms and Privacy to create an account.', true, 'error');
      terms.focus();
      return false;
    }

    async function loadCaptchaConfig() {
      if (!captchaConfigPromise) {
        captchaConfigPromise = fetch('/auth/captcha/config')
          .then((response) => response.ok ? response.json() : { provider: null, site_key: null })
          .then((config) => {
            captchaConfig = config || { provider: null, site_key: null };
            return captchaConfig;
          })
          .catch(() => {
            captchaConfig = { provider: null, site_key: null };
            return captchaConfig;
          });
      }
      return captchaConfigPromise;
    }

    function loadTurnstileScript() {
      if (window.turnstile) return Promise.resolve();
      if (window.__blueyTurnstileScriptPromise) return window.__blueyTurnstileScriptPromise;
      window.__blueyTurnstileScriptPromise = new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
        script.async = true;
        script.defer = true;
        script.onload = () => resolve();
        script.onerror = () => reject(new Error('Could not load captcha.'));
        document.head.appendChild(script);
      });
      return window.__blueyTurnstileScriptPromise;
    }

    async function refreshSignupCaptcha() {
      const el = document.getElementById('signupCaptcha');
      if (!el) return;
      const config = await loadCaptchaConfig();
      const enabled = accountAuthMode === 'signup' && config.provider === 'turnstile' && config.site_key;
      el.hidden = !enabled;
      if (!enabled) {
        signupTurnstileToken = '';
        return;
      }
      await loadTurnstileScript();
      if (signupTurnstileWidgetId !== null && window.turnstile) {
        window.turnstile.reset(signupTurnstileWidgetId);
        signupTurnstileToken = '';
        return;
      }
      signupTurnstileWidgetId = window.turnstile.render(el, {
        sitekey: config.site_key,
        theme: 'dark',
        callback: (token) => {
          signupTurnstileToken = token || '';
        },
        'expired-callback': () => {
          signupTurnstileToken = '';
        },
        'error-callback': () => {
          signupTurnstileToken = '';
        },
      });
    }

    function signupTurnstilePayload() {
      if (captchaConfig.provider !== 'turnstile' || !captchaConfig.site_key) return null;
      return signupTurnstileToken || null;
    }

    function setAccountAuthMode(mode) {
      accountAuthMode = mode === 'signup' ? 'signup' : 'login';
      const title = document.getElementById('accountAuthTitle');
      const copy = document.getElementById('accountAuthCopy');
      const app = document.getElementById('accountApp');
      const emailLabel = document.getElementById('accountEmailLabel');
      const passwordLabel = document.getElementById('accountPasswordLabel');
      const confirmLabel = document.getElementById('accountPasswordConfirmLabel');
      const rememberLabel = document.getElementById('rememberMeLabel');
      const terms = document.getElementById('signupTermsLabel');
      const primary = document.getElementById('accountPrimaryButton');
      const create = document.getElementById('createAccountButton');
      const switchText = document.getElementById('authSwitchText');
      const differentEmail = document.getElementById('useDifferentEmailButton');
      const password = document.getElementById('accountPassword');
      const passwordConfirm = document.getElementById('accountPasswordConfirm');
      app?.classList.toggle('is-signup', accountAuthMode === 'signup');
      app?.classList.remove('is-verifying');
      if (title) title.textContent = accountAuthMode === 'signup' ? 'Create account' : 'Welcome back';
      if (copy) {
        copy.textContent = accountAuthMode === 'signup'
          ? 'Create an account, verify email, then connect the desktop if asked.'
          : 'Sign in to manage balance, sessions, and linked computers.';
      }
      if (emailLabel) emailLabel.hidden = false;
      if (passwordLabel) passwordLabel.hidden = false;
      if (confirmLabel) confirmLabel.hidden = accountAuthMode !== 'signup';
      if (rememberLabel) rememberLabel.hidden = accountAuthMode !== 'login';
      if (terms) terms.hidden = accountAuthMode !== 'signup';
      if (primary) primary.textContent = accountAuthMode === 'signup' ? 'Send verification code' : 'Sign in';
      if (primary) primary.hidden = false;
      if (create) create.textContent = accountAuthMode === 'signup' ? 'Log in' : 'Sign up';
      if (switchText) switchText.textContent = accountAuthMode === 'signup' ? 'Already have an account?' : "Don't have an account?";
      if (differentEmail) differentEmail.hidden = true;
      if (password) {
        password.autocomplete = accountAuthMode === 'signup' ? 'new-password' : 'current-password';
        password.placeholder = accountAuthMode === 'signup' ? 'Create a password' : 'Password';
      }
      if (passwordConfirm) passwordConfirm.required = accountAuthMode === 'signup';
      if (accountAuthMode !== 'signup') {
        setSignupOtpMode(false);
      }
      refreshSignupCaptcha().catch((error) => accountMessage(error.message, true, 'error'));
      accountMessage('', true);
    }

    function setRailCommand(authed, email = '') {
      const label = document.getElementById('railCommandLabel');
      const copy = document.getElementById('railCommandCopy');
      if (!label || !copy) return;
      if (authed) {
        label.textContent = 'signed in';
        copy.textContent = email
          ? `${email} is ready. Run bluey on to open the overlay.`
          : 'Account ready. Run bluey on to open the overlay.';
      } else {
        label.textContent = 'bluey on';
        copy.textContent = 'Start Bluey from Terminal. Sign in only when needed.';
      }
    }

    function setAccountChrome(authed) {
      const app = document.getElementById('accountApp');
      if (!app) return;
      app.classList.toggle('is-authed', Boolean(authed));
      app.classList.toggle('is-guest', !authed);
      setRailCommand(authed);
      syncAccountNav();

      const title = app.querySelector('.account-hero h1');
      const copy = app.querySelector('.account-hero p');
      if (!title || !copy) return;

      if (authed) {
        title.textContent = 'Bluey account';
        copy.innerHTML = 'Manage balance, sessions, and linked computers.';
      } else {
        title.textContent = 'Try Bluey on desktop';
        copy.innerHTML = 'Install Bluey, then sign in when the desktop asks.';
      }
    }

    function setSignupOtpMode(enabled, email = '') {
      const label = document.getElementById('signupOtpLabel');
      const otp = document.getElementById('signupOtp');
      const confirm = document.getElementById('confirmSignupButton');
      const create = document.getElementById('createAccountButton');
      const app = document.getElementById('accountApp');
      const title = document.getElementById('accountAuthTitle');
      const copy = document.getElementById('accountAuthCopy');
      const emailLabel = document.getElementById('accountEmailLabel');
      const passwordLabel = document.getElementById('accountPasswordLabel');
      const confirmLabel = document.getElementById('accountPasswordConfirmLabel');
      const rememberLabel = document.getElementById('rememberMeLabel');
      const terms = document.getElementById('signupTermsLabel');
      const captcha = document.getElementById('signupCaptcha');
      const primary = document.getElementById('accountPrimaryButton');
      const switchText = document.getElementById('authSwitchText');
      const differentEmail = document.getElementById('useDifferentEmailButton');
      if (!label || !otp || !confirm || !create) return;
      pendingSignupEmail = enabled ? email : '';
      app?.classList.toggle('is-verifying', enabled);
      if (title && enabled) title.textContent = 'Check your email';
      if (copy && enabled) {
        copy.textContent = `Enter the 6-digit code we sent to ${email || 'your email'}.`;
      } else if (copy && !enabled) {
        copy.textContent = accountAuthMode === 'signup'
          ? 'Create an account, verify email, then connect the desktop if asked.'
          : 'Sign in to manage balance, sessions, and linked computers.';
      }
      if (emailLabel) emailLabel.hidden = enabled;
      if (passwordLabel) passwordLabel.hidden = enabled;
      if (confirmLabel) confirmLabel.hidden = enabled || accountAuthMode !== 'signup';
      if (rememberLabel) rememberLabel.hidden = enabled || accountAuthMode !== 'login';
      if (terms) terms.hidden = enabled || accountAuthMode !== 'signup';
      if (captcha) captcha.hidden = enabled || captcha.hidden;
      if (primary) primary.hidden = enabled;
      label.hidden = !enabled;
      confirm.hidden = !enabled;
      create.textContent = enabled ? 'Resend code' : (accountAuthMode === 'signup' ? 'Log in' : 'Sign up');
      if (switchText) switchText.textContent = enabled ? 'No email yet? Check spam or' : (accountAuthMode === 'signup' ? 'Already have an account?' : "Don't have an account?");
      if (differentEmail) differentEmail.hidden = !enabled;
      if (!enabled) otp.value = '';
    }

    function recoveryMessage(text, tone = '') {
      const el = document.getElementById('recoveryMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function renderDeviceLinkHint() {
      const code = pendingDeviceCode();
      const authHint = document.getElementById('deviceLinkHint');
      const dashboardHint = document.getElementById('dashboardDeviceLinkHint');
      const targets = [authHint, dashboardHint].filter(Boolean);
      if (!targets.length) return;
      for (const el of targets) {
        el.hidden = true;
        el.classList.remove('is-code-entry', 'is-awaiting-action');
        el.closest('.device-connect-section')?.classList.remove('is-action-needed');
        el.replaceChildren();
      }

      const renderCodeEntry = (el) => {
        el.hidden = false;
        el.classList.add('is-code-entry');
        el.closest('.device-connect-section')?.classList.add('is-action-needed');
        el.replaceChildren();
        const title = document.createElement('strong');
        title.textContent = 'Connect Bluey desktop';
        const body = document.createElement('span');
        body.className = 'device-code-helper';
        body.textContent = accountToken()
          ? 'Enter the code shown in the Bluey host overlay to sign that desktop into this account.'
          : 'Enter the code shown in the Bluey host overlay, then sign in to connect that desktop.';
        const form = document.createElement('form');
        form.className = 'device-code-form';
        form.noValidate = true;
        const input = document.createElement('input');
        input.name = 'user_code';
        input.type = 'text';
        input.placeholder = 'XXXX-XXXX';
        input.autocomplete = 'one-time-code';
        input.spellcheck = false;
        input.inputMode = 'text';
        const button = document.createElement('button');
        button.type = 'submit';
        button.className = 'account-button secondary compact device-primary-action';
        button.textContent = 'Connect';
        const status = document.createElement('span');
        status.className = 'device-code-status';
        form.addEventListener('submit', (event) => {
          event.preventDefault();
          const nextCode = normalizeDeviceCode(input.value);
          if (!nextCode) {
            status.textContent = 'Enter the code shown in the Bluey host overlay.';
            input.focus();
            return;
          }
          rememberPendingDeviceCode(nextCode);
          status.textContent = '';
          renderDeviceLinkHint();
        });
        form.append(input, button);
        el.append(title, form, body, status);
      };

      if (!code) {
        if (accountToken() && dashboardHint && lastLinkedComputerCount === 0) {
          renderCodeEntry(dashboardHint);
        }
        return;
      }

      const renderInto = (el) => {
        el.hidden = false;
        el.replaceChildren();
        const title = document.createElement('strong');
        const approved = isPendingDeviceApprovalFresh(code);
        title.textContent = approved ? 'Bluey desktop is connected' : 'Finish desktop sign-in';
        const body = document.createElement('span');
        if (approved) {
          body.textContent = 'Return to Terminal or Bluey. This browser tab can stay open.';
          el.append(title, body);
          return;
        }
        el.classList.add('is-awaiting-action');
        el.closest('.device-connect-section')?.classList.add('is-action-needed');
        body.append('Terminal is waiting on code ');
        const codeEl = document.createElement('code');
        codeEl.textContent = code;
        if (accountToken()) {
          body.append(
            codeEl,
            `. This connects only that desktop to ${currentAccountEmail || 'this Bluey account'} so it uses this account's shared balance.`
          );
        } else {
          body.append(
            codeEl,
            '. This code came from your desktop app. Sign in or create an account here, then confirm the desktop link before Bluey can use balance or cloud answers.'
          );
        }
        el.append(title, body);
        if (accountToken()) {
          const actionRow = document.createElement('div');
          actionRow.className = 'device-action-row';
          const cue = document.createElement('span');
          cue.className = 'device-action-cue';
          cue.textContent = 'Next step';
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'account-button secondary compact device-primary-action';
          button.textContent = 'Connect desktop';
          button.setAttribute('aria-label', `Connect Bluey desktop using code ${code}`);
          button.addEventListener('click', () => {
            const previousCount = lastLinkedComputerCount;
            button.disabled = true;
            button.textContent = 'Connecting...';
            sessionStorage.setItem(deviceConfirmStorageKey(code), '1');
            approvePendingDevice()
              .then(async () => {
                renderDeviceLinkHint();
                await waitForLinkedDesktop({ previousCount });
              })
              .catch((error) => {
                button.disabled = false;
                button.textContent = 'Connect desktop';
                accountMessage(`Desktop link failed: ${error.message}`);
              });
            });
          actionRow.append(cue, button);
          el.append(actionRow);
        }
      };
      const visibleTargets = accountToken()
        ? [dashboardHint].filter(Boolean)
        : [authHint].filter(Boolean);
      for (const el of visibleTargets) renderInto(el);
    }

    function focusDesktopConnectCard() {
      setDashboardTab('computers');
      renderDeviceLinkHint();
      const hint = document.getElementById('dashboardDeviceLinkHint');
      if (!hint || hint.hidden) return;
      hint.scrollIntoView({ block: 'center', behavior: 'smooth' });
      const input = hint.querySelector('input[name="user_code"]');
      if (input) input.focus();
    }

    async function approvePendingDevice() {
      const code = pendingDeviceCode();
      if (!code || !accountToken()) return false;
      if (isPendingDeviceApprovalFresh(code)) return true;
      if (pendingDeviceApprovedAt(code) > 0) {
        clearPendingDeviceCode(code);
        renderDeviceLinkHint();
        return false;
      }
      if (sessionStorage.getItem(deviceConfirmStorageKey(code)) !== '1') {
        accountMessage('Press Connect desktop to finish Bluey login.');
        return false;
      }
      accountMessage('Connecting this account to the desktop app...');
      await apiJson('/auth/device/approve', {
        method: 'POST',
        body: JSON.stringify({ user_code: code }),
      });
      markPendingDeviceApproved(code);
      accountMessage('Bluey desktop approved. Waiting for it to appear below...');
      return true;
    }

    async function openDesktopDeepLinkIfNeeded() {
      if (currentPath !== '/link' || pendingDeviceCode() || !accountToken()) return false;
      if (sessionStorage.getItem('bluey_desktop_deep_link_started') === '1') return false;
      accountMessage('Opening Bluey desktop...');
      const link = await apiJson('/auth/link/mint', {
        method: 'POST',
        body: JSON.stringify({}),
      });
      sessionStorage.setItem('bluey_desktop_deep_link_started', '1');
      if (!link?.deep_link_url) {
        accountMessage('Bluey desktop link is unavailable right now. The web dashboard is ready.');
        return false;
      }
      window.location.href = link.deep_link_url;
      accountMessage('Bluey should open now. If it does not, make sure the desktop app is installed, then run bluey on.');
      return true;
    }

    async function accountAuth(mode) {
      const email = document.getElementById('accountEmail').value.trim();
      const password = document.getElementById('accountPassword').value;
      const remember = document.getElementById('rememberMe')?.checked !== false;
      if (!email || !password) {
        accountMessage('Email and password are required.', true, 'error');
        return;
      }
      if (mode === 'signup' && !requireSignupTerms()) return;
      accountMessage(mode === 'signup' ? 'Creating account...' : 'Signing in...', true);
      const auth = await apiJson(`/auth/${mode === 'signup' ? 'signup' : 'login'}`, {
        method: 'POST',
        body: JSON.stringify({ email, password }),
      });
      setAccountToken(auth, remember);
      if (currentPath === '/login' && !pendingDeviceCode()) {
        history.replaceState(history.state, '', '/account');
      }
      accountMessage('', true);
      await loadAccount();
    }

    async function startSignupOtp() {
      const email = document.getElementById('accountEmail').value.trim();
      const password = document.getElementById('accountPassword').value;
      const passwordConfirm = document.getElementById('accountPasswordConfirm')?.value || '';
      if (!email || !password) {
        accountMessage('Email and password are required.', true, 'error');
        return;
      }
      if (password !== passwordConfirm) {
        accountMessage('Passwords do not match.', true, 'error');
        document.getElementById('accountPasswordConfirm')?.focus();
        return;
      }
      if (!requireSignupTerms()) return;
      await loadCaptchaConfig();
      const turnstileToken = signupTurnstilePayload();
      if (captchaConfig.provider === 'turnstile' && captchaConfig.site_key && !turnstileToken) {
        accountMessage('Complete the security check to create an account.', true, 'error');
        refreshSignupCaptcha().catch(() => {});
        return;
      }
      accountMessage('Sending verification code...', true);
      const result = await apiJson('/auth/signup/start', {
        method: 'POST',
        body: JSON.stringify({
          email,
          password,
          turnstile_token: turnstileToken,
          device_fingerprint: browserTrialDeviceId(),
        }),
      });
      setSignupOtpMode(true, result.email || email);
      if (result.trial_seconds === 0 || result.no_trial_reason) {
        accountMessage(
          'Verification code sent. This email already used its free trial, so the account will start without free trial minutes.',
          true
        );
      } else {
        accountMessage('', true);
      }
      document.getElementById('signupOtp')?.focus();
    }

    async function confirmSignupOtp() {
      const email = (pendingSignupEmail || document.getElementById('accountEmail').value).trim();
      const otp = document.getElementById('signupOtp').value.trim();
      if (!email || !otp) {
        accountMessage('Enter the 6-digit verification code.', true, 'error');
        return;
      }
      if (!requireSignupTerms()) return;
      accountMessage('Verifying code...', true);
      const auth = await apiJson('/auth/signup/confirm', {
        method: 'POST',
        body: JSON.stringify({ email, otp, device_fingerprint: browserTrialDeviceId() }),
      });
      setAccountToken(auth, document.getElementById('rememberMe')?.checked !== false);
      setSignupOtpMode(false);
      accountMessage('', true);
      await loadAccount();
    }

    function trialConvertMessage(text, tone = '') {
      const el = document.getElementById('trialConvertMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function resetTrialConvertForm() {
      pendingTrialConvertEmail = '';
      ['trialConvertEmail', 'trialConvertPassword', 'trialConvertPasswordConfirm', 'trialConvertOtp'].forEach((id) => {
        const el = document.getElementById(id);
        if (el) el.value = '';
      });
      const terms = document.getElementById('trialConvertTerms');
      if (terms) terms.checked = false;
      const accountFields = document.getElementById('trialConvertAccountFields');
      const verifyFields = document.getElementById('trialConvertVerifyFields');
      if (accountFields) accountFields.hidden = false;
      if (verifyFields) verifyFields.hidden = true;
      trialConvertMessage('');
    }

    function temporaryExpiryCopy(expiresAt) {
      if (!expiresAt) return 'Temporary trial expires after 24 hours.';
      const text = formatDeviceTime(expiresAt);
      return text === 'unknown time'
        ? 'Temporary trial expires after 24 hours.'
        : `Temporary trial expires ${text}.`;
    }

    function renderTemporaryAccount(me) {
      const isTemporary = Boolean(me?.is_temporary);
      const section = document.getElementById('trialConvertSection');
      const copy = document.getElementById('trialConvertCopy');
      const billing = document.querySelector('#accountApp .billing-section');
      if (section) section.hidden = !isTemporary;
      if (billing) billing.hidden = isTemporary;
      if (!isTemporary) {
        resetTrialConvertForm();
        return;
      }
      if (copy) {
        copy.textContent = `${temporaryExpiryCopy(me.temporary_expires_at)} Create an account before then to keep this browser, desktop link, and saved sessions attached.`;
      }
    }

    async function startTrialConversion() {
      const email = document.getElementById('trialConvertEmail')?.value.trim() || '';
      const password = document.getElementById('trialConvertPassword')?.value || '';
      const confirm = document.getElementById('trialConvertPasswordConfirm')?.value || '';
      const terms = document.getElementById('trialConvertTerms');
      const button = document.getElementById('trialConvertStartButton');
      if (!email || !password) {
        trialConvertMessage('Enter your email and password.', 'error');
        return;
      }
      if (password !== confirm) {
        trialConvertMessage('Passwords do not match.', 'error');
        return;
      }
      if (!terms?.checked) {
        trialConvertMessage('Accept Terms and Privacy to continue.', 'error');
        return;
      }
      const previous = button?.textContent || 'Send verification code';
      if (button) {
        button.disabled = true;
        button.textContent = 'Sending...';
      }
      try {
        const result = await apiJson('/auth/trial/convert/start', {
          method: 'POST',
          body: JSON.stringify({ email, password, terms_accepted: true }),
        });
        pendingTrialConvertEmail = result.email || email;
        const accountFields = document.getElementById('trialConvertAccountFields');
        const verifyFields = document.getElementById('trialConvertVerifyFields');
        if (accountFields) accountFields.hidden = true;
        if (verifyFields) verifyFields.hidden = false;
        trialConvertMessage(`Code sent to ${pendingTrialConvertEmail}.`, 'success');
        document.getElementById('trialConvertOtp')?.focus();
      } catch (error) {
        trialConvertMessage(error.message, 'error');
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
      }
    }

    async function confirmTrialConversion() {
      const otp = document.getElementById('trialConvertOtp')?.value.trim() || '';
      const button = document.getElementById('trialConvertConfirmButton');
      if (!pendingTrialConvertEmail || !otp) {
        trialConvertMessage('Enter the verification code.', 'error');
        return;
      }
      const previous = button?.textContent || 'Verify and save account';
      if (button) {
        button.disabled = true;
        button.textContent = 'Verifying...';
      }
      try {
        const auth = await apiJson('/auth/trial/convert/confirm', {
          method: 'POST',
          body: JSON.stringify({ email: pendingTrialConvertEmail, otp }),
        });
        setAccountToken(auth);
        resetTrialConvertForm();
        await loadAccount();
        accountMessage('Account saved. Your trial, browser session, and desktop links stay attached.');
      } catch (error) {
        trialConvertMessage(error.message, 'error');
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
      }
    }

    async function startPasswordReset(email) {
      recoveryMessage('Sending reset email...');
      await apiJson('/auth/password-reset/start', {
        method: 'POST',
        body: JSON.stringify({ email }),
      });
      recoveryMessage('If the account exists, a reset email was sent. Open it to choose a new password.', 'success');
    }

    async function confirmPasswordReset(token, newPassword) {
      recoveryMessage('Updating password...');
      await apiJson('/auth/password-reset/confirm', {
        method: 'POST',
        body: JSON.stringify({ token, new_password: newPassword }),
      });
      recoveryMessage('Password reset. You can sign in now.', 'success');
    }

    function resetDeleteAccountDialog() {
      const dialog = document.getElementById('deleteAccountDialog');
      const data = document.getElementById('deleteAcceptDataLoss');
      const credits = document.getElementById('deleteAcceptCreditLoss');
      const text = document.getElementById('deleteConfirmText');
      const confirm = document.getElementById('deleteAccountConfirm');
      if (dialog) dialog.hidden = true;
      if (data) data.checked = false;
      if (credits) credits.checked = false;
      if (text) text.value = '';
      if (confirm) confirm.disabled = true;
    }

    function changePasswordMessage(text, tone = '') {
      const el = document.getElementById('changePasswordMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function resetChangePasswordDialog() {
      const dialog = document.getElementById('changePasswordDialog');
      const form = document.getElementById('changePasswordForm');
      if (dialog) dialog.hidden = true;
      if (form) form.reset();
      changePasswordMessage('');
    }

    function closeConfirmActionDialog(result = false) {
      const dialog = document.getElementById('confirmActionDialog');
      if (dialog) dialog.hidden = true;
      if (confirmActionResolve) {
        const resolve = confirmActionResolve;
        confirmActionResolve = null;
        resolve(Boolean(result));
      }
    }

    function confirmAction(options = {}) {
      const dialog = document.getElementById('confirmActionDialog');
      const title = document.getElementById('confirmActionTitle');
      const message = document.getElementById('confirmActionMessage');
      const note = document.getElementById('confirmActionNote');
      const confirm = document.getElementById('confirmActionConfirm');
      const cancel = document.getElementById('confirmActionCancel');
      if (!dialog || !title || !message || !note || !confirm) {
        return Promise.resolve(false);
      }
      if (confirmActionResolve) closeConfirmActionDialog(false);
      title.textContent = options.title || 'Confirm action';
      message.textContent = options.message || 'This action needs confirmation.';
      note.textContent = options.note || 'Saved sessions stay in Session History.';
      note.hidden = !note.textContent;
      confirm.textContent = options.confirmText || 'Confirm';
      confirm.classList.toggle('danger', options.tone !== 'primary');
      confirm.classList.toggle('primary', options.tone === 'primary');
      if (cancel) cancel.textContent = options.cancelText || 'Cancel';
      dialog.hidden = false;
      setTimeout(() => cancel?.focus(), 0);
      return new Promise((resolve) => {
        confirmActionResolve = resolve;
      });
    }

    function confirmAutoReloadOff() {
      return confirmAction({
        title: 'Turn off Auto Reload?',
        message: 'Bluey can run out of balance during a call, interview, or long conversation if Auto Reload is off.',
        note: 'Keep Auto Reload on to top up before the account balance gets too low. You can still add balance manually anytime.',
        confirmText: 'Turn off',
        cancelText: 'Keep on',
      });
    }

    function openChangePasswordDialog() {
      const dialog = document.getElementById('changePasswordDialog');
      const email = document.getElementById('changePasswordEmail');
      const current = document.getElementById('changeCurrentPassword');
      if (!dialog) return;
      resetChangePasswordDialog();
      if (email) email.textContent = currentAccountEmail || 'this Bluey account';
      dialog.hidden = false;
      setTimeout(() => current?.focus(), 0);
    }

    async function changePassword() {
      const current = document.getElementById('changeCurrentPassword');
      const next = document.getElementById('changeNewPassword');
      const confirm = document.getElementById('changeConfirmPassword');
      const submit = document.getElementById('changePasswordSubmit');
      const currentPassword = current?.value || '';
      const newPassword = next?.value || '';
      const confirmPassword = confirm?.value || '';
      if (!currentPassword) {
        changePasswordMessage('Enter your current password.', 'error');
        current?.focus();
        return;
      }
      if (newPassword.length < 8) {
        changePasswordMessage('New password must be at least 8 characters.', 'error');
        next?.focus();
        return;
      }
      if (newPassword !== confirmPassword) {
        changePasswordMessage('New passwords do not match.', 'error');
        confirm?.focus();
        return;
      }
      const previous = submit?.textContent || 'Update';
      if (submit) {
        submit.disabled = true;
        submit.textContent = 'Updating...';
      }
      changePasswordMessage('Updating password...');
      try {
        const auth = await apiJson('/auth/password/change', {
          method: 'POST',
          body: JSON.stringify({
            current_password: currentPassword,
            new_password: newPassword,
          }),
        });
        if (auth?.access_token) setAccountToken(auth);
        changePasswordMessage('Password updated.', 'success');
        setTimeout(resetChangePasswordDialog, 900);
      } finally {
        if (submit) {
          submit.disabled = false;
          submit.textContent = previous;
        }
      }
    }

    function updateDeleteAccountConfirmState() {
      const data = document.getElementById('deleteAcceptDataLoss');
      const credits = document.getElementById('deleteAcceptCreditLoss');
      const text = document.getElementById('deleteConfirmText');
      const confirm = document.getElementById('deleteAccountConfirm');
      if (!confirm) return;
      confirm.disabled = !(data?.checked && credits?.checked && text?.value === 'DELETE');
    }

    function openDeleteAccountDialog() {
      const dialog = document.getElementById('deleteAccountDialog');
      const email = document.getElementById('deleteAccountEmail');
      const text = document.getElementById('deleteConfirmText');
      if (!dialog) return;
      resetDeleteAccountDialog();
      if (email) {
        email.textContent = `This permanently deletes ${currentAccountEmail || 'this Bluey account'}.`;
      }
      dialog.hidden = false;
      setTimeout(() => text?.focus(), 0);
    }

    async function deleteAccount() {
      const data = document.getElementById('deleteAcceptDataLoss');
      const credits = document.getElementById('deleteAcceptCreditLoss');
      const text = document.getElementById('deleteConfirmText');
      if (!(data?.checked && credits?.checked && text?.value === 'DELETE')) {
        accountMessage('Confirm account deletion, balance loss, and type DELETE first.');
        updateDeleteAccountConfirmState();
        return;
      }
      accountMessage('Deleting account...');
      await apiJson('/account/delete', {
        method: 'POST',
        body: JSON.stringify({
          confirm_text: 'DELETE',
          accept_data_loss: true,
          accept_credit_loss: true,
        }),
      });
      resetDeleteAccountDialog();
      clearAccountToken();
      await loadAccount();
    }

    async function confirmEmailVerification(token) {
      recoveryMessage('Verifying email...');
      await apiJson('/auth/verify-email/confirm', {
        method: 'POST',
        body: JSON.stringify({ token }),
      });
      recoveryMessage('Email verified. You can return to Bluey.');
    }

    function renderUsage(usage, me = latestAccountForBilling) {
      const estimate = usageEstimate(usage, me);
      document.getElementById('usageValue').textContent = `${usage.total_cues || 0}`;
      const averageCopy = formatAverageCost(estimate.avgCostCents);
      document.getElementById('usageHint').textContent = estimate.spentCents > 0
        ? `${money(estimate.spentCents)} in ${estimate.periodDays} days${averageCopy ? ` · ${averageCopy}.` : '.'}`
        : `No paid work yet. ${shortMoney(MANUAL_RELOAD_AMOUNT_CENTS)} is a starter balance for light mixed use.`;

      const reloadDaysCopy = formatApproxDays(estimate.reloadDays);
      const balanceDaysCopy = formatApproxDays(estimate.balanceDays);
      document.getElementById('tierValue').textContent = estimate.hasRecentPace && reloadDaysCopy
        ? reloadDaysCopy
        : estimate.defaultUseCopy;
      document.getElementById('projectionHint').textContent = estimate.hasRecentPace
        ? `${shortMoney(MANUAL_RELOAD_AMOUNT_CENTS)} at recent pace. Actual use varies by route and context.`
        : `Approximate light mixed use. ${AUTO_ROUTING_USAGE_HINT}`;

      const reloadEstimateValue = document.getElementById('reloadEstimateValue');
      const reloadEstimateHint = document.getElementById('reloadEstimateHint');
      const autoReloadEstimateValue = document.getElementById('autoReloadEstimateValue');
      const autoReloadEstimateHint = document.getElementById('autoReloadEstimateHint');
      if (reloadEstimateValue) {
        reloadEstimateValue.textContent = estimate.hasRecentPace && reloadDaysCopy
          ? reloadDaysCopy
          : estimate.defaultUseCopy;
      }
      if (reloadEstimateHint) {
        reloadEstimateHint.textContent = estimate.hasRecentPace
          ? `Based on recent spend. ${AUTO_ROUTING_USAGE_HINT}`
          : `Approximate light mixed use. ${AUTO_ROUTING_USAGE_HINT}`;
      }
      if (autoReloadEstimateValue) {
        autoReloadEstimateValue.textContent = estimate.autoReloadOn
          ? shortMoney(estimate.reloadCents)
          : 'Off';
      }
      if (autoReloadEstimateHint) {
        autoReloadEstimateHint.textContent = estimate.autoReloadOn
          ? `${reloadDaysCopy || estimate.reloadUseCopy} per reload, approximate. Adds ${money(estimate.reloadCents)} below ${money(estimate.thresholdCents)}.`
          : 'Optional. Turn it on to add balance automatically when balance is low.';
      }

      const list = document.getElementById('usageList');
      const rows = usage.mix || [];
      list.replaceChildren();
      if (!rows.length) {
        const row = document.createElement('div');
        row.className = 'usage-row';
        ['No paid requests yet', 'Ready', '$0.00'].forEach((label) => {
          const item = document.createElement('span');
          item.textContent = label;
          row.appendChild(item);
        });
        list.appendChild(row);
        return;
      }
      rows.forEach((entry) => {
        const row = document.createElement('div');
        row.className = 'usage-row';
        [
          entry.task_type || 'general',
          `${entry.count || 0} cue${entry.count === 1 ? '' : 's'}`,
          money(entry.cost_cents),
        ].forEach((label) => {
          const item = document.createElement('span');
          item.textContent = label;
          row.appendChild(item);
        });
        list.appendChild(row);
      });
    }

    function squareScriptUrl(environment) {
      return environment === 'production'
        ? 'https://web.squarecdn.com/v1/square.js'
        : 'https://sandbox.web.squarecdn.com/v1/square.js';
    }

    async function loadSquareSdk(environment) {
      if (window.Square?.payments) return;
      const src = squareScriptUrl(environment);
      const existing = document.querySelector(`script[data-square-sdk="${environment}"]`);
      if (existing) {
        await new Promise((resolve, reject) => {
          existing.addEventListener('load', resolve, { once: true });
          existing.addEventListener('error', () => reject(new Error('Could not load card form.')), { once: true });
        });
        return;
      }
      await new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = src;
        script.async = true;
        script.dataset.squareSdk = environment;
        script.onload = resolve;
        script.onerror = () => reject(new Error('Could not load card form.'));
        document.head.append(script);
      });
    }

    async function setupSquareCard(me, options = {}) {
      const setupId = options.setupId || 'squareCardSetup';
      const containerId = options.containerId || 'squareCardContainer';
      const setup = document.getElementById(setupId);
      const container = document.getElementById(containerId);
      if (!setup || !container) return;
      if (!me?.square_application_id || !me?.square_location_id) return;
      const environment = me.square_environment || 'sandbox';
      if (squareCard && squareCardEnvironment === environment && squareCardContainerId === containerId) return;
      if (squareCardSetupPromise) return squareCardSetupPromise;

      squareCardSetupPromise = (async () => {
        if (squareCard && typeof squareCard.destroy === 'function') {
          try {
            await squareCard.destroy();
          } catch {
            // Square cleanup is best-effort when moving the form between panels.
          }
        }
        await loadSquareSdk(environment);
        if (!window.Square?.payments) {
          throw new Error('Card form is unavailable.');
        }
        const payments = window.Square.payments(me.square_application_id, me.square_location_id);
        container.replaceChildren();
        squareCardAttached = false;
        squareCard = await payments.card();
        squareCardEnvironment = environment;
        squareCardSetupId = setupId;
        squareCardContainerId = containerId;
        await squareCard.attach(`#${containerId}`);
        squareCardAttached = true;
      })().finally(() => {
        squareCardSetupPromise = null;
      });
      return squareCardSetupPromise;
    }

    function canUseSquareCardSetup(me) {
      return me?.billing_provider === 'square'
        && Boolean(me.square_application_id)
        && Boolean(me.square_location_id)
        && !me.is_temporary
        && !me.is_admin
        && !me.billing_restricted;
    }

    function paidBillingBlocked(me) {
      return Boolean(me?.is_temporary || me?.is_admin || me?.billing_restricted);
    }

    function renderAutoReload(me) {
      latestAccountForBilling = me || null;
      const card = document.getElementById('autoReloadCard');
      const hint = document.getElementById('autoReloadHint');
      const method = document.getElementById('autoReloadMethod');
      const toggle = document.getElementById('autoReloadToggle');
      const changeCardButton = document.getElementById('changeSquareCardButton');
      const cancelAutoReloadButton = document.getElementById('cancelAutoReloadButton');
      const billingProviderLabel = document.getElementById('billingProviderLabel');
      const thresholdInput = document.getElementById('autoReloadThreshold');
      const amountInput = document.getElementById('autoReloadAmount');
      if (!card || !hint || !method || !toggle) return;

      let amountCents = Number(me?.auto_topup_amount_cents || AUTO_RELOAD_DEFAULT_AMOUNT_CENTS);
      let thresholdCents = Number(me?.auto_topup_threshold_cents || AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS);
      if (!me?.auto_topup_enabled
        && amountCents === LEGACY_AUTO_RELOAD_DEFAULT_AMOUNT_CENTS
        && thresholdCents === LEGACY_AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS) {
        amountCents = AUTO_RELOAD_DEFAULT_AMOUNT_CENTS;
        thresholdCents = AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS;
      }
      const amount = money(amountCents);
      const threshold = money(thresholdCents);
      const canSaveSquareCard = canUseSquareCardSetup(me);
      const hasSavedMethod = Boolean(me?.auto_topup_available);
      const optedOutOfSetup = autoReloadSetupOptedOut(me);

      const setupDefaultOn = !me?.auto_topup_enabled && canSaveSquareCard && !hasSavedMethod && !optedOutOfSetup;
      const toggleChecked = Boolean(me?.auto_topup_enabled) || setupDefaultOn;
      card.classList.toggle('is-on', Boolean(me?.auto_topup_enabled));
      card.classList.toggle('is-setup-default', setupDefaultOn);
      toggle.checked = toggleChecked;
      toggle.disabled = false;
      if (changeCardButton) {
        changeCardButton.hidden = !canSaveSquareCard;
        changeCardButton.disabled = !canSaveSquareCard;
        changeCardButton.textContent = hasSavedMethod ? 'Update card' : 'Save card';
      }
      if (cancelAutoReloadButton) {
        cancelAutoReloadButton.hidden = !me?.auto_topup_enabled;
        cancelAutoReloadButton.disabled = !me?.auto_topup_enabled;
      }
      if (billingProviderLabel) {
        billingProviderLabel.textContent = me?.billing_provider === 'square'
          ? 'Secure checkout'
          : 'Prepaid wallet';
      }
      if (thresholdInput) thresholdInput.value = centsToDollars(thresholdCents);
      if (amountInput) amountInput.value = centsToDollars(amountCents);

      if (me?.auto_topup_enabled) {
        hint.textContent = `On. Adds ${amount} when balance is below ${threshold}.`;
      } else if (hasSavedMethod) {
        hint.textContent = 'Off. Turn on to resume automatic reloads.';
      } else if (canSaveSquareCard) {
        hint.textContent = 'Add balance once and keep Auto Reload on to save a card.';
      } else {
        hint.textContent = friendlyBillingMessage(me?.auto_topup_unavailable_reason);
      }

      method.textContent = me?.saved_payment_method_label
        ? `Saved card: ${me.saved_payment_method_label}`
        : canSaveSquareCard
          ? 'No saved card.'
          : friendlyBillingMessage(me?.auto_topup_unavailable_reason, 'Card saving is unavailable for this account.');
      updateAutoReloadDraftCopy();
      renderBillingStatus(me);
      renderReloadSetupCardState(me);
      if (!document.getElementById('addCreditsDialog')?.hidden) updateReloadSetupDraftCopy();
    }

    function renderBillingStatus(me) {
      const rowsEl = document.getElementById('billingStatusRows');
      const copyEl = document.getElementById('billingStatusCopy');
      if (!rowsEl) return;

      const balanceCents = Number(me?.balance_cents || 0);
      const amountCents = Number(me?.auto_topup_amount_cents || AUTO_RELOAD_DEFAULT_AMOUNT_CENTS);
      const thresholdCents = Number(me?.auto_topup_threshold_cents || AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS);
      const autoReloadOn = Boolean(me?.auto_topup_enabled);
      const savedMethod = String(me?.saved_payment_method_label || '').trim();
      const balanceStatus = balanceCents <= 0
        ? { label: 'Add balance', tone: 'danger' }
        : balanceCents < 500
          ? { label: 'Low', tone: 'warn' }
          : { label: 'Ready', tone: 'good' };
      const rows = [
        {
          item: 'Balance',
          detail: 'Shared account balance',
          amount: money(balanceCents),
          status: balanceStatus.label,
          tone: balanceStatus.tone,
        },
        {
          item: 'Auto Reload',
          detail: autoReloadOn
            ? `Adds balance below ${money(thresholdCents)}`
            : 'No automatic reloads',
          amount: autoReloadOn ? money(amountCents) : '-',
          status: autoReloadOn ? 'On' : 'Off',
          tone: autoReloadOn ? 'good' : 'muted',
        },
        {
          item: 'Card',
          detail: savedMethod || 'No saved card',
          amount: '-',
          status: savedMethod ? 'Saved' : 'None',
          tone: savedMethod ? 'good' : 'muted',
        },
      ];

      rowsEl.replaceChildren(...rows.map((row) => {
        const tr = document.createElement('tr');
        ['item', 'detail', 'amount'].forEach((key) => {
          const td = document.createElement('td');
          td.textContent = row[key];
          if (key === 'item') td.className = 'billing-status-item';
          if (key === 'detail') td.className = 'billing-status-detail';
          if (key === 'amount') td.className = 'billing-status-amount';
          tr.appendChild(td);
        });
        const statusTd = document.createElement('td');
        const badge = document.createElement('span');
        badge.className = `billing-status-badge is-${row.tone}`;
        badge.textContent = row.status;
        statusTd.appendChild(badge);
        tr.appendChild(statusTd);
        return tr;
      }));

      if (copyEl) {
        copyEl.textContent = autoReloadOn
          ? `Auto Reload is active with ${savedMethod || 'a saved card'}.`
          : savedMethod
            ? 'Auto Reload is off. Your saved card stays available if you turn it back on.'
            : 'Add balance once, or keep Auto Reload on to save a card.';
      }
    }

    function readAutoReloadSettings(options = {}) {
      const thresholdInputId = options.thresholdInputId || 'autoReloadThreshold';
      const amountInputId = options.amountInputId || 'autoReloadAmount';
      normalizeBillingMoneyInput(thresholdInputId);
      normalizeBillingMoneyInput(amountInputId);
      const threshold = dollarsToCents(document.getElementById(thresholdInputId)?.value || centsToDollars(AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS));
      const amount = dollarsToCents(document.getElementById(amountInputId)?.value || centsToDollars(AUTO_RELOAD_DEFAULT_AMOUNT_CENTS));
      if (!Number.isFinite(threshold) || !Number.isFinite(amount)) {
        throw new Error('Enter valid Auto Reload dollar amounts.');
      }
      if (amount < AUTO_RELOAD_MIN_CENTS) {
        throw new Error('Auto Reload amount must be at least $15.');
      }
      if (amount > AUTO_RELOAD_MAX_CENTS) {
        throw new Error('Auto Reload amount can be at most $500.');
      }
      if (threshold < 100) {
        throw new Error('Auto Reload threshold must be at least $1.');
      }
      if (threshold > 5000) {
        throw new Error('Auto Reload threshold can be at most $50.');
      }
      if (threshold >= amount) {
        throw new Error('Auto Reload amount must be greater than the threshold.');
      }
      return {
        auto_topup_threshold_cents: threshold,
        auto_topup_amount_cents: amount,
      };
    }

    function updateAutoReloadDraftCopy() {
      const rule = document.getElementById('autoReloadRule');
      const toggle = document.getElementById('autoReloadToggle');
      if (!rule) return;
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      if (!enabled) {
        rule.textContent = wasEnabled ? 'Turning off...' : 'Auto Reload is off.';
        return;
      }
      try {
        const settings = readAutoReloadSettings();
        rule.textContent = hasSavedMethod
          ? `Adds ${money(settings.auto_topup_amount_cents)} when balance is below ${money(settings.auto_topup_threshold_cents)}.`
          : `Add balance once and keep Auto Reload on to add ${money(settings.auto_topup_amount_cents)} below ${money(settings.auto_topup_threshold_cents)}.`;
      } catch (error) {
        rule.textContent = error.message;
      }
    }

    function readManualReloadCents(inputId = 'manualReloadAmount') {
      const input = document.getElementById(inputId);
      normalizeBillingMoneyInput(inputId);
      const amount = dollarsToCents(input?.value || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS));
      if (!Number.isFinite(amount)) {
        input?.focus();
        throw new Error('Enter a reload amount.');
      }
      if (amount < MANUAL_RELOAD_MIN_CENTS) {
        input?.focus();
        throw new Error('Minimum reload is $15.');
      }
      if (amount > MANUAL_RELOAD_MAX_CENTS) {
        input?.focus();
        throw new Error('Maximum reload is $500.');
      }
      return amount;
    }

    function normalizeReloadAmountInput(inputId) {
      normalizeBillingMoneyInput(inputId);
    }

    function setReloadButtonsBusy(busy) {
      [
        document.getElementById('reloadButton'),
        document.getElementById('overviewReloadButton'),
        document.getElementById('reloadSetupCheckoutButton'),
      ].forEach((button) => {
        if (!button) return;
        button.disabled = busy;
        button.classList.toggle('is-loading', busy);
      });
    }

    function openCheckoutPlaceholder(amountCents) {
      let checkoutWindow = null;
      try {
        checkoutWindow = window.open('', '_blank');
      } catch {
        checkoutWindow = null;
      }
      if (!checkoutWindow) return null;
      try {
        checkoutWindow.opener = null;
        checkoutWindow.document.title = 'Opening Bluey checkout';
        checkoutWindow.document.body.style.cssText = 'margin:0;background:#050505;color:#f5f5f5;font:18px system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;display:grid;place-items:center;min-height:100vh;';
        checkoutWindow.document.body.textContent = `Opening checkout for ${money(amountCents)}...`;
      } catch {
        // Some browsers lock down the placeholder tab; navigating it still works.
      }
      return checkoutWindow;
    }

    function closeCheckoutPlaceholder(checkoutWindow) {
      try {
        if (checkoutWindow && !checkoutWindow.closed) checkoutWindow.close();
      } catch {
        // Ignore popup cleanup failures.
      }
    }

    function updateManualReloadDraftCopy() {
      const rule = document.getElementById('manualReloadRule');
      const overviewReloadButton = document.getElementById('overviewReloadButton');
      try {
        const amount = readManualReloadCents();
        if (rule) rule.textContent = '';
        if (overviewReloadButton) overviewReloadButton.textContent = 'Add balance';
      } catch (error) {
        if (rule) rule.textContent = error.message;
        if (overviewReloadButton) overviewReloadButton.textContent = 'Add balance';
      }
    }

    function trialMinutesRemaining(me) {
      return Math.max(0, Math.ceil(Number(me?.trial_seconds_remaining || 0) / 60));
    }

    function accountBalanceHint(me) {
      if (me?.is_temporary) {
        const minutes = trialMinutesRemaining(me);
        return `${minutes} free minute${minutes === 1 ? '' : 's'} left. Create an account within 24 hours to keep it.`;
      }
      if (me?.is_admin || me?.billing_restricted) {
        return 'Internal and test accounts use admin credits. Paid checkout and Auto Reload are disabled here.';
      }
      const balanceCents = Number(me?.balance_cents || 0);
      if (balanceCents <= 0) {
        return 'Add balance to start. Keep Auto Reload on to top up before work stops.';
      }
      if (balanceCents < 100) {
        return 'Almost out. Add balance or keep Auto Reload on.';
      }
      if (balanceCents < 500) {
        return 'Low balance. Add more soon.';
      }
      return 'Ready across this Bluey account.';
    }

    function accountBalanceUsageHint(me, usage) {
      if (me?.is_temporary) return accountBalanceHint(me);
      const estimate = usageEstimate(usage, me);
      const balanceDaysCopy = formatApproxDays(estimate.balanceDays);
      if (estimate.balanceCents <= 0) {
        return `${shortMoney(MANUAL_RELOAD_AMOUNT_CENTS)} starts about ${estimate.defaultUseCopy} of light mixed use.`;
      }
      if (estimate.hasRecentPace && balanceDaysCopy) {
        return `At recent pace, this balance lasts ${balanceDaysCopy}.`;
      }
      return `Approximate: ${estimate.defaultUseCopy} of light mixed use per ${shortMoney(MANUAL_RELOAD_AMOUNT_CENTS)}.`;
    }

    function renderAccountBalanceSnapshot(me, usage = null) {
      currentAccountEmail = me.email || '';
      const accountLabel = me.is_temporary ? 'Temporary Bluey trial' : (me.email || 'Bluey account');
      document.querySelectorAll('[data-profile-email-label]').forEach((profileEmailLabel) => {
        profileEmailLabel.textContent = accountLabel;
      });
      setRailCommand(true, accountLabel);
      const billingProviderLabel = document.getElementById('billingProviderLabel');
      if (billingProviderLabel) {
        billingProviderLabel.textContent = me.is_temporary
          ? 'Temporary trial'
          : me.billing_provider === 'square'
            ? 'Secure checkout'
            : 'Prepaid wallet';
      }
      const balanceValue = document.getElementById('balanceValue');
      const balanceCard = balanceValue?.closest('.balance-kpi');
      const balanceTitle = document.getElementById('balanceTitle');
      const overviewReloadButton = document.getElementById('overviewReloadButton');
      const balanceReloadPanel = document.getElementById('balanceReloadPanel');
      const isTemporaryAccount = Boolean(me.is_temporary);
      const isBillingBlocked = paidBillingBlocked(me);
      const trialMinutes = trialMinutesRemaining(me);
      if (balanceTitle) balanceTitle.textContent = isTemporaryAccount ? 'Trial time' : 'Remaining balance';
      if (balanceValue) {
        balanceValue.textContent = isTemporaryAccount ? `${trialMinutes} min` : money(me.balance_cents);
        balanceCard?.classList.toggle('balance-trial', isTemporaryAccount);
        balanceCard?.classList.toggle('balance-billing-blocked', isBillingBlocked && !isTemporaryAccount);
        balanceCard?.classList.toggle('balance-critical', !isTemporaryAccount && me.balance_cents > 0 && me.balance_cents < 500);
        balanceCard?.classList.toggle('balance-low', !isTemporaryAccount && me.balance_cents >= 500 && me.balance_cents < 1000);
      }
      if (overviewReloadButton) {
        overviewReloadButton.hidden = isBillingBlocked;
        overviewReloadButton.disabled = isBillingBlocked;
      }
      if (balanceReloadPanel) {
        balanceReloadPanel.hidden = isBillingBlocked;
      }
      const summaryBalanceValue = document.getElementById('summaryBalanceValue');
      const summaryBalanceLabel = document.getElementById('summaryBalanceLabel');
      const summaryBalanceCard = summaryBalanceValue?.closest('.summary-balance-kpi');
      if (summaryBalanceLabel) summaryBalanceLabel.textContent = isTemporaryAccount ? 'Trial time' : 'Balance';
      if (summaryBalanceValue) {
        summaryBalanceValue.textContent = isTemporaryAccount ? `${trialMinutes} min` : money(me.balance_cents);
        summaryBalanceCard?.classList.toggle('balance-critical', !isTemporaryAccount && me.balance_cents > 0 && me.balance_cents < 500);
        summaryBalanceCard?.classList.toggle('balance-low', !isTemporaryAccount && me.balance_cents >= 500 && me.balance_cents < 1000);
      }
      const balanceHint = document.getElementById('balanceHint');
      const summaryBalanceHint = document.getElementById('summaryBalanceHint');
      const balanceHintText = usage ? accountBalanceUsageHint(me, usage) : accountBalanceHint(me);
      if (balanceHint) balanceHint.textContent = balanceHintText;
      if (summaryBalanceHint) summaryBalanceHint.textContent = balanceHintText;
      renderTemporaryAccount(me);
      renderAutoReload(me);
      updateManualReloadDraftCopy();
    }

    function stopAccountBalancePolling() {
      if (accountBalancePollTimer) {
        clearInterval(accountBalancePollTimer);
        accountBalancePollTimer = null;
      }
    }

    function startAccountBalancePolling() {
      stopAccountBalancePolling();
      if (!isAccountRoute || !accountToken()) return;
      accountBalancePollTimer = setInterval(async () => {
        if (!accountToken() || document.hidden) return;
        try {
          const me = await apiJson('/account/me');
          renderAccountBalanceSnapshot(me);
        } catch (error) {
          if (!accountToken()) {
            stopAccountBalancePolling();
            loadAccount().catch(() => {});
          }
        }
      }, 10_000);
    }

    async function updateAutoReload(enabled) {
      const settings = enabled ? readAutoReloadSettings() : null;
      const me = await apiJson('/account/billing', {
        method: 'PATCH',
        body: JSON.stringify({
          auto_topup_enabled: enabled,
          ...(settings || {}),
        }),
      });
      renderAutoReload(me);
      accountMessage(enabled
        ? `Auto Reload is on. Bluey adds ${money(settings.auto_topup_amount_cents)} when balance is below ${money(settings.auto_topup_threshold_cents)}.`
        : 'Auto Reload is off. You can add balance manually whenever you need it.');
      return me;
    }

    async function saveDashboardAutoReloadSettings() {
      const toggle = document.getElementById('autoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);

      if (!enabled && !wasEnabled) {
        updateAutoReloadDraftCopy();
        return;
      }
      if (enabled && !latestAccountForBilling?.auto_topup_available) {
        throw new Error('Auto Reload needs a saved card. Use Add balance to set it up.');
      }

      await updateAutoReload(enabled);
      updateAutoReloadDraftCopy();
    }

    async function openSquareCardSetup(options = {}) {
      const button = options.buttonId === null
        ? null
        : document.getElementById(options.buttonId || 'changeSquareCardButton');
      const setupId = options.setupId || 'squareCardSetup';
      const containerId = options.containerId || 'squareCardContainer';
      const setup = document.getElementById(setupId);
      if (!setup) return;
      if (!latestAccountForBilling?.square_application_id || !latestAccountForBilling?.square_location_id) {
        throw new Error('Card changes are not configured for this account yet.');
      }
      const previous = button?.textContent || 'Save card';
      setup.hidden = false;
      setup.dataset.open = '1';
      if (button) {
        button.disabled = true;
        button.textContent = 'Loading...';
      }
      try {
        await setupSquareCard(latestAccountForBilling, { setupId, containerId });
        if (button && options.hideButtonOnAttach !== false) button.hidden = true;
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
      }
    }

    function closeSquareCardSetup(options = {}) {
      const setupId = options.setupId || 'squareCardSetup';
      const containerId = options.containerId || 'squareCardContainer';
      const changeButtonId = options.changeButtonId || 'changeSquareCardButton';
      const setup = document.getElementById(setupId);
      const changeCardButton = document.getElementById(changeButtonId);
      const container = document.getElementById(containerId);
      if (setup) {
        setup.hidden = true;
        setup.dataset.open = '';
      }
      if (container) container.replaceChildren();
      if (changeCardButton) changeCardButton.hidden = false;
      if (!squareCardContainerId || squareCardContainerId === containerId) {
        if (squareCard && typeof squareCard.destroy === 'function') {
          try {
            const destroyed = squareCard.destroy();
            if (destroyed && typeof destroyed.catch === 'function') destroyed.catch(() => {});
          } catch {
            // Ignore cleanup failures while closing a hidden card form.
          }
        }
        squareCard = null;
        squareCardEnvironment = '';
        squareCardSetupId = '';
        squareCardContainerId = '';
        squareCardAttached = false;
      }
    }

    async function tokenizeSquareCardForm() {
      if (!squareCard || !squareCardAttached) {
        throw new Error('Card form is still loading. Try again in a moment.');
      }
      let result;
      try {
        result = await squareCard.tokenize();
      } catch (error) {
        const message = String(error?.message || '');
        if (/not been attached|must be attached|attach/i.test(message)) {
          throw new Error('Card form is still loading. Try again in a moment.');
        }
        throw error;
      }
      if (result.status !== 'OK') {
        const details = (result.errors || []).map((error) => error.message).filter(Boolean).join(' ');
        throw new Error(details || 'Card could not be used.');
      }
      return result.token;
    }

    async function saveSquareCard(options = {}) {
      const button = document.getElementById(options.buttonId || 'saveSquareCardButton');
      const previous = button?.textContent || 'Save card';
      if (button) {
        button.disabled = true;
        button.textContent = 'Saving...';
      }
      try {
        const token = await tokenizeSquareCardForm();
        let me = await apiJson('/billing/square/card', {
          method: 'POST',
          body: JSON.stringify({ source_id: token }),
        });
        closeSquareCardSetup({
          setupId: options.setupId || squareCardSetupId || 'squareCardSetup',
          containerId: options.containerId || squareCardContainerId || 'squareCardContainer',
          changeButtonId: options.changeButtonId || 'changeSquareCardButton',
        });
        renderAutoReload(me);
        if (typeof options.onSaved === 'function') options.onSaved(me);
        accountMessage('Card saved. Auto Reload can use it when turned on.');
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
      }
    }

    function updateCardMessage(text, tone = '') {
      const el = document.getElementById('updateCardMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function resetUpdateCardDialog() {
      const dialog = document.getElementById('updateCardDialog');
      if (dialog) dialog.hidden = true;
      updateCardMessage('');
      closeSquareCardSetup({
        setupId: 'updateCardSquareCardSetup',
        containerId: 'updateCardSquareCardContainer',
        changeButtonId: 'changeSquareCardButton',
      });
    }

    async function openUpdateCardDialog() {
      if (!canUseSquareCardSetup(latestAccountForBilling)) {
        accountMessage(
          friendlyBillingMessage(latestAccountForBilling?.auto_topup_unavailable_reason, 'Card updates are unavailable for this account.'),
          false,
          'error'
        );
        return;
      }
      const dialog = document.getElementById('updateCardDialog');
      const setup = document.getElementById('updateCardSquareCardSetup');
      if (!dialog || !setup) return;

      dialog.hidden = false;
      setup.hidden = false;
      setup.dataset.open = '1';
      updateCardMessage('Loading secure card form...');
      await openSquareCardSetup({
        buttonId: null,
        setupId: 'updateCardSquareCardSetup',
        containerId: 'updateCardSquareCardContainer',
        hideButtonOnAttach: false,
      });
      updateCardMessage('');
      setTimeout(() => document.getElementById('updateCardSaveButton')?.focus(), 0);
    }

    async function saveUpdateCard() {
      await saveSquareCard({
        buttonId: 'updateCardSaveButton',
        setupId: 'updateCardSquareCardSetup',
        containerId: 'updateCardSquareCardContainer',
        changeButtonId: 'changeSquareCardButton',
        onSaved: () => {
          const dialog = document.getElementById('updateCardDialog');
          if (dialog) dialog.hidden = true;
          updateCardMessage('');
        },
      });
    }

    function reloadSetupMessage(text, tone = '') {
      const el = document.getElementById('reloadSetupMessage');
      if (!el) return;
      el.textContent = text || '';
      el.dataset.tone = tone || '';
    }

    function readModalAutoReloadSettings() {
      return readAutoReloadSettings({
        thresholdInputId: 'modalAutoReloadThreshold',
        amountInputId: 'modalAutoReloadAmount',
      });
    }

    function syncReloadSetupFromDashboard() {
      const modalAmount = document.getElementById('modalReloadAmount');
      const manualAmount = document.getElementById('manualReloadAmount');
      const modalToggle = document.getElementById('modalAutoReloadToggle');
      const dashboardToggle = document.getElementById('autoReloadToggle');
      const modalThreshold = document.getElementById('modalAutoReloadThreshold');
      const dashboardThreshold = document.getElementById('autoReloadThreshold');
      const modalAutoAmount = document.getElementById('modalAutoReloadAmount');
      const dashboardAutoAmount = document.getElementById('autoReloadAmount');

      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const canSaveSquareCard = canUseSquareCardSetup(latestAccountForBilling);
      const defaultAutoReloadOn = wasEnabled
        || (canSaveSquareCard && !hasSavedMethod && !autoReloadSetupOptedOut(latestAccountForBilling));

      if (modalAmount) modalAmount.value = manualAmount?.value || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS);
      if (modalToggle) modalToggle.checked = defaultAutoReloadOn || Boolean(dashboardToggle?.checked);
      if (modalThreshold) modalThreshold.value = dashboardThreshold?.value || centsToDollars(AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS);
      if (modalAutoAmount) modalAutoAmount.value = dashboardAutoAmount?.value || centsToDollars(AUTO_RELOAD_DEFAULT_AMOUNT_CENTS);
      renderReloadSetupCardState();
      updateReloadSetupDraftCopy();
    }

    function applyReloadSetupToDashboard(options = {}) {
      const modalAmount = document.getElementById('modalReloadAmount');
      const manualAmount = document.getElementById('manualReloadAmount');
      const modalToggle = document.getElementById('modalAutoReloadToggle');
      const dashboardToggle = document.getElementById('autoReloadToggle');
      const modalThreshold = document.getElementById('modalAutoReloadThreshold');
      const dashboardThreshold = document.getElementById('autoReloadThreshold');
      const modalAutoAmount = document.getElementById('modalAutoReloadAmount');
      const dashboardAutoAmount = document.getElementById('autoReloadAmount');

      if (manualAmount && modalAmount) manualAmount.value = modalAmount.value;
      if (dashboardToggle && modalToggle && options.copyAutoReloadToggle !== false) dashboardToggle.checked = modalToggle.checked;
      if (dashboardThreshold && modalThreshold) dashboardThreshold.value = modalThreshold.value;
      if (dashboardAutoAmount && modalAutoAmount) dashboardAutoAmount.value = modalAutoAmount.value;
      updateManualReloadDraftCopy();
      updateAutoReloadDraftCopy();
    }

    function renderReloadSetupCardState(me = latestAccountForBilling) {
      const cardButton = document.getElementById('reloadSetupCardButton');
      if (!cardButton) return;
      const canUseCard = canUseSquareCardSetup(me);
      const hasSavedMethod = Boolean(me?.auto_topup_available);
      const setup = document.getElementById('reloadSetupSquareCardSetup');

      cardButton.disabled = !canUseCard;
      cardButton.hidden = setup?.dataset.open === '1';
      cardButton.textContent = hasSavedMethod ? 'Use another card' : 'Add card';
    }

    function updateReloadAmountPresets() {
      const input = document.getElementById('modalReloadAmount');
      const amount = dollarsToCents(input?.value || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS));
      document.querySelectorAll('[data-reload-amount]').forEach((button) => {
        const preset = dollarsToCents(button.dataset.reloadAmount || '');
        button.classList.toggle('is-active', Number.isFinite(amount) && amount === preset);
      });
    }

    function updateReloadSetupDraftCopy() {
      const checkoutButton = document.getElementById('reloadSetupCheckoutButton');
      const autoRule = document.getElementById('modalAutoReloadRule');
      const cardButton = document.getElementById('reloadSetupCardButton');
      const cardActions = document.querySelector('#addCreditsDialog .reload-setup-card-actions');
      const toggle = document.getElementById('modalAutoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      const canUseCard = canUseSquareCardSetup(latestAccountForBilling);
      let amountCents = MANUAL_RELOAD_AMOUNT_CENTS;

      try {
        amountCents = readManualReloadCents('modalReloadAmount');
        if (checkoutButton) {
          checkoutButton.textContent = canUseCard
            ? `Add ${money(amountCents)}`
            : 'Continue to checkout';
          checkoutButton.disabled = false;
        }
      } catch (error) {
        if (autoRule) autoRule.textContent = error.message;
        if (checkoutButton) {
          checkoutButton.textContent = canUseCard ? 'Add balance' : 'Continue to checkout';
          checkoutButton.disabled = true;
        }
        updateReloadAmountPresets();
        return;
      }
      updateReloadAmountPresets();

      if (!autoRule) return;
      if (cardActions) cardActions.hidden = !canUseCard;
      if (cardButton) {
        cardButton.hidden = !canUseCard || document.getElementById('reloadSetupSquareCardSetup')?.dataset.open === '1';
        cardButton.disabled = !canUseCard;
      }
      if (!enabled) {
        autoRule.textContent = wasEnabled
          ? 'Auto Reload turns off after this payment.'
          : hasSavedMethod
            ? 'This payment uses your saved card. Auto Reload stays off.'
            : canUseCard
              ? 'Use a card once. Auto Reload stays off.'
              : 'Auto Reload is off.';
        return;
      }

      try {
        const settings = readModalAutoReloadSettings();
        autoRule.textContent = hasSavedMethod
          ? `Add ${money(settings.auto_topup_amount_cents)} when balance is below ${money(settings.auto_topup_threshold_cents)}.`
          : canUseCard
            ? `Add a card once. Then Bluey adds ${money(settings.auto_topup_amount_cents)} when below ${money(settings.auto_topup_threshold_cents)}.`
            : 'Auto Reload is not available for this account yet.';
      } catch (error) {
        autoRule.textContent = error.message;
      }
    }

    function resetReloadSetupDialog() {
      const dialog = document.getElementById('addCreditsDialog');
      if (dialog) dialog.hidden = true;
      reloadSetupMessage('');
      closeSquareCardSetup({
        setupId: 'reloadSetupSquareCardSetup',
        containerId: 'reloadSetupSquareCardContainer',
        changeButtonId: 'reloadSetupCardButton',
      });
    }

    function openReloadSetupDialog() {
      const dialog = document.getElementById('addCreditsDialog');
      const amount = document.getElementById('modalReloadAmount');
      if (paidBillingBlocked(latestAccountForBilling)) {
        accountMessage('Paid checkout is disabled for this account. Use Trial Ops for internal test balance changes.', false, 'error');
        return;
      }
      if (!dialog) {
        startReload();
        return;
      }
      syncReloadSetupFromDashboard();
      reloadSetupMessage('');
      dialog.hidden = false;
      if (canUseSquareCardSetup(latestAccountForBilling) && !latestAccountForBilling?.auto_topup_available) {
        openReloadSetupCard().catch((error) => reloadSetupMessage(error.message, 'error'));
      }
      setTimeout(() => amount?.focus(), 0);
    }

    async function openReloadSetupCard() {
      await openSquareCardSetup({
        buttonId: 'reloadSetupCardButton',
        setupId: 'reloadSetupSquareCardSetup',
        containerId: 'reloadSetupSquareCardContainer',
      });
      renderReloadSetupCardState();
      updateReloadSetupDraftCopy();
      reloadSetupMessage('Enter your card, then continue.');
    }

    async function saveReloadSetupAutoReloadIfReady() {
      const toggle = document.getElementById('modalAutoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);

      if (enabled) {
        setAutoReloadSetupOptOut(false);
      } else if (!wasEnabled && !hasSavedMethod) {
        setAutoReloadSetupOptOut(true);
      }

      if (enabled && hasSavedMethod) {
        applyReloadSetupToDashboard();
        await updateAutoReload(true);
        return true;
      }
      if (!enabled && wasEnabled) {
        applyReloadSetupToDashboard();
        await updateAutoReload(false);
        return true;
      }
      applyReloadSetupToDashboard({ copyAutoReloadToggle: false });
      return false;
    }

    async function startReloadFromSetup() {
      let amountCents;
      try {
        amountCents = readManualReloadCents('modalReloadAmount');
      } catch (error) {
        reloadSetupMessage(error.message, 'error');
        document.getElementById('modalReloadAmount')?.focus();
        updateReloadSetupDraftCopy();
        return false;
      }

      const autoReloadSelected = Boolean(document.getElementById('modalAutoReloadToggle')?.checked);
      setAutoReloadSetupOptOut(!autoReloadSelected
        && !latestAccountForBilling?.auto_topup_enabled
        && !latestAccountForBilling?.auto_topup_available);
      let autoReloadSettings = null;
      if (autoReloadSelected) {
        try {
          autoReloadSettings = readModalAutoReloadSettings();
        } catch (error) {
          reloadSetupMessage(error.message, 'error');
          updateReloadSetupDraftCopy();
          return false;
        }
      }

      const canUseCard = canUseSquareCardSetup(latestAccountForBilling);
      if (!canUseCard) {
        reloadSetupMessage('Opening checkout...');
        await saveReloadSetupAutoReloadIfReady();
        const opened = await startReload();
        reloadSetupMessage(opened
          ? 'Checkout opened in a new tab. Complete payment there, then return to Bluey.'
          : 'Checkout did not open. Check the message above and try again.',
        opened ? 'success' : 'error');
        return opened;
      }

      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      const cardSetup = document.getElementById('reloadSetupSquareCardSetup');
      const usingCardForm = Boolean(cardSetup && cardSetup.dataset.open === '1' && !cardSetup.hidden);
      let sourceId = '';

      if (!hasSavedMethod || usingCardForm) {
        if (!usingCardForm) {
          await openReloadSetupCard();
        }
        try {
          sourceId = await tokenizeSquareCardForm();
        } catch (error) {
          reloadSetupMessage(error.message, 'error');
          updateReloadSetupDraftCopy();
          return false;
        }
      }

      const request = {
        amount_cents: amountCents,
        source_id: sourceId || null,
        use_saved_card: !sourceId && hasSavedMethod,
        save_card_for_auto_reload: Boolean(sourceId && autoReloadSelected),
        auto_topup_enabled: autoReloadSelected,
        client_request_id: paymentRequestId(),
        ...(autoReloadSettings || {}),
      };

      setReloadButtonsBusy(true);
      reloadSetupMessage(autoReloadSelected ? 'Adding balance and setting Auto Reload...' : 'Adding balance...');
      try {
        const result = await apiJson('/billing/square/pay', {
          method: 'POST',
          body: JSON.stringify(request),
        });
        if (result?.account) {
          latestAccountForBilling = result.account;
          renderAutoReload(result.account);
        }
        applyReloadSetupToDashboard();
        resetReloadSetupDialog();
        await loadAccount();
        const message = result?.credited
          ? (autoReloadSelected ? 'Balance added. Auto Reload is ready.' : 'Balance added.')
          : 'Payment is processing. Your balance will update shortly.';
        accountMessage(message, false, result?.credited ? 'success' : '');
        return true;
      } catch (error) {
        reloadSetupMessage(friendlyBillingMessage(error.message, 'Could not add balance. Try again.'), 'error');
        return false;
      } finally {
        setReloadButtonsBusy(false);
      }
    }

    function shortSessionId(id) {
      const value = String(id || '');
      return value.length > 8 ? value.slice(0, 8) : value || 'session';
    }

    function formatSessionTime(ms) {
      const value = Number(ms || 0);
      if (!value) return 'not saved yet';
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return 'unknown time';
      return date.toLocaleString([], {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
      });
    }

    function formatDeviceTime(value) {
      if (!value) return 'never used';
      let text = String(value).trim();
      if (!text) return 'never used';
      if (!/[zZ]|[+-]\d{2}:?\d{2}$/.test(text)) {
        text = text.includes('T') ? `${text}Z` : `${text.replace(' ', 'T')}Z`;
      }
      const date = new Date(text);
      if (Number.isNaN(date.getTime())) return 'unknown time';
      return date.toLocaleString([], {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
      });
    }

    function devicePlatform(device) {
      const value = `${device?.platform || ''} ${device?.kind || ''} ${device?.label || ''}`.toLowerCase();
      if (value.includes('macos') || value.includes('darwin') || value.includes('macbook') || value.includes('mac ')) return 'macos';
      if (value.includes('windows') || value.includes('win32') || value.includes('dell')) return 'windows';
      if (value.includes('linux')) return 'linux';
      return 'desktop';
    }

    function isBrowserDevice(device) {
      const platform = String(device?.platform || '').trim().toLowerCase();
      const kind = String(device?.kind || '').trim().toLowerCase();
      const label = String(device?.label || '').trim().toLowerCase();
      const deviceId = String(device?.device_id || '').trim().toLowerCase();
      // Browser-login rows are intentionally hidden until the account-session view is revisited.
      return platform === 'web'
        || platform === 'browser'
        || kind === 'web'
        || kind === 'browser'
        || label.includes('browser session')
        || label.includes('web session')
        || deviceId.startsWith('web-')
        || deviceId.startsWith('browser-');
    }

    function applySvgAttrs(node, attrs) {
      const nextAttrs = { ...attrs };
      if (!Object.prototype.hasOwnProperty.call(nextAttrs, 'fill')
        && !Object.prototype.hasOwnProperty.call(nextAttrs, 'stroke')) {
        nextAttrs.fill = 'currentColor';
      }
      Object.entries(nextAttrs).forEach(([key, value]) => node.setAttribute(key, value));
    }

    function appendSvgPath(svg, d, attrs = {}) {
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('d', d);
      applySvgAttrs(path, attrs);
      svg.append(path);
    }

    function appendSvgRect(svg, attrs) {
      const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
      applySvgAttrs(rect, attrs);
      svg.append(rect);
    }

    function createDeviceIcon(device) {
      const platform = devicePlatform(device);
      const icon = document.createElement('div');
      icon.className = 'device-icon';
      icon.dataset.platform = platform;
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('viewBox', '0 0 24 24');
      svg.setAttribute('aria-hidden', 'true');
      svg.setAttribute('focusable', 'false');
      if (platform === 'windows') {
        appendSvgRect(svg, { x: '3.5', y: '4.5', width: '7.2', height: '6.6', rx: '.6' });
        appendSvgRect(svg, { x: '12.2', y: '4.5', width: '8.3', height: '6.6', rx: '.6' });
        appendSvgRect(svg, { x: '3.5', y: '12.7', width: '7.2', height: '6.8', rx: '.6' });
        appendSvgRect(svg, { x: '12.2', y: '12.7', width: '8.3', height: '6.8', rx: '.6' });
      } else if (platform === 'macos') {
        appendSvgPath(svg, 'M16.7 12.3c0-2.3 1.9-3.4 2-3.5-1.1-1.6-2.7-1.8-3.3-1.9-1.4-.1-2.7.8-3.4.8-.7 0-1.8-.8-3-.8-1.5 0-2.9.9-3.7 2.3-1.6 2.8-.4 6.9 1.1 9.1.8 1.1 1.7 2.3 2.9 2.3 1.2 0 1.6-.7 3-.7s1.8.7 3 .7c1.3 0 2.1-1.1 2.8-2.2.9-1.3 1.2-2.5 1.2-2.6 0 0-2.5-1-2.6-3.5z');
        appendSvgPath(svg, 'M14.5 5.5c.6-.7 1-1.7.9-2.7-.9.1-1.9.6-2.5 1.3-.6.7-1 1.6-.9 2.6.9.1 1.9-.5 2.5-1.2z');
      } else if (platform === 'linux') {
        const strokeAttrs = { fill: 'none', stroke: 'currentColor', 'stroke-width': '1.9', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' };
        appendSvgPath(svg, 'M4.5 6.5A2.5 2.5 0 0 1 7 4h10a2.5 2.5 0 0 1 2.5 2.5v11A2.5 2.5 0 0 1 17 20H7a2.5 2.5 0 0 1-2.5-2.5v-11z', strokeAttrs);
        appendSvgPath(svg, 'M8.2 9.1l3.2 2.9-3.2 2.9', strokeAttrs);
        appendSvgPath(svg, 'M12.4 15h4', strokeAttrs);
      } else {
        const strokeAttrs = { fill: 'none', stroke: 'currentColor', 'stroke-width': '1.9', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' };
        appendSvgRect(svg, { x: '4.5', y: '5.5', width: '15', height: '10', rx: '1.8', ...strokeAttrs });
        appendSvgPath(svg, 'M9 19h6', strokeAttrs);
        appendSvgPath(svg, 'M12 15.5V19', strokeAttrs);
      }
      icon.append(svg);
      return icon;
    }

    function deviceStatus(device) {
      if (device?.live) return 'Active';
      const lastValue = device?.last_heartbeat_at || device?.last_used_at || device?.created_at || '';
      const lastTime = lastValue ? new Date(String(lastValue).replace(' ', 'T')).getTime() : 0;
      return lastTime ? 'Linked' : 'Offline';
    }

    function linkedComputersFromPayload(payload) {
      const devices = Array.isArray(payload?.devices) ? payload.devices : [];
      return devices.filter((device) => !isBrowserDevice(device));
    }

    function renderLinkedDevicesConnecting(message = 'Waiting for Bluey desktop to appear...') {
      const list = document.getElementById('linkedDevicesList');
      if (!list) return;
      list.replaceChildren();
      const countLabel = document.getElementById('linkedDeviceCount');
      const removeAllButton = document.getElementById('removeAllDevicesButton');
      if (countLabel) countLabel.textContent = 'Connecting desktop';
      if (removeAllButton) removeAllButton.disabled = true;
      const empty = document.createElement('div');
      empty.className = 'device-empty device-empty-loading';
      empty.textContent = message;
      list.append(empty);
    }

    function renderLinkedDevices(payload, message = '') {
      const list = document.getElementById('linkedDevicesList');
      if (!list) return;
      list.replaceChildren();
      const computers = linkedComputersFromPayload(payload);
      lastLinkedComputerCount = computers.length;
      const pending = storedPendingDeviceRecord();
      if (computers.length > 0 && pending?.code) {
        const approved = isPendingDeviceApprovalFresh(pending.code);
        const confirmed = sessionStorage.getItem(deviceConfirmStorageKey(pending.code)) === '1';
        const staleUnconfirmed = !confirmed && Date.now() - pending.savedAt > 30 * 1000;
        if (approved || staleUnconfirmed) {
          clearPendingDeviceCode(pending.code);
          setTimeout(renderDeviceLinkHint, 0);
        }
      } else {
        setTimeout(renderDeviceLinkHint, 0);
      }
      const countLabel = document.getElementById('linkedDeviceCount');
      const removeAllButton = document.getElementById('removeAllDevicesButton');
      if (countLabel) {
        countLabel.textContent = computers.length === 0
          ? 'No Bluey desktops'
          : computers.length === 1
          ? '1 Bluey desktop'
          : `${computers.length} Bluey desktops`;
      }
      if (removeAllButton) {
        removeAllButton.disabled = computers.length === 0;
      }
      if (!computers.length) {
        const empty = document.createElement('div');
        empty.className = 'device-empty';
        empty.textContent = message || 'No Bluey desktop connected yet. Open the host overlay and enter its code above.';
        list.append(empty);
        return;
      }

      for (const device of computers) {
        const row = document.createElement('article');
        row.className = 'device-row';

        const body = document.createElement('div');
        body.className = 'device-main';
        const icon = createDeviceIcon(device);
        const copy = document.createElement('div');
        const title = document.createElement('strong');
        title.className = 'device-title';
        const name = document.createElement('span');
        name.textContent = device.label || 'Linked device';
        const kind = document.createElement('span');
        kind.className = 'device-kind';
        kind.textContent = deviceStatus(device);
        title.append(name, kind);

        const meta = document.createElement('span');
        meta.className = 'device-meta';
        const lastSeenAt = device.last_heartbeat_at || device.last_used_at;
        const lastSeen = lastSeenAt
          ? `Last seen ${formatDeviceTime(lastSeenAt)}`
          : `Linked ${formatDeviceTime(device.created_at)}`;
        meta.textContent = lastSeen;
        copy.append(title, meta);
        body.append(icon, copy);

        const button = document.createElement('button');
        button.className = 'account-button danger compact';
        button.type = 'button';
        button.dataset.deviceId = device.id || '';
        button.dataset.deviceLabel = device.label || 'Linked device';
        button.textContent = 'Remove';
        row.append(body, button);
        list.append(row);
      }
    }

    async function loadLinkedDevices() {
      const devices = await apiJson('/account/devices');
      renderLinkedDevices(devices);
      return devices;
    }

    async function waitForLinkedDesktop(options = {}) {
      const previousCount = Number.isFinite(options.previousCount)
        ? options.previousCount
        : lastLinkedComputerCount;
      const handoffCode = pendingDeviceCode();
      renderLinkedDevicesConnecting('Connecting Bluey desktop...');
      let latest = null;
      for (let attempt = 0; attempt < 8; attempt += 1) {
        if (attempt > 0) await wait(700);
        latest = await apiJson('/account/devices');
        const computers = linkedComputersFromPayload(latest);
        const hasConnectedDesktop = computers.length > previousCount
          || (previousCount === 0 && computers.length > 0)
          || (Boolean(handoffCode) && computers.length > 0);
        if (hasConnectedDesktop) {
          renderLinkedDevices(latest);
          clearPendingDeviceCode(handoffCode);
          renderDeviceLinkHint();
          accountMessage('Bluey desktop connected.', false, 'success');
          return latest;
        }
        renderLinkedDevicesConnecting(attempt < 3
          ? 'Waiting for Bluey desktop to check in...'
          : 'Still waiting for Bluey desktop. Keep the host overlay open.');
      }
      renderLinkedDevices(latest || { devices: [] }, 'Bluey approved the connection. Waiting for the desktop to check in; press Refresh if it does not appear in a moment.');
      accountMessage('Bluey approved the desktop. It should appear here in a moment.', false, 'success');
      return latest;
    }

    async function revokeLinkedDevice(deviceId, label) {
      if (!deviceId) return;
      const devices = await apiJson(`/account/devices/${encodeURIComponent(deviceId)}`, {
        method: 'DELETE',
      });
      renderLinkedDevices(devices);
      accountMessage(`${label || 'Computer'} removed. Bluey will sign out on that desktop.`);
      return devices;
    }

    async function revokeAllLinkedDevices() {
      const devices = await apiJson('/account/devices', {
        method: 'DELETE',
      });
      renderLinkedDevices(devices);
      accountMessage('Linked computers removed. Bluey will sign out on those desktops.');
      return devices;
    }

    function renderCloudSessions(payload) {
      const list = document.getElementById('cloudSessionsList');
      if (!list) return;
      list.replaceChildren();
      const allSessions = Array.isArray(payload?.sessions) ? payload.sessions : [];
      if (!allSessions.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No saved desktop chats yet. Connected desktop conversations appear here after transcript or answer upload.';
        list.append(empty);
        return;
      }

      const sessions = allSessions.filter(sessionHasUploadedContent);
      if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No saved conversation text yet. Bluey found desktop session records, but no transcript or answers have uploaded for them.';
        list.append(empty);
        return;
      }

      for (const session of sessions) {
        const counts = sessionUploadCounts(session);
        const timeLabel = formatSessionTime(session.updated_at_ms || session.last_active_at_ms);
        const row = document.createElement('article');
        row.className = 'session-row';

        const body = document.createElement('div');
        body.className = 'session-row-body';
        const title = document.createElement('strong');
        title.className = 'session-title';
        title.textContent = session.title || `Session ${shortSessionId(session.session_id)}`;
        const meta = document.createElement('span');
        meta.className = 'session-meta';
        meta.textContent = `Conversation uploaded - ${timeLabel}`;
        const chips = document.createElement('div');
        chips.className = 'session-upload-chips';
        for (const chipText of sessionUploadChipLabels(counts)) {
          const chip = document.createElement('span');
          chip.className = 'session-upload-chip';
          chip.textContent = chipText;
          chips.append(chip);
        }
        body.append(title, meta, chips);

        const actions = document.createElement('div');
        actions.className = 'session-actions';
        const copyId = document.createElement('button');
        copyId.className = 'account-button ghost compact';
        copyId.type = 'button';
        copyId.dataset.copy = session.session_id || '';
        copyId.textContent = 'Copy ID';
        actions.append(copyId);

        const link = document.createElement('a');
        link.className = 'account-button ghost compact session-open-link';
        link.href = accountSessionHref(session.session_id);
        link.target = '_blank';
        link.rel = 'noopener';
        link.dataset.sessionId = session.session_id || '';
        link.setAttribute('aria-label', `Open ${session.title || 'uploaded session'} in a new tab`);
        link.title = 'Open uploaded conversation in a new tab';
        link.textContent = 'Open';
        actions.append(link);

        row.append(body, actions);
        list.append(row);
      }
    }

    function countLabel(count, singular, plural = `${singular}s`) {
      const value = Number(count || 0);
      return `${value} ${value === 1 ? singular : plural}`;
    }

    function sessionUploadCounts(session) {
      const transcriptItems = firstArray(session?.transcript_segments, session?.transcript, session?.transcripts);
      const responseItems = firstArray(session?.cue_responses, session?.responses, session?.answers, session?.conversation, session?.messages);
      const contextItems = firstArray(session?.context_artifacts, session?.context, session?.context_items, session?.files);
      return {
        transcript: Number(session?.transcript_count || transcriptItems.length || 0),
        responses: Number(session?.response_count || session?.answer_count || responseItems.length || 0),
        context: Number(session?.context_count || contextItems.length || 0),
      };
    }

    function sessionHasUploadedContent(session) {
      const counts = sessionUploadCounts(session);
      return counts.transcript > 0 || counts.responses > 0 || counts.context > 0;
    }

    function sessionUploadChipLabels(counts) {
      if (!counts.transcript && !counts.responses && !counts.context) {
        return ['No conversation uploaded yet'];
      }
      return [
        counts.responses ? countLabel(counts.responses, 'chat turn') : '',
        counts.transcript ? countLabel(counts.transcript, 'transcript segment') : '',
        counts.context ? countLabel(counts.context, 'context item') : '',
      ].filter(Boolean);
    }

    function renderCloudSessionsLoading() {
      const list = document.getElementById('cloudSessionsList');
      if (!list) return;
      list.replaceChildren();
      const loading = document.createElement('div');
      loading.className = 'session-empty';
      loading.textContent = 'Checking for saved sessions...';
      list.append(loading);
    }

    function setCloudSessionDetail(text) {
      const detail = document.getElementById('cloudSessionDetail');
      if (!detail) return;
      detail.hidden = !text;
      detail.replaceChildren();
      if (!text) return;
      const empty = document.createElement('span');
      empty.textContent = text;
      detail.append(empty);
    }

    function appendBundlePreview(detail, label, text) {
      if (!text) return;
      const pre = document.createElement('pre');
      pre.textContent = `${label}\n${text}`;
      detail.append(pre);
    }

    function diagnosticsText(metadata) {
      const diagnostics = metadata?.diagnostics;
      if (!diagnostics || typeof diagnostics !== 'object') return '';
      const lines = [
        `listen runs: ${diagnostics.listen_runs || 0}`,
        `stt parse errors: ${diagnostics.stt_parse_errors || 0}`,
        `stt provider errors: ${diagnostics.stt_provider_errors || 0}`,
        `audio start errors: ${diagnostics.audio_start_errors || 0}`,
        `audio source errors: ${diagnostics.audio_source_errors || 0}`,
      ];
      if (diagnostics.last_stt_provider) lines.push(`last provider: ${diagnostics.last_stt_provider}`);
      if (diagnostics.last_audio_session_id) lines.push(`audio session: ${diagnostics.last_audio_session_id}`);
      if (diagnostics.last_error_kind) lines.push(`last error: ${diagnostics.last_error_kind}`);
      if (diagnostics.last_error_message) lines.push(String(diagnostics.last_error_message));
      if (diagnostics.last_error_at) lines.push(`last error at: ${formatSessionTime(diagnostics.last_error_at)}`);
      return lines.join('\n');
    }

    function joinSessionMeta(values) {
      return values
        .map((value) => String(value || '').trim())
        .filter(Boolean)
        .join(' - ');
    }

    function appendSessionBundleSection(detail, heading, records, emptyText, renderRecord) {
      const section = document.createElement('section');
      section.className = 'session-detail-section';
      const title = document.createElement('h4');
      title.textContent = heading;
      section.append(title);

      const items = Array.isArray(records) ? records : [];
      if (!items.length) {
        const empty = document.createElement('p');
        empty.className = 'session-detail-empty';
        empty.textContent = emptyText;
        section.append(empty);
        detail.append(section);
        return;
      }

      const list = document.createElement('div');
      list.className = 'session-detail-list';
      for (const item of items) {
        const rendered = renderRecord(item) || {};
        const row = document.createElement('article');
        row.className = 'session-detail-item';
        const rowTitle = document.createElement('strong');
        rowTitle.textContent = rendered.title || heading;
        const meta = document.createElement('span');
        meta.textContent = rendered.meta || '';
        row.append(rowTitle, meta);
        if (rendered.body) {
          const pre = document.createElement('pre');
          pre.textContent = rendered.body;
          row.append(pre);
        }
        list.append(row);
      }
      section.append(list);
      detail.append(section);
    }

    function firstArray(...values) {
      for (const value of values) {
        if (Array.isArray(value)) return value;
      }
      return [];
    }

    function normalizeTranscriptSegments(bundle) {
      return firstArray(bundle?.transcript_segments, bundle?.transcript, bundle?.transcripts)
        .map((segment) => {
          if (typeof segment === 'string') return { text: segment };
          return segment && typeof segment === 'object' ? segment : null;
        })
        .filter(Boolean);
    }

    function normalizeCueResponses(bundle) {
      return firstArray(bundle?.cue_responses, bundle?.responses, bundle?.answers, bundle?.conversation, bundle?.messages)
        .map((answer) => {
          if (typeof answer === 'string') return { text: answer };
          return answer && typeof answer === 'object' ? answer : null;
        })
        .filter(Boolean);
    }

    function normalizeContextArtifacts(bundle) {
      return firstArray(bundle?.context_artifacts, bundle?.context, bundle?.context_items, bundle?.files)
        .map((artifact) => {
          if (typeof artifact === 'string') return { text_preview: artifact };
          return artifact && typeof artifact === 'object' ? artifact : null;
        })
        .filter(Boolean);
    }

    async function loadCloudSessions() {
      const sessions = await apiJson('/sync/sessions?limit=8');
      renderCloudSessions(sessions);
      return sessions;
    }

    async function loadCloudSessionDetail(sessionId) {
      if (!sessionId) return;
      setCloudSessionDetail('Loading saved session...');
      const bundle = await apiJson(`/sync/sessions/${encodeURIComponent(sessionId)}`);
      const detail = document.getElementById('cloudSessionDetail');
      if (!detail) return;
      detail.hidden = false;
      detail.replaceChildren();
      const sessionMeta = bundle.session || bundle;
      const transcriptSegments = normalizeTranscriptSegments(bundle);
      const cueResponses = normalizeCueResponses(bundle);
      const contextArtifacts = normalizeContextArtifacts(bundle);
      const hasUploadedContent = transcriptSegments.length > 0 || cueResponses.length > 0 || contextArtifacts.length > 0;

      const title = document.createElement('strong');
      title.textContent = sessionMeta?.title || `Session ${shortSessionId(sessionId)}`;
      const meta = document.createElement('span');
      const sessionCode = shortSessionId(sessionId).toUpperCase();
      meta.textContent = [
        `ID ${sessionCode}`,
        countLabel(transcriptSegments.length, 'transcript segment'),
        countLabel(cueResponses.length, 'chat turn'),
        countLabel(contextArtifacts.length, 'context item'),
      ].join(' - ');
      const uploadState = document.createElement('p');
      uploadState.className = `session-upload-state${hasUploadedContent ? '' : ' is-empty'}`;
      uploadState.textContent = hasUploadedContent
        ? 'Uploaded from Bluey desktop. This is view-only on web for now.'
        : 'Bluey has the session record, but no conversation text has uploaded yet. Keep the desktop open, ask or answer once, then refresh.';
      detail.append(title, meta, uploadState);

      const actions = document.createElement('div');
      actions.className = 'session-actions';
      const copyId = document.createElement('button');
      copyId.className = 'account-button ghost compact';
      copyId.type = 'button';
      copyId.dataset.copy = sessionId;
      copyId.textContent = 'Copy full session ID';
      const deleteSession = document.createElement('button');
      deleteSession.className = 'account-button danger compact';
      deleteSession.type = 'button';
      deleteSession.textContent = 'Delete session';
      deleteSession.addEventListener('click', () => {
        deleteCloudSession(sessionId).catch((error) => {
          accountMessage(error.message || 'Could not delete saved session.');
        });
      });
      actions.append(copyId, deleteSession);
      detail.append(actions);

      if (sessionMeta?.answer_style) {
        appendBundlePreview(detail, 'Answer style', sessionMeta.answer_style);
      }
      appendBundlePreview(detail, 'Diagnostics', diagnosticsText(sessionMeta?.metadata));
      appendSessionBundleSection(
        detail,
        'Conversation',
        cueResponses,
        'No Bluey desktop conversation uploaded for this session yet.',
        (answer) => {
          const answerText = answer.text || '';
          const artifactText = answer.artifact_body && answer.artifact_body !== answerText
            ? answer.artifact_body
            : '';
          return {
            title: answer.source_text ? 'You asked' : 'Bluey answer',
            meta: joinSessionMeta([
              formatSessionTime(answer.ts_ms),
              answer.model || answer.provider,
              answer.cost_label || (answer.cost_cents ? money(answer.cost_cents) : ''),
            ]),
            body: [
              answer.source_text ? `You\n${answer.source_text}` : '',
              answerText ? `Bluey\n${answerText}` : '',
              artifactText ? `Artifact\n${artifactText}` : '',
            ].filter(Boolean).join('\n\n'),
          };
        }
      );
      appendSessionBundleSection(
        detail,
        'Transcript',
        transcriptSegments,
        'No live transcript uploaded for this session yet.',
        (segment) => ({
          title: segment.speaker || segment.source || 'Speaker',
          meta: joinSessionMeta([
            formatSessionTime(segment.ts_ms),
            segment.source,
            segment.is_final === false ? 'draft' : 'final',
          ]),
          body: segment.text || '',
        })
      );
      appendSessionBundleSection(
        detail,
        'Context',
        contextArtifacts,
        'No context files or notes uploaded for this session yet.',
        (artifact) => ({
          title: artifact.title || artifact.kind || artifact.artifact_id || 'Context',
          meta: joinSessionMeta([
            artifact.kind,
            formatSessionTime(artifact.created_at_ms),
            artifact.source_uri,
          ]),
          body: [artifact.note, artifact.text_preview].filter(Boolean).join('\n\n'),
        })
      );
    }

    async function deleteCloudSession(sessionId) {
      if (!sessionId) return;
      const code = shortSessionId(sessionId).toUpperCase();
      if (!window.confirm(`Delete Bluey session ${code} from this account?`)) return;
      await apiJson(`/sync/sessions/${encodeURIComponent(sessionId)}`, { method: 'DELETE' });
      accountMessage(`Deleted session ${code}.`);
      setCloudSessionDetail('Session deleted.');
      const url = new URL(window.location.href);
      url.searchParams.delete('session');
      window.history.replaceState(null, '', url.toString());
      await loadCloudSessions();
    }

    function renderAdminAbuse(payload, message = '') {
      const section = document.getElementById('adminAbuseSection');
      const summary = document.getElementById('adminAbuseSummary');
      const events = document.getElementById('adminAbuseEvents');
      if (!section || !summary || !events) return;
      section.hidden = false;
      summary.replaceChildren();
      events.replaceChildren();
      const stats = [
        ['Trial grants', payload?.trial_grants_24h ?? '--'],
        ['Denied', payload?.trial_denials_24h ?? '--'],
        ['Events', payload?.abuse_events_24h ?? '--'],
      ];
      for (const [label, value] of stats) {
        const item = document.createElement('div');
        item.className = 'abuse-summary-item';
        const strong = document.createElement('strong');
        strong.textContent = String(value);
        const span = document.createElement('span');
        span.textContent = `${label} in 24h`;
        item.append(strong, span);
        summary.append(item);
      }
      if (message) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = message;
        events.append(empty);
        return;
      }
      const recent = Array.isArray(payload?.recent_events) ? payload.recent_events : [];
      if (!recent.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No recent abuse events.';
        events.append(empty);
        return;
      }
      for (const event of recent.slice(0, 8)) {
        const row = document.createElement('article');
        row.className = 'abuse-event-row';
        const title = document.createElement('strong');
        title.textContent = event.reason || event.event_type || 'event';
        const meta = document.createElement('span');
        meta.textContent = [
          event.created_at ? formatDeviceTime(event.created_at) : '',
          event.email_hash ? `email ${event.email_hash.slice(0, 10)}` : '',
          event.ip_hash ? `ip ${event.ip_hash.slice(0, 10)}` : '',
          event.device_hash ? `device ${event.device_hash.slice(0, 10)}` : '',
        ].filter(Boolean).join(' - ');
        row.append(title, meta);
        events.append(row);
      }
    }

    async function loadAdminAbuse() {
      const section = document.getElementById('adminAbuseSection');
      if (!section) return null;
      renderAdminAbuse({}, 'Loading trial abuse events...');
      const summary = await apiJson('/admin/trial-abuse');
      renderAdminAbuse(summary);
      return summary;
    }

    function maybeLoadAdminAbuse() {
      if (!currentAccountIsAdmin || adminAbuseLoaded) return;
      adminAbuseLoaded = true;
      loadAdminAbuse().catch((error) => {
        adminAbuseLoaded = false;
        renderAdminAbuse({}, `Could not load trial protection: ${error.message}`);
      });
    }

    async function loadAccount() {
      if (currentPath === '/verify-email' || currentPath === '/password-reset') {
        renderRecoveryRoute();
        return;
      }
      const authed = Boolean(accountToken());
      setAccountChrome(authed);
      renderDeviceLinkHint();
      document.getElementById('accountAuthCard').hidden = authed;
      document.getElementById('accountPreviewCard').hidden = true;
      document.getElementById('accountDashboard').hidden = !authed;
      document.getElementById('accountRecoveryCard').hidden = true;
      if (!authed) {
        stopAccountBalancePolling();
        currentAccountIsAdmin = false;
        adminAbuseLoaded = false;
        setAdminDashboardAvailability(false);
        return;
      }
      const requestedSessionId = pendingSessionId();
      const requestedHashTab = normalizeDashboardTabName(window.location.hash);
      const shouldOpenRequestedSession = Boolean(requestedSessionId)
        && (!requestedHashTab || requestedHashTab === 'history');
      const refreshButton = document.getElementById('refreshAccountButton');
      if (refreshButton) refreshButton.disabled = true;
      accountMessage('Loading account...');
      renderCloudSessionsLoading();
      const linkedDevicesPromise = loadLinkedDevices().catch((error) => {
        renderLinkedDevices({ devices: [] }, `Could not load computers: ${error.message}`);
        return null;
      });
      const sessionsPromise = loadCloudSessions().then(async (sessions) => {
        if (shouldOpenRequestedSession) {
          setDashboardTab('history');
          try {
            await loadCloudSessionDetail(requestedSessionId);
          } catch (error) {
            setCloudSessionDetail(`Could not load saved session: ${error.message}`);
          }
        } else {
          setCloudSessionDetail('');
        }
        return sessions;
      }).catch((error) => {
        renderCloudSessions({ sessions: [] });
        setCloudSessionDetail(`Could not load saved sessions: ${error.message}`);
        return null;
      });
      try {
        const [me, usage] = await Promise.all([
          apiJson('/account/me'),
          apiJson('/account/usage'),
        ]);
        renderAccountBalanceSnapshot(me, usage);
        renderUsage(usage, me);
        const isAdmin = Boolean(me.is_admin);
        currentAccountIsAdmin = isAdmin;
        if (!isAdmin) adminAbuseLoaded = false;
        setAdminDashboardAvailability(isAdmin);
        const adminSection = document.getElementById('adminAbuseSection');
        if (adminSection) adminSection.hidden = !isAdmin;
        if (isAdmin && normalizeDashboardTabName(window.location.hash) === 'admin') {
          setDashboardTab('admin');
        }
        void sessionsPromise;
        accountMessage(new URLSearchParams(location.search).get('reload') === 'success'
          ? 'Checkout complete. Balance refreshes automatically while Square finishes the credit event.'
          : '');
        startAccountBalancePolling();
        openAccountActionFromHash();
        try {
          const code = pendingDeviceCode();
          const alreadyApproved = Boolean(code && isPendingDeviceApprovalFresh(code));
          const linkedDevicesResult = await linkedDevicesPromise;
          const previousCount = linkedComputersFromPayload(linkedDevicesResult).length;
          const approved = await approvePendingDevice();
          if (approved && (!alreadyApproved || previousCount === 0)) {
            await waitForLinkedDesktop({ previousCount });
          } else if (!approved) {
            await openDesktopDeepLinkIfNeeded();
          }
        } catch {
          accountMessage('Bluey signed in. If the desktop is waiting for this account, use Connect desktop below.');
        }
      } finally {
        if (refreshButton) refreshButton.disabled = false;
      }
    }

    async function startReload() {
      if (paidBillingBlocked(latestAccountForBilling)) {
        accountMessage('Paid checkout is disabled for this account. Use Trial Ops for internal test balance changes.', false, 'error');
        return false;
      }
      let amountCents;
      try {
        amountCents = readManualReloadCents();
      } catch (error) {
        accountMessage(error.message, false, 'error');
        return false;
      }
      const checkoutWindow = openCheckoutPlaceholder(amountCents);
      setReloadButtonsBusy(true);
      accountMessage(`Opening checkout for ${money(amountCents)}...`);
      try {
        const checkout = await apiJson('/billing/checkout', {
          method: 'POST',
          body: JSON.stringify({ amount_cents: amountCents }),
        });
        const url = String(checkout.checkout_url || '').trim();
        if (!url) throw new Error('Checkout did not return a payment link.');
        if (checkoutWindow && !checkoutWindow.closed) {
          checkoutWindow.location.assign(url);
        } else {
          const opened = window.open(url, '_blank', 'noopener,noreferrer');
          if (!opened) {
            accountMessage('Checkout was blocked by the browser. Allow popups for bluey.sh, then click Add balance again.', false, 'error');
            return false;
          }
        }
        accountMessage('Checkout opened in a new tab. Complete payment there, then return here; the balance refreshes automatically.');
        return true;
      } catch (error) {
        closeCheckoutPlaceholder(checkoutWindow);
        accountMessage(friendlyBillingMessage(error?.message, 'Could not open checkout.'), false, 'error');
        return false;
      } finally {
        setReloadButtonsBusy(false);
      }
    }

    function normalizeDashboardTabName(value) {
      const tab = String(value || '').replace(/^#/, '').trim().toLowerCase();
      if (tab === 'computers' || tab === 'my-computers' || tab === 'devices' || tab === 'linked-devices') return 'computers';
      if (tab === 'sessions' || tab === 'session-history' || tab === 'history' || tab === 'saved-sessions') return 'history';
      if (tab === 'summary' || tab === 'usage-summary' || tab === 'usage') return 'summary';
      if (tab === 'billing' || tab === 'reload' || tab === 'credits') return 'billing';
      if (tab === 'admin' || tab === 'trial-ops' || tab === 'trial-protection' || tab === 'trial-abuse') return 'admin';
      return '';
    }

    function setAdminDashboardAvailability(enabled) {
      const button = document.getElementById('dashboardTabAdminButton');
      const panel = document.getElementById('dashboardTabAdmin');
      if (button) {
        button.hidden = !enabled;
        if (!enabled) {
          button.classList.remove('is-active');
          button.setAttribute('aria-selected', 'false');
        }
      }
      if (!enabled && panel) {
        panel.hidden = true;
        panel.classList.remove('is-active');
      }
      if (!enabled && normalizeDashboardTabName(window.location.hash) === 'admin') {
        setDashboardTab('computers', true);
      }
    }

    function openAccountActionFromHash() {
      if (!accountToken()) return false;
      const action = String(window.location.hash || '').replace(/^#/, '').trim().toLowerCase();
      if (action === 'password' || action === 'change-password') {
        openChangePasswordDialog();
        return true;
      }
      if (action === 'delete' || action === 'delete-account') {
        openDeleteAccountDialog();
        return true;
      }
      return false;
    }

    function setDashboardTab(tabName, updateHash = false) {
      const name = normalizeDashboardTabName(tabName) || 'computers';
      document.querySelectorAll('[data-dashboard-tab]').forEach((button) => {
        const active = button.dataset.dashboardTab === name;
        button.classList.toggle('is-active', active);
        button.setAttribute('aria-selected', active ? 'true' : 'false');
      });
      document.querySelectorAll('[data-dashboard-panel]').forEach((panel) => {
        const active = panel.dataset.dashboardPanel === name;
        panel.hidden = !active;
        panel.classList.toggle('is-active', active);
      });
      if (name === 'admin') maybeLoadAdminAbuse();
      if (updateHash && window.history?.replaceState) {
        const url = new URL(window.location.href);
        if (name !== 'history') {
          url.searchParams.delete('session');
        }
        url.hash = name;
        const nextUrl = `${url.pathname}${url.search}${url.hash}`;
        window.history.replaceState(null, '', nextUrl);
      }
    }

    function initDashboardTabs() {
      const hashTab = normalizeDashboardTabName(window.location.hash);
      setDashboardTab(hashTab || (currentPath === '/reload' ? 'billing' : 'computers'));
      document.querySelectorAll('[data-dashboard-tab]').forEach((button) => {
        if (button.dataset.dashboardReady === '1') return;
        button.dataset.dashboardReady = '1';
        button.addEventListener('click', () => {
          setDashboardTab(button.dataset.dashboardTab, true);
        });
      });
      window.addEventListener('hashchange', () => {
        const tab = normalizeDashboardTabName(window.location.hash);
        if (tab) {
          setDashboardTab(tab);
        } else {
          openAccountActionFromHash();
        }
      });
    }

    function initAccountApp() {
      if (isPolicyRoute) {
        productSite.hidden = true;
        accountApp.hidden = true;
        downloadApp.hidden = true;
        policyApp.hidden = false;
        document.getElementById('privacyPolicy').hidden = currentPath !== '/privacy' && currentPath !== '/docs/privacy';
        document.getElementById('termsPolicy').hidden = currentPath !== '/terms' && currentPath !== '/docs/terms';
        document.getElementById('disguisePolicy').hidden = currentPath !== '/docs/disguise';
        return;
      }
      if (isDownloadRoute) {
        productSite.hidden = true;
        accountApp.hidden = true;
        policyApp.hidden = true;
        downloadApp.hidden = false;
        return;
      }
      if (!isAccountRoute) return;
      productSite.hidden = true;
      accountApp.hidden = false;
      downloadApp.hidden = true;
      renderDeviceLinkHint();
      loadCaptchaConfig()
        .then(() => refreshSignupCaptcha())
        .catch(() => {});
      initDashboardTabs();
      installBillingMoneyInputGuards();

      document.getElementById('accountForm').addEventListener('submit', (event) => {
        event.preventDefault();
        if (accountAuthMode === 'signup') {
          startSignupOtp().catch((error) => accountMessage(friendlyAuthMessage(error.message), true, 'error'));
        } else {
          accountAuth('login').catch((error) => accountMessage(friendlyAuthMessage(error.message), true, 'error'));
        }
      });
      document.getElementById('createAccountButton').addEventListener('click', () => {
        if (pendingSignupEmail) {
          startSignupOtp().catch((error) => accountMessage(friendlyAuthMessage(error.message), true, 'error'));
          return;
        }
        if (accountAuthMode === 'signup') {
          setAccountAuthMode('login');
        } else {
          setAccountAuthMode('signup');
          document.getElementById('accountEmail')?.focus();
        }
      });
      document.getElementById('useDifferentEmailButton')?.addEventListener('click', () => {
        setSignupOtpMode(false);
        setAccountAuthMode('signup');
        document.getElementById('accountEmail')?.focus();
      });
      document.querySelectorAll('[data-password-toggle]').forEach((button) => {
        if (button.dataset.passwordReady === '1') return;
        button.dataset.passwordReady = '1';
        button.addEventListener('click', () => {
          const target = document.getElementById(button.dataset.passwordToggle || '');
          if (!target) return;
          const showing = target.getAttribute('type') === 'text';
          target.setAttribute('type', showing ? 'password' : 'text');
          button.classList.toggle('is-showing', !showing);
          button.setAttribute('aria-label', `${showing ? 'Show' : 'Hide'} ${target.id === 'accountPasswordConfirm' ? 'confirm password' : 'password'}`);
        });
      });
      document.getElementById('confirmSignupButton').addEventListener('click', () => {
        confirmSignupOtp().catch((error) => accountMessage(friendlyAuthMessage(error.message), true, 'error'));
      });
      document.getElementById('trialConvertForm')?.addEventListener('submit', (event) => {
        event.preventDefault();
        startTrialConversion();
      });
      document.getElementById('trialConvertConfirmButton')?.addEventListener('click', () => {
        confirmTrialConversion();
      });
      document.getElementById('reloadButton').addEventListener('click', () => {
        openReloadSetupDialog();
      });
      document.getElementById('overviewReloadButton')?.addEventListener('click', () => {
        openReloadSetupDialog();
      });
      document.getElementById('reloadSetupCheckoutButton')?.addEventListener('click', () => {
        startReloadFromSetup().catch((error) => {
          reloadSetupMessage(friendlyBillingMessage(error.message, 'Could not add balance. Try again.'), 'error');
        });
      });
      document.getElementById('addCreditsCancelX')?.addEventListener('click', resetReloadSetupDialog);
      document.getElementById('addCreditsDialog')?.addEventListener('click', (event) => {
        if (event.target?.id === 'addCreditsDialog') resetReloadSetupDialog();
      });
      document.getElementById('refreshAccountButton')?.addEventListener('click', () => {
        loadAccount().catch((error) => accountMessage(error.message));
      });
      document.getElementById('manualReloadAmount')?.addEventListener('input', () => {
        updateManualReloadDraftCopy();
      });
      document.getElementById('manualReloadAmount')?.addEventListener('blur', () => {
        normalizeReloadAmountInput('manualReloadAmount');
        updateManualReloadDraftCopy();
      });
      document.getElementById('modalReloadAmount')?.addEventListener('input', () => {
        updateReloadSetupDraftCopy();
      });
      document.getElementById('modalReloadAmount')?.addEventListener('blur', () => {
        normalizeReloadAmountInput('modalReloadAmount');
        updateReloadSetupDraftCopy();
      });
      document.querySelectorAll('[data-reload-amount]').forEach((button) => {
        button.addEventListener('click', () => {
          const amount = button.dataset.reloadAmount || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS);
          const input = document.getElementById('modalReloadAmount');
          if (input) input.value = amount;
          updateReloadSetupDraftCopy();
        });
      });
      document.getElementById('autoReloadToggle')?.addEventListener('change', async (event) => {
        const toggle = event.currentTarget;
        if (!toggle) return;
        if (!toggle.checked) {
          const confirmed = await confirmAutoReloadOff();
          if (!confirmed) {
            toggle.checked = true;
            updateAutoReloadDraftCopy();
            return;
          }
          if (!latestAccountForBilling?.auto_topup_enabled && !latestAccountForBilling?.auto_topup_available) {
            setAutoReloadSetupOptOut(true);
          }
        } else {
          setAutoReloadSetupOptOut(false);
        }
        if (toggle.checked && !latestAccountForBilling?.auto_topup_available) {
          toggle.checked = Boolean(latestAccountForBilling?.auto_topup_enabled);
          updateAutoReloadDraftCopy();
          accountMessage(
            friendlyBillingMessage(
              latestAccountForBilling?.auto_topup_unavailable_reason,
              'Use Add balance to set up Auto Reload with a card.'
            ),
            false,
            'error'
          );
          return;
        }
        updateAutoReloadDraftCopy();
        saveDashboardAutoReloadSettings().catch((error) => {
          accountMessage(friendlyBillingMessage(error.message), false, 'error');
        });
      });
      ['autoReloadThreshold', 'autoReloadAmount'].forEach((id) => {
        document.getElementById(id)?.addEventListener('input', () => {
          updateAutoReloadDraftCopy();
        });
        document.getElementById(id)?.addEventListener('blur', () => {
          normalizeBillingMoneyInput(id);
          updateAutoReloadDraftCopy();
          if (latestAccountForBilling?.auto_topup_available || latestAccountForBilling?.auto_topup_enabled) {
            saveDashboardAutoReloadSettings().catch((error) => accountMessage(error.message, false, 'error'));
          }
        });
      });
      document.getElementById('modalAutoReloadToggle')?.addEventListener('change', async (event) => {
        const toggle = event.currentTarget;
        if (toggle && !toggle.checked) {
          const confirmed = await confirmAutoReloadOff();
          if (!confirmed) {
            toggle.checked = true;
          } else if (!latestAccountForBilling?.auto_topup_enabled && !latestAccountForBilling?.auto_topup_available) {
            setAutoReloadSetupOptOut(true);
          }
        } else if (toggle?.checked) {
          setAutoReloadSetupOptOut(false);
        }
        updateReloadSetupDraftCopy();
      });
      ['modalAutoReloadThreshold', 'modalAutoReloadAmount'].forEach((id) => {
        document.getElementById(id)?.addEventListener('input', () => {
          updateReloadSetupDraftCopy();
        });
        document.getElementById(id)?.addEventListener('blur', () => {
          normalizeBillingMoneyInput(id);
          updateReloadSetupDraftCopy();
        });
      });
      document.getElementById('changeSquareCardButton')?.addEventListener('click', () => {
        openUpdateCardDialog().catch((error) => updateCardMessage(error.message, 'error'));
      });
      document.getElementById('updateCardSaveButton')?.addEventListener('click', () => {
        saveUpdateCard().catch((error) => updateCardMessage(error.message, 'error'));
      });
      document.getElementById('updateCardCancel')?.addEventListener('click', resetUpdateCardDialog);
      document.getElementById('updateCardCancelX')?.addEventListener('click', resetUpdateCardDialog);
      document.getElementById('updateCardDialog')?.addEventListener('click', (event) => {
        if (event.target?.id === 'updateCardDialog') resetUpdateCardDialog();
      });
      document.getElementById('cancelAutoReloadButton')?.addEventListener('click', async () => {
        const button = document.getElementById('cancelAutoReloadButton');
        if (!button || button.disabled) return;
        const confirmed = await confirmAutoReloadOff();
        if (!confirmed) return;
        const previous = button.textContent;
        button.disabled = true;
        button.textContent = 'Turning off...';
        updateAutoReload(false).catch((error) => {
          accountMessage(friendlyBillingMessage(error.message), false, 'error');
        }).finally(() => {
          button.disabled = false;
          button.textContent = previous;
        });
      });
      document.getElementById('reloadSetupCardButton')?.addEventListener('click', () => {
        openReloadSetupCard().catch((error) => reloadSetupMessage(error.message, 'error'));
      });
      document.getElementById('refreshDevicesButton')?.addEventListener('click', () => {
        renderLinkedDevices({ devices: [] }, 'Loading computers...');
        loadLinkedDevices().catch((error) => {
          renderLinkedDevices({ devices: [] }, `Could not load computers: ${error.message}`);
        });
      });
      document.getElementById('refreshAdminAbuseButton')?.addEventListener('click', () => {
        loadAdminAbuse().catch((error) => {
          renderAdminAbuse({}, `Could not load trial abuse events: ${error.message}`);
        });
      });
      document.getElementById('removeAllDevicesButton')?.addEventListener('click', async () => {
        const button = document.getElementById('removeAllDevicesButton');
        if (!button || button.disabled) return;
        const confirmed = await confirmAction({
          title: 'Remove all computers?',
          message: 'Bluey will sign out on every linked desktop for this account.',
          note: 'Saved chats stay in Session History. You can connect a desktop again with its Bluey code.',
          confirmText: 'Remove all',
        });
        if (!confirmed) return;
        const previous = button.textContent;
        button.disabled = true;
        button.textContent = 'Removing...';
        let removed = false;
        revokeAllLinkedDevices()
          .then(() => {
            removed = true;
          })
          .catch((error) => accountMessage(error.message))
          .finally(() => {
            if (!removed) button.disabled = false;
            button.textContent = previous;
          });
      });
      document.getElementById('linkedDevicesList')?.addEventListener('click', async (event) => {
        const button = event.target.closest('button[data-device-id]');
        if (!button) return;
        const label = button.dataset.deviceLabel || 'this computer';
        const confirmed = await confirmAction({
          title: `Remove ${label}?`,
          message: 'Bluey will sign out on that desktop.',
          note: 'Saved chats stay in Session History. You can connect this desktop again from the Bluey host overlay.',
          confirmText: 'Remove desktop',
        });
        if (!confirmed) return;
        const previous = button.textContent;
        button.disabled = true;
        button.textContent = 'Removing...';
        revokeLinkedDevice(button.dataset.deviceId, button.dataset.deviceLabel)
          .catch((error) => accountMessage(error.message))
          .finally(() => {
            button.disabled = false;
            button.textContent = previous;
          });
      });
      initProfileMenus();
      document.querySelectorAll('[data-change-password-button], #changePasswordButton').forEach((button) => {
        if (button.dataset.changePasswordReady === '1') return;
        button.dataset.changePasswordReady = '1';
        button.addEventListener('click', () => {
          closeProfileMenu();
          openChangePasswordDialog();
        });
      });
      document.getElementById('changePasswordForm')?.addEventListener('submit', (event) => {
        event.preventDefault();
        changePassword().catch((error) => changePasswordMessage(error.message, 'error'));
      });
      document.getElementById('changePasswordCancel')?.addEventListener('click', resetChangePasswordDialog);
      document.getElementById('changePasswordCancelX')?.addEventListener('click', resetChangePasswordDialog);
      document.getElementById('changePasswordDialog')?.addEventListener('click', (event) => {
        if (event.target?.id === 'changePasswordDialog') resetChangePasswordDialog();
      });
      document.getElementById('confirmActionCancel')?.addEventListener('click', () => closeConfirmActionDialog(false));
      document.getElementById('confirmActionCancelX')?.addEventListener('click', () => closeConfirmActionDialog(false));
      document.getElementById('confirmActionConfirm')?.addEventListener('click', () => closeConfirmActionDialog(true));
      document.getElementById('confirmActionDialog')?.addEventListener('click', (event) => {
        if (event.target?.id === 'confirmActionDialog') closeConfirmActionDialog(false);
      });
      document.addEventListener('keydown', (event) => {
        const dialog = document.getElementById('confirmActionDialog');
        if (event.key === 'Escape' && dialog && !dialog.hidden) closeConfirmActionDialog(false);
      });
      document.querySelectorAll('[data-delete-account-button], #deleteAccountButton').forEach((button) => {
        if (button.dataset.deleteAccountReady === '1') return;
        button.dataset.deleteAccountReady = '1';
        button.addEventListener('click', () => {
          closeProfileMenu();
          openDeleteAccountDialog();
        });
      });
      document.getElementById('deleteAcceptDataLoss')?.addEventListener('change', updateDeleteAccountConfirmState);
      document.getElementById('deleteAcceptCreditLoss')?.addEventListener('change', updateDeleteAccountConfirmState);
      document.getElementById('deleteConfirmText')?.addEventListener('input', updateDeleteAccountConfirmState);
      document.getElementById('deleteAccountCancel')?.addEventListener('click', resetDeleteAccountDialog);
      document.getElementById('deleteAccountCancelX')?.addEventListener('click', resetDeleteAccountDialog);
      document.getElementById('deleteAccountDialog')?.addEventListener('click', (event) => {
        if (event.target?.id === 'deleteAccountDialog') resetDeleteAccountDialog();
      });
      document.getElementById('deleteAccountConfirm')?.addEventListener('click', () => {
        deleteAccount().catch((error) => accountMessage(error.message));
      });
      document.getElementById('refreshSessionsButton').addEventListener('click', () => {
        setCloudSessionDetail('');
        loadCloudSessions().catch((error) => setCloudSessionDetail(`Could not load saved sessions: ${error.message}`));
      });
      document.getElementById('passwordResetStartForm').addEventListener('submit', (event) => {
        event.preventDefault();
        const email = document.getElementById('resetEmail').value.trim();
        if (!email) return recoveryMessage('Email is required.', 'error');
        startPasswordReset(email).catch((error) => recoveryMessage(error.message, 'error'));
      });
      document.getElementById('passwordResetConfirmForm').addEventListener('submit', (event) => {
        event.preventDefault();
        const token = new URLSearchParams(location.search).get('token') || '';
        const password = document.getElementById('resetPassword').value;
        if (!token) return recoveryMessage('Open the reset email to choose a new password.', 'error');
        if (!password) return recoveryMessage('New password is required.', 'error');
        confirmPasswordReset(token, password).catch((error) => recoveryMessage(error.message, 'error'));
      });
      loadAccount().catch((error) => {
        accountMessage(`Could not load account: ${error.message}`);
      });
    }

    function renderRecoveryRoute() {
      setAccountChrome(false);
      document.getElementById('accountAuthCard').hidden = true;
      document.getElementById('accountPreviewCard').hidden = true;
      document.getElementById('accountDashboard').hidden = true;
      document.getElementById('accountRecoveryCard').hidden = false;
      closeProfileMenu();

      const params = new URLSearchParams(location.search);
      const token = params.get('token') || '';
      const isVerify = currentPath === '/verify-email';
      document.getElementById('recoveryTitle').textContent = isVerify
        ? 'Verify email'
        : token
          ? 'New password'
          : 'Reset password';
      document.getElementById('recoveryCopy').textContent = isVerify
        ? 'Bluey will verify this email token and return you to sign-in.'
        : token
          ? 'Your reset email is ready. Choose a new password for this Bluey account.'
          : 'Enter your account email. Bluey will send a secure password reset email.';
      document.getElementById('passwordResetStartForm').hidden = isVerify || Boolean(token);
      document.getElementById('passwordResetConfirmForm').hidden = isVerify || !token;

      if (isVerify) {
        if (!token) {
          recoveryMessage('Verification token is missing.', 'error');
        } else {
          confirmEmailVerification(token).catch((error) => recoveryMessage(error.message, 'error'));
        }
      }
    }

    function selectDownloadPlatform(platform) {
      const downloadApp = document.getElementById('downloadApp');
      if (downloadApp) {
        downloadApp.classList.toggle('platform-windows', platform === 'windows');
        downloadApp.classList.toggle('platform-mac', platform !== 'windows');
      }
      const cards = document.querySelectorAll('#downloadApp [data-platform-card]');
      const panels = document.querySelectorAll('#downloadApp [data-platform-instructions]');
      cards.forEach((card) => {
        const selected = card.dataset.platformCard === platform;
        card.classList.toggle('active', selected);
        card.setAttribute('aria-expanded', selected ? 'true' : 'false');
      });
      panels.forEach((panel) => {
        const selected = panel.dataset.platformInstructions === platform;
        panel.hidden = !selected;
        if (selected) panel.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
    }

    function initDownloadInstructions() {
      document.querySelectorAll('#downloadApp [data-platform-card]').forEach((card) => {
        if (card.dataset.instructionsReady === '1') return;
        card.dataset.instructionsReady = '1';
        const platform = card.dataset.platformCard;
        const open = () => {
          if (!platform || card.classList.contains('disabled')) return;
          selectDownloadPlatform(platform);
        };
        card.addEventListener('click', (event) => {
          if (event.target.closest('[data-copy]')) return;
          open();
        });
        card.addEventListener('keydown', (event) => {
          if (event.key !== 'Enter' && event.key !== ' ') return;
          event.preventDefault();
          open();
        });
      });
      if (isDownloadRoute && !document.querySelector('#downloadApp [data-platform-card].active')) {
        selectDownloadPlatform('mac');
      }
    }

    document.addEventListener('click', async (event) => {
      if (!event.target.closest('[data-account-profile]')) {
        closeProfileMenu();
      }

      const signOutButton = event.target.closest('[data-sign-out]');
      if (signOutButton) {
        event.preventDefault();
        closeProfileMenu();
        await signOut();
        return;
      }

      const installCommand = event.target.closest('#downloadApp .install-command');
      document.querySelectorAll('#downloadApp .install-command.is-active').forEach((command) => {
        if (command !== installCommand) command.classList.remove('is-active');
      });
      if (installCommand) installCommand.classList.add('is-active');

      const button = event.target.closest('[data-copy]');
      if (!button) return;
      const text = button.dataset.copy || '';
      if (!text) return;
      const old = button.textContent;
      try {
        await navigator.clipboard.writeText(text);
        button.closest('.install-command')?.classList.add('is-active');
        button.textContent = 'Copied!';
        button.classList.add('copied');
        setTimeout(() => {
          button.textContent = old;
          button.classList.remove('copied');
        }, 1200);
      } catch {
        button.textContent = 'Copy failed';
        setTimeout(() => {
          button.textContent = old;
        }, 1500);
      }
    });

    document.getElementById('tryUsButton')?.addEventListener('click', () => {
      startTrial();
    });

    document.getElementById('trialModalClose')?.addEventListener('click', () => {
      setTrialModal(false);
    });

    document.getElementById('blueyTrialModal')?.addEventListener('click', (event) => {
      if (event.target?.id === 'blueyTrialModal') setTrialModal(false);
    });

    document.getElementById('trialCopyLogin')?.addEventListener('click', () => {
      copyTrialLogin().catch(() => {
        const note = document.getElementById('trialNote');
        if (note) note.textContent = 'Copy failed. You can select the username and password above.';
      });
    });

    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        closeProfileMenu();
        resetChangePasswordDialog();
        setTrialModal(false);
      }
    });

    initFilePreview();
    initSiteTheme();
    initProfileMenus();
    initProductJoinForm();
    bootBlueyTerminal();
    syncAccountNav();
    initDownloadInstructions();
    initAccountApp();
}
