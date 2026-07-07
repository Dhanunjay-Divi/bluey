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
    const AUTO_RELOAD_MIN_CENTS = 1500;
    const AUTO_RELOAD_MAX_CENTS = 50000;
    const AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 500;
    const AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 1500;
    const LEGACY_AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 1000;
    const LEGACY_AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 3000;
    const MANUAL_RELOAD_MIN_CENTS = 1500;
    const MANUAL_RELOAD_AMOUNT_CENTS = 1500;
    let captchaConfigPromise = null;
    let captchaConfig = { provider: null, site_key: null };
    let signupTurnstileWidgetId = null;
    let signupTurnstileToken = '';
    let trialTurnstileWidgetId = null;
    let trialTurnstileToken = '';
    let pendingTrialButton = null;
    const DEVICE_LINK_TTL_MS = 10 * 60 * 1000;
    const DEVICE_LINK_STORAGE_KEY = 'bluey_pending_device_code';
    const ACCESS_TOKEN_KEY = 'bluey_access_token';
    const REFRESH_TOKEN_KEY = 'bluey_refresh_token';
    const AUTH_PERSISTENCE_KEY = 'bluey_auth_persistence';

    function money(cents) {
      return `$${(Number(cents || 0) / 100).toFixed(2)}`;
    }

    function centsToDollars(cents) {
      return String(Math.round(Number(cents || 0) / 100));
    }

    function dollarsToCents(value) {
      const dollars = Number(value);
      if (!Number.isFinite(dollars)) return NaN;
      return Math.round(dollars * 100);
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

    function rememberPendingDeviceCode(code) {
      if (!code) return;
      try {
        sessionStorage.setItem(DEVICE_LINK_STORAGE_KEY, JSON.stringify({
          code,
          saved_at: Date.now(),
        }));
      } catch {
        // Browser storage can be disabled; the query-param path still works.
      }
    }

    function storedPendingDeviceCode() {
      try {
        const raw = sessionStorage.getItem(DEVICE_LINK_STORAGE_KEY);
        if (!raw) return '';
        const parsed = JSON.parse(raw);
        const code = normalizeDeviceCode(parsed?.code);
        const savedAt = Number(parsed?.saved_at || 0);
        if (!code || !savedAt || Date.now() - savedAt > DEVICE_LINK_TTL_MS) {
          sessionStorage.removeItem(DEVICE_LINK_STORAGE_KEY);
          return '';
        }
        return code;
      } catch {
        sessionStorage.removeItem(DEVICE_LINK_STORAGE_KEY);
        return '';
      }
    }

    function pendingDeviceCode() {
      const params = new URLSearchParams(location.search);
      const code = normalizeDeviceCode(params.get('user_code') || params.get('device_code') || '');
      if (code) {
        rememberPendingDeviceCode(code);
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
        sessionStorage.setItem(`bluey_device_approved_${code}`, '1');
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
          copy: 'This browser already has a Bluey account. Download Bluey and run bluey on, or open Dashboard to manage credits and saved sessions.',
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
          copy: 'The Bluey website is updated, but the temporary-trial server endpoint is not live yet. You can still download Bluey or create a normal account now.',
          note: 'No trial minutes were created and nothing was charged. The 15-minute trial will turn on after the API rollout finishes.',
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
        el.replaceChildren();
      }

      const renderCodeEntry = (el) => {
        el.hidden = false;
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
        button.className = 'account-button secondary compact';
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
        if (accountToken() && dashboardHint) renderCodeEntry(dashboardHint);
        return;
      }

      const renderInto = (el) => {
        el.hidden = false;
        el.replaceChildren();
        const title = document.createElement('strong');
        const approved = sessionStorage.getItem(`bluey_device_approved_${code}`) === '1';
        title.textContent = approved ? 'Desktop Bluey is connected' : 'Finish moving desktop Bluey';
        const body = document.createElement('span');
        if (approved) {
          body.textContent = 'Return to Terminal or Bluey. This browser tab can stay open.';
          el.append(title, body);
          return;
        }
        body.append('Terminal is waiting on code ');
        const codeEl = document.createElement('code');
        codeEl.textContent = code;
        if (accountToken()) {
          body.append(
            codeEl,
            `. This moves only that desktop to ${currentAccountEmail || 'this Bluey account'} so it uses this account's shared balance.`
          );
        } else {
          body.append(
            codeEl,
            '. This code came from your desktop app. Sign in or create an account here, then confirm the desktop link before Bluey can use credits or cloud answers.'
          );
        }
        el.append(title, body);
        if (accountToken()) {
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'account-button secondary compact';
          button.textContent = 'Move desktop';
          button.setAttribute('aria-label', `Move desktop Bluey using code ${code}`);
          button.addEventListener('click', () => {
            button.disabled = true;
            button.textContent = 'Connecting...';
            sessionStorage.setItem(`bluey_device_confirmed_${code}`, '1');
            approvePendingDevice()
              .then(() => loadAccount())
              .catch((error) => {
                button.disabled = false;
                button.textContent = 'Move desktop';
                accountMessage(`Desktop link failed: ${error.message}`);
              });
            });
          el.append(button);
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
      const storageKey = `bluey_device_approved_${code}`;
      if (sessionStorage.getItem(storageKey) === '1') return true;
      if (sessionStorage.getItem(`bluey_device_confirmed_${code}`) !== '1') {
        accountMessage('Press Move desktop to finish Bluey login.');
        return false;
      }
      accountMessage('Connecting this account to the desktop app...');
      await apiJson('/auth/device/approve', {
        method: 'POST',
        body: JSON.stringify({ user_code: code }),
      });
      sessionStorage.setItem(storageKey, '1');
      accountMessage('Desktop linked. Return to Bluey; it will finish automatically.');
      return true;
    }

    async function openDesktopDeepLinkIfNeeded() {
      if (!['/login', '/link'].includes(currentPath) || pendingDeviceCode() || !accountToken()) return false;
      if (sessionStorage.getItem('bluey_desktop_deep_link_started') === '1') return false;
      accountMessage('Opening Bluey desktop...');
      const link = await apiJson('/auth/link/mint', {
        method: 'POST',
        body: JSON.stringify({}),
      });
      sessionStorage.setItem('bluey_desktop_deep_link_started', '1');
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
      accountMessage('', true);
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
        accountMessage('Confirm account deletion, credit loss, and type DELETE first.');
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

    function renderUsage(usage) {
      document.getElementById('usageValue').textContent = `${usage.total_cues || 0}`;
      document.getElementById('usageHint').textContent = `${money(usage.total_cents_spent)} in ${usage.period_days || 7} days.`;
      document.getElementById('tierValue').textContent = usage.tier_label || '--';
      const projectedDays = Math.round(usage.projected_days_remaining || 0);
      const projectionLabel = (usage.projection_label || '').trim();
      document.getElementById('projectionHint').textContent = projectionLabel
        || (projectedDays > 0
          ? `~${Math.min(projectedDays, 90)} days at recent pace.`
          : 'Appears after usage.');
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
          existing.addEventListener('error', () => reject(new Error('Could not load Square card form.')), { once: true });
        });
        return;
      }
      await new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = src;
        script.async = true;
        script.dataset.squareSdk = environment;
        script.onload = resolve;
        script.onerror = () => reject(new Error('Could not load Square card form.'));
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
          throw new Error('Square card form is unavailable.');
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

    function renderAutoReload(me) {
      latestAccountForBilling = me || null;
      const card = document.getElementById('autoReloadCard');
      const hint = document.getElementById('autoReloadHint');
      const method = document.getElementById('autoReloadMethod');
      const toggle = document.getElementById('autoReloadToggle');
      const setup = document.getElementById('squareCardSetup');
      const saveButton = document.getElementById('saveSquareCardButton');
      const saveAutoReloadButton = document.getElementById('saveAutoReloadButton');
      const changeCardButton = document.getElementById('changeSquareCardButton');
      const thresholdInput = document.getElementById('autoReloadThreshold');
      const amountInput = document.getElementById('autoReloadAmount');
      if (!card || !hint || !method || !toggle || !setup) return;

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

      const setupDefaultOn = !me?.auto_topup_enabled && canSaveSquareCard && !hasSavedMethod;
      const toggleChecked = Boolean(me?.auto_topup_enabled) || setupDefaultOn;
      card.classList.toggle('is-on', Boolean(me?.auto_topup_enabled));
      card.classList.toggle('is-setup-default', setupDefaultOn);
      toggle.checked = toggleChecked;
      toggle.disabled = false;
      if (setup.dataset.open !== '1') {
        setup.hidden = true;
      }
      if (saveButton) {
        saveButton.disabled = !canSaveSquareCard;
        saveButton.textContent = hasSavedMethod ? 'Update card' : 'Save card';
      }
      if (changeCardButton) {
        changeCardButton.hidden = !canSaveSquareCard || setup.dataset.open === '1';
        changeCardButton.disabled = !canSaveSquareCard;
        changeCardButton.textContent = hasSavedMethod ? 'Change card' : 'Save card';
      }
      if (thresholdInput) thresholdInput.value = centsToDollars(thresholdCents);
      if (amountInput) amountInput.value = centsToDollars(amountCents);

      if (me?.auto_topup_enabled) {
        hint.textContent = `On. Reloads ${amount} when below ${threshold}.`;
      } else if (hasSavedMethod) {
        hint.textContent = 'Off. Turn on and click Save to enable.';
      } else if (canSaveSquareCard) {
        hint.textContent = 'Ready to activate when you save a card during checkout.';
      } else {
        hint.textContent = me?.auto_topup_unavailable_reason || 'Unavailable';
      }

      if (saveAutoReloadButton) {
        const showSave = hasSavedMethod || Boolean(me?.auto_topup_enabled);
        saveAutoReloadButton.hidden = !showSave;
        saveAutoReloadButton.disabled = !showSave;
        saveAutoReloadButton.textContent = 'Save';
      }

      method.textContent = me?.saved_payment_method_label
        ? `Saved card: ${me.saved_payment_method_label}`
        : canSaveSquareCard
          ? 'No saved card.'
          : me?.auto_topup_unavailable_reason || 'Card saving unavailable.';
      updateAutoReloadDraftCopy();
      renderReloadSetupCardState(me);
      if (!document.getElementById('addCreditsDialog')?.hidden) updateReloadSetupDraftCopy();
    }

    function readAutoReloadSettings(options = {}) {
      const thresholdInputId = options.thresholdInputId || 'autoReloadThreshold';
      const amountInputId = options.amountInputId || 'autoReloadAmount';
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
      const saveButton = document.getElementById('saveAutoReloadButton');
      if (!rule) return;
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      if (!enabled) {
        rule.textContent = wasEnabled ? 'Click Save to turn Auto Reload off.' : 'Auto Reload is ready when you want it.';
        if (saveButton) {
          saveButton.hidden = !wasEnabled && !hasSavedMethod;
          saveButton.disabled = !wasEnabled;
        }
        return;
      }
      try {
        const settings = readAutoReloadSettings();
        rule.textContent = hasSavedMethod
          ? `When balance is below ${money(settings.auto_topup_threshold_cents)}, reload ${money(settings.auto_topup_amount_cents)}.`
          : `Save a card during Add credits to reload ${money(settings.auto_topup_amount_cents)} when below ${money(settings.auto_topup_threshold_cents)}.`;
        if (saveButton) {
          saveButton.hidden = !hasSavedMethod;
          saveButton.disabled = !hasSavedMethod;
        }
      } catch (error) {
        rule.textContent = error.message;
        if (saveButton) {
          saveButton.hidden = !hasSavedMethod;
          saveButton.disabled = true;
        }
      }
    }

    function readManualReloadCents(inputId = 'manualReloadAmount') {
      const input = document.getElementById(inputId);
      const amount = dollarsToCents(input?.value || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS));
      if (!Number.isFinite(amount)) {
        input?.focus();
        throw new Error('Enter a reload amount.');
      }
      if (amount < MANUAL_RELOAD_MIN_CENTS) {
        input?.focus();
        throw new Error('Minimum reload is $15.');
      }
      return amount;
    }

    function normalizeReloadAmountInput(inputId) {
      const input = document.getElementById(inputId);
      if (!input) return;
      const amount = dollarsToCents(input.value || centsToDollars(MANUAL_RELOAD_AMOUNT_CENTS));
      if (!Number.isFinite(amount) || amount < MANUAL_RELOAD_MIN_CENTS) {
        input.value = centsToDollars(MANUAL_RELOAD_MIN_CENTS);
      }
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
        checkoutWindow.document.body.textContent = `Opening checkout for ${money(amountCents)} Bluey credits...`;
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
      const modalRule = document.getElementById('modalReloadRule');
      try {
        const amount = readManualReloadCents();
        if (rule) rule.textContent = `${money(amount)} adds ${money(amount)} credits.`;
        if (modalRule) modalRule.textContent = `${money(amount)} adds ${money(amount)} credits after Square confirms payment.`;
        if (overviewReloadButton) overviewReloadButton.textContent = 'Add credits';
      } catch (error) {
        if (rule) rule.textContent = error.message;
        if (modalRule) modalRule.textContent = error.message;
        if (overviewReloadButton) overviewReloadButton.textContent = 'Add credits';
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
      const balanceCents = Number(me?.balance_cents || 0);
      if (balanceCents <= 0) {
        return 'Add credits to start. This balance is shared across your Bluey account.';
      }
      if (balanceCents < 500) {
        return 'Low balance. Add credits to keep Bluey ready.';
      }
      return 'Ready for paid answers, screen context, and saved sessions.';
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
        ? `Auto Reload is on. Bluey will reload ${money(settings.auto_topup_amount_cents)} when balance drops below ${money(settings.auto_topup_threshold_cents)}.`
        : 'Auto Reload is off. You can add credits manually whenever you need them.');
      return me;
    }

    async function saveAutoReloadSettings() {
      const button = document.getElementById('saveAutoReloadButton');
      const toggle = document.getElementById('autoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);

      if (!enabled && !wasEnabled) {
        updateAutoReloadDraftCopy();
        return;
      }
      if (enabled && !latestAccountForBilling?.auto_topup_available) {
        throw new Error('Auto Reload needs a saved card. Use Add credits to set it up.');
      }

      const previous = button?.textContent || 'Save';
      if (button) {
        button.disabled = true;
        button.textContent = 'Saving...';
      }
      try {
        await updateAutoReload(enabled);
      } finally {
        if (button) {
          button.textContent = previous;
        }
        updateAutoReloadDraftCopy();
      }
    }

    async function openSquareCardSetup(options = {}) {
      const button = document.getElementById(options.buttonId || 'changeSquareCardButton');
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
        if (button) button.hidden = true;
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

    async function saveSquareCard(options = {}) {
      const button = document.getElementById(options.buttonId || 'saveSquareCardButton');
      if (!squareCard || !squareCardAttached) {
        throw new Error('Card form is still loading. Try again in a moment.');
      }
      const previous = button?.textContent || 'Save card';
      if (button) {
        button.disabled = true;
        button.textContent = 'Saving...';
      }
      try {
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
          throw new Error(details || 'Card could not be saved.');
        }
        let me = await apiJson('/billing/square/card', {
          method: 'POST',
          body: JSON.stringify({ source_id: result.token }),
        });
        closeSquareCardSetup({
          setupId: options.setupId || squareCardSetupId || 'squareCardSetup',
          containerId: options.containerId || squareCardContainerId || 'squareCardContainer',
          changeButtonId: options.changeButtonId || 'changeSquareCardButton',
        });
        renderAutoReload(me);
        if (typeof options.onSaved === 'function') options.onSaved(me);
        accountMessage('Card saved. Auto Reload can now be saved when you want it active.');
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
      }
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
      const defaultAutoReloadOn = wasEnabled || (canSaveSquareCard && !hasSavedMethod);

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
      const saveAutoButton = document.getElementById('reloadSetupSaveAutoButton');
      if (!cardButton && !saveAutoButton) return;
      const canSaveSquareCard = canUseSquareCardSetup(me);
      const hasSavedMethod = Boolean(me?.auto_topup_available);

      if (cardButton) {
        cardButton.disabled = !canSaveSquareCard;
        cardButton.textContent = hasSavedMethod ? 'Change card' : 'Add card for Auto Reload';
      }
      if (saveAutoButton) {
        const showSave = hasSavedMethod || Boolean(me?.auto_topup_enabled);
        saveAutoButton.hidden = !showSave;
        saveAutoButton.disabled = !showSave;
      }
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
      const reloadRule = document.getElementById('modalReloadRule');
      const autoRule = document.getElementById('modalAutoReloadRule');
      const saveAutoButton = document.getElementById('reloadSetupSaveAutoButton');
      const cardButton = document.getElementById('reloadSetupCardButton');
      const cardActions = document.querySelector('#addCreditsDialog .reload-setup-card-actions');
      const toggle = document.getElementById('modalAutoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      const canSaveSquareCard = canUseSquareCardSetup(latestAccountForBilling);

      try {
        const amount = readManualReloadCents('modalReloadAmount');
        if (reloadRule) reloadRule.textContent = `${money(amount)} adds ${money(amount)} credits after Square confirms payment.`;
        if (checkoutButton) {
          checkoutButton.textContent = `Continue to Square checkout`;
          checkoutButton.disabled = false;
        }
      } catch (error) {
        if (reloadRule) reloadRule.textContent = error.message;
        if (checkoutButton) {
          checkoutButton.textContent = 'Continue to Square checkout';
          checkoutButton.disabled = true;
        }
      }
      updateReloadAmountPresets();

      if (!autoRule) return;
      if (!enabled) {
        autoRule.textContent = wasEnabled ? 'Auto Reload will turn off before checkout.' : 'Off for this checkout. You can add credits manually anytime.';
        if (cardActions) cardActions.hidden = !hasSavedMethod && !wasEnabled;
        if (saveAutoButton) {
          saveAutoButton.hidden = !wasEnabled && !hasSavedMethod;
          saveAutoButton.disabled = !wasEnabled;
        }
        return;
      }

      try {
        const settings = readModalAutoReloadSettings();
        if (cardActions) cardActions.hidden = false;
        if (cardButton) cardButton.hidden = hasSavedMethod ? false : !canSaveSquareCard;
        autoRule.textContent = hasSavedMethod
          ? `When balance is below ${money(settings.auto_topup_threshold_cents)}, Bluey reloads ${money(settings.auto_topup_amount_cents)} from your saved card.`
          : canSaveSquareCard
            ? `Add a card here to reload ${money(settings.auto_topup_amount_cents)} when balance is below ${money(settings.auto_topup_threshold_cents)}.`
            : 'Auto Reload is not available for this account yet. You can still add credits with checkout.';
        if (saveAutoButton) {
          saveAutoButton.hidden = !hasSavedMethod;
          saveAutoButton.disabled = !hasSavedMethod;
        }
      } catch (error) {
        autoRule.textContent = error.message;
        if (cardActions) cardActions.hidden = !hasSavedMethod;
        if (saveAutoButton) {
          saveAutoButton.hidden = !hasSavedMethod;
          saveAutoButton.disabled = true;
        }
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
      if (!dialog) {
        startReload();
        return;
      }
      syncReloadSetupFromDashboard();
      reloadSetupMessage('');
      dialog.hidden = false;
      setTimeout(() => amount?.focus(), 0);
    }

    async function openReloadSetupCard() {
      await openSquareCardSetup({
        buttonId: 'reloadSetupCardButton',
        setupId: 'reloadSetupSquareCardSetup',
        containerId: 'reloadSetupSquareCardContainer',
      });
      reloadSetupMessage('Enter a card, then save. Square handles the card details.');
    }

    async function saveReloadSetupCard() {
      await saveSquareCard({
        buttonId: 'reloadSetupSaveSquareCardButton',
        setupId: 'reloadSetupSquareCardSetup',
        containerId: 'reloadSetupSquareCardContainer',
        changeButtonId: 'reloadSetupCardButton',
        onSaved: (me) => {
          latestAccountForBilling = me || latestAccountForBilling;
          renderReloadSetupCardState(me);
          updateReloadSetupDraftCopy();
        },
      });
      if (document.getElementById('modalAutoReloadToggle')?.checked && latestAccountForBilling?.auto_topup_available) {
        applyReloadSetupToDashboard();
        await updateAutoReload(true);
        reloadSetupMessage('Card saved. Auto Reload is on. Continue to checkout to add credits.', 'success');
      } else {
        reloadSetupMessage('Card saved. Continue to checkout to add credits.', 'success');
      }
    }

    async function saveReloadSetupAutoReload() {
      applyReloadSetupToDashboard();
      await saveAutoReloadSettings();
      updateReloadSetupDraftCopy();
      reloadSetupMessage('Auto Reload settings saved.', 'success');
    }

    async function saveReloadSetupAutoReloadIfReady() {
      const toggle = document.getElementById('modalAutoReloadToggle');
      const enabled = Boolean(toggle?.checked);
      const wasEnabled = Boolean(latestAccountForBilling?.auto_topup_enabled);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);

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
      try {
        readManualReloadCents('modalReloadAmount');
      } catch (error) {
        reloadSetupMessage(error.message, 'error');
        document.getElementById('modalReloadAmount')?.focus();
        updateReloadSetupDraftCopy();
        return false;
      }
      const autoReloadSelected = Boolean(document.getElementById('modalAutoReloadToggle')?.checked);
      const hasSavedMethod = Boolean(latestAccountForBilling?.auto_topup_available);
      const canSaveSquareCard = canUseSquareCardSetup(latestAccountForBilling);
      const cardSetup = document.getElementById('reloadSetupSquareCardSetup');

      if (autoReloadSelected && !hasSavedMethod) {
        if (!canSaveSquareCard) {
          reloadSetupMessage('Auto Reload is not available yet. Turn it off to continue with a one-time checkout.', 'error');
          return false;
        }
        if (cardSetup?.hidden) {
          await openReloadSetupCard();
          reloadSetupMessage('Auto Reload is on. Add a card here, or turn Auto Reload off for a one-time checkout.', 'error');
          return false;
        }
        await saveReloadSetupCard();
        if (!latestAccountForBilling?.auto_topup_available) {
          reloadSetupMessage('Add a card for Auto Reload, or turn Auto Reload off to continue.', 'error');
          return false;
        }
      }

      reloadSetupMessage('Preparing Square checkout...');
      await saveReloadSetupAutoReloadIfReady();
      reloadSetupMessage('Opening Square checkout...');
      const opened = await startReload();
      reloadSetupMessage(opened
        ? 'Checkout opened in a new tab. Complete payment there, then return to Bluey.'
        : 'Checkout did not open. Check the message above and try again.',
      opened ? 'success' : 'error');
      return opened;
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

    function renderLinkedDevices(payload, message = '') {
      const list = document.getElementById('linkedDevicesList');
      if (!list) return;
      list.replaceChildren();
      const devices = Array.isArray(payload?.devices) ? payload.devices : [];
      const computers = devices.filter((device) => !isBrowserDevice(device));
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
      const sessions = Array.isArray(payload?.sessions) ? payload.sessions : [];
      if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No uploaded desktop sessions yet. Sign in from Bluey desktop, then refresh after a chat or transcript syncs.';
        list.append(empty);
        return;
      }

      for (const session of sessions) {
        const counts = sessionUploadCounts(session);
        const hasContent = sessionHasUploadedContent(session);
        const row = document.createElement('article');
        row.className = `session-row${hasContent ? '' : ' session-row-empty'}`;

        const body = document.createElement('div');
        const title = document.createElement('strong');
        title.className = 'session-title';
        title.textContent = session.title || `Session ${shortSessionId(session.session_id)}`;
        const meta = document.createElement('span');
        meta.className = 'session-meta';
        meta.textContent = hasContent
          ? `${session.status || 'uploaded'} - ${formatSessionTime(session.updated_at_ms || session.last_active_at_ms)}`
          : `Session record uploaded - no chat transcript or Bluey answers yet - ${formatSessionTime(session.updated_at_ms || session.last_active_at_ms)}`;
        const chips = document.createElement('div');
        chips.className = 'session-upload-chips';
        for (const chipText of sessionUploadChipLabels(counts)) {
          const chip = document.createElement('span');
          chip.className = 'session-upload-chip';
          chip.textContent = chipText;
          chips.append(chip);
        }
        body.append(title, meta, chips);

        const copyId = document.createElement('button');
        copyId.className = 'account-button ghost compact';
        copyId.type = 'button';
        copyId.dataset.copy = session.session_id || '';
        copyId.textContent = 'Copy ID';

        const link = document.createElement('a');
        link.className = 'account-button ghost';
        link.href = accountSessionHref(session.session_id);
        link.target = '_blank';
        link.rel = 'noopener';
        link.dataset.sessionId = session.session_id || '';
        link.setAttribute('aria-label', `Open ${session.title || 'uploaded session'} in a new tab`);
        link.title = hasContent
          ? 'Open uploaded conversation in a new tab'
          : 'Open upload status in a new tab';
        link.textContent = 'Open tab';
        row.append(body, copyId, link);
        list.append(row);
      }
    }

    function countLabel(count, singular, plural = `${singular}s`) {
      const value = Number(count || 0);
      return `${value} ${value === 1 ? singular : plural}`;
    }

    function sessionUploadCounts(session) {
      return {
        transcript: Number(session?.transcript_count || 0),
        responses: Number(session?.response_count || 0),
        context: Number(session?.context_count || 0),
      };
    }

    function sessionHasUploadedContent(session) {
      const counts = sessionUploadCounts(session);
      return counts.transcript > 0 || counts.responses > 0 || counts.context > 0;
    }

    function sessionUploadChipLabels(counts) {
      if (!counts.transcript && !counts.responses && !counts.context) {
        return ['No chat uploaded yet'];
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
      const transcriptSegments = Array.isArray(bundle.transcript_segments) ? bundle.transcript_segments : [];
      const cueResponses = Array.isArray(bundle.cue_responses) ? bundle.cue_responses : [];
      const contextArtifacts = Array.isArray(bundle.context_artifacts) ? bundle.context_artifacts : [];
      const hasUploadedContent = transcriptSegments.length > 0 || cueResponses.length > 0 || contextArtifacts.length > 0;

      const title = document.createElement('strong');
      title.textContent = bundle.session?.title || `Session ${shortSessionId(sessionId)}`;
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
        : 'Only the session record has uploaded so far. No local chat transcript, Bluey answers, or context have arrived for this session yet.';
      detail.append(title, meta, uploadState);

      const copyId = document.createElement('button');
      copyId.className = 'account-button ghost compact';
      copyId.type = 'button';
      copyId.dataset.copy = sessionId;
      copyId.textContent = 'Copy full session ID';
      detail.append(copyId);

      if (bundle.session?.answer_style) {
        appendBundlePreview(detail, 'Answer style', bundle.session.answer_style);
      }
      appendBundlePreview(detail, 'Diagnostics', diagnosticsText(bundle.session?.metadata));
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
              ? 'Square checkout'
              : 'No subscription';
        }
        const balanceValue = document.getElementById('balanceValue');
        const balanceCard = balanceValue?.closest('.balance-kpi');
        const balanceTitle = document.getElementById('balanceTitle');
        const overviewReloadButton = document.getElementById('overviewReloadButton');
        const balanceReloadPanel = document.getElementById('balanceReloadPanel');
        const isTemporaryAccount = Boolean(me.is_temporary);
        const trialMinutes = trialMinutesRemaining(me);
        if (balanceTitle) balanceTitle.textContent = isTemporaryAccount ? 'Trial time' : 'Remaining balance';
        if (balanceValue) {
          balanceValue.textContent = isTemporaryAccount ? `${trialMinutes} min` : money(me.balance_cents);
          balanceCard?.classList.toggle('balance-trial', isTemporaryAccount);
          balanceCard?.classList.toggle('balance-critical', !isTemporaryAccount && me.balance_cents > 0 && me.balance_cents < 500);
          balanceCard?.classList.toggle('balance-low', !isTemporaryAccount && me.balance_cents >= 500 && me.balance_cents < 1000);
        }
        if (overviewReloadButton) {
          overviewReloadButton.hidden = isTemporaryAccount;
        }
        if (balanceReloadPanel) {
          balanceReloadPanel.hidden = isTemporaryAccount;
        }
        const summaryBalanceValue = document.getElementById('summaryBalanceValue');
        const summaryBalanceLabel = document.getElementById('summaryBalanceLabel');
        const summaryBalanceCard = summaryBalanceValue?.closest('.summary-balance-kpi');
        if (summaryBalanceLabel) summaryBalanceLabel.textContent = isTemporaryAccount ? 'Trial time' : 'Credits balance';
        if (summaryBalanceValue) {
          summaryBalanceValue.textContent = isTemporaryAccount ? `${trialMinutes} min` : money(me.balance_cents);
          summaryBalanceCard?.classList.toggle('balance-critical', !isTemporaryAccount && me.balance_cents > 0 && me.balance_cents < 500);
          summaryBalanceCard?.classList.toggle('balance-low', !isTemporaryAccount && me.balance_cents >= 500 && me.balance_cents < 1000);
        }
        const balanceHint = document.getElementById('balanceHint');
        const summaryBalanceHint = document.getElementById('summaryBalanceHint');
        const balanceHintText = accountBalanceHint(me);
        if (balanceHint) balanceHint.textContent = balanceHintText;
        if (summaryBalanceHint) summaryBalanceHint.textContent = balanceHintText;
        renderTemporaryAccount(me);
        renderUsage(usage);
        renderAutoReload(me);
        updateManualReloadDraftCopy();
        const isAdmin = Boolean(me.is_admin);
        currentAccountIsAdmin = isAdmin;
        if (!isAdmin) adminAbuseLoaded = false;
        setAdminDashboardAvailability(isAdmin);
        const adminSection = document.getElementById('adminAbuseSection');
        if (adminSection) adminSection.hidden = !isAdmin;
        if (isAdmin && normalizeDashboardTabName(window.location.hash) === 'admin') {
          setDashboardTab('admin');
        }
        void linkedDevicesPromise;
        void sessionsPromise;
        accountMessage(new URLSearchParams(location.search).get('reload') === 'success'
          ? 'Checkout complete. If the balance still looks old, Square is finishing the credit event; press Refresh balance in a moment.'
          : '');
        openAccountActionFromHash();
        try {
          const approved = await approvePendingDevice();
          if (!approved) await openDesktopDeepLinkIfNeeded();
        } catch (error) {
          accountMessage(`Account signed in, but desktop handoff failed: ${error.message}`);
        }
      } finally {
        if (refreshButton) refreshButton.disabled = false;
      }
    }

    async function startReload() {
      let amountCents;
      try {
        amountCents = readManualReloadCents();
      } catch (error) {
        accountMessage(error.message, false, 'error');
        return false;
      }
      const checkoutWindow = openCheckoutPlaceholder(amountCents);
      setReloadButtonsBusy(true);
      accountMessage(`Opening checkout for ${money(amountCents)} Bluey credits...`);
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
            accountMessage('Checkout was blocked by the browser. Allow popups for bluey.sh, then click Add credits again.', false, 'error');
            return false;
          }
        }
        accountMessage('Checkout opened in a new tab. Complete payment there, then return here and press Refresh balance.');
        return true;
      } catch (error) {
        closeCheckoutPlaceholder(checkoutWindow);
        const message = String(error?.message || 'Could not open checkout.');
        if (/billing provider unavailable|billing checkout failed/i.test(message)) {
          accountMessage(`${message} Please retry in a moment.`, false, 'error');
        } else {
          accountMessage(message, false, 'error');
        }
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

      document.getElementById('accountForm').addEventListener('submit', (event) => {
        event.preventDefault();
        if (accountAuthMode === 'signup') {
          startSignupOtp().catch((error) => accountMessage(error.message, true, 'error'));
        } else {
          accountAuth('login').catch((error) => accountMessage(error.message, true, 'error'));
        }
      });
      document.getElementById('createAccountButton').addEventListener('click', () => {
        if (pendingSignupEmail) {
          startSignupOtp().catch((error) => accountMessage(error.message, true, 'error'));
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
        confirmSignupOtp().catch((error) => accountMessage(error.message, true, 'error'));
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
        startReloadFromSetup().catch((error) => reloadSetupMessage(error.message, 'error'));
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
      document.getElementById('autoReloadToggle')?.addEventListener('change', (event) => {
        if (event.currentTarget?.checked && !latestAccountForBilling?.auto_topup_available) {
          event.currentTarget.checked = Boolean(latestAccountForBilling?.auto_topup_enabled);
          updateAutoReloadDraftCopy();
          openReloadSetupDialog();
          return;
        }
        updateAutoReloadDraftCopy();
      });
      ['autoReloadThreshold', 'autoReloadAmount'].forEach((id) => {
        document.getElementById(id)?.addEventListener('input', () => {
          updateAutoReloadDraftCopy();
        });
      });
      document.getElementById('modalAutoReloadToggle')?.addEventListener('change', () => {
        updateReloadSetupDraftCopy();
      });
      ['modalAutoReloadThreshold', 'modalAutoReloadAmount'].forEach((id) => {
        document.getElementById(id)?.addEventListener('input', () => {
          updateReloadSetupDraftCopy();
        });
      });
      document.getElementById('saveAutoReloadButton')?.addEventListener('click', () => {
        saveAutoReloadSettings().catch((error) => accountMessage(error.message));
      });
      document.getElementById('reloadSetupSaveAutoButton')?.addEventListener('click', () => {
        saveReloadSetupAutoReload().catch((error) => reloadSetupMessage(error.message, 'error'));
      });
      document.getElementById('saveSquareCardButton')?.addEventListener('click', () => {
        saveSquareCard().catch((error) => accountMessage(error.message));
      });
      document.getElementById('changeSquareCardButton')?.addEventListener('click', () => {
        openSquareCardSetup().catch((error) => accountMessage(error.message));
      });
      document.getElementById('reloadSetupCardButton')?.addEventListener('click', () => {
        openReloadSetupCard().catch((error) => reloadSetupMessage(error.message, 'error'));
      });
      document.getElementById('reloadSetupSaveSquareCardButton')?.addEventListener('click', () => {
        saveReloadSetupCard().catch((error) => reloadSetupMessage(error.message, 'error'));
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
      document.getElementById('removeAllDevicesButton')?.addEventListener('click', () => {
        const button = document.getElementById('removeAllDevicesButton');
        if (!button || button.disabled) return;
        if (!window.confirm('Remove all linked computers and sign Bluey out on those desktops?')) return;
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
      document.getElementById('linkedDevicesList')?.addEventListener('click', (event) => {
        const button = event.target.closest('button[data-device-id]');
        if (!button) return;
        const label = button.dataset.deviceLabel || 'this computer';
        if (!window.confirm(`Remove ${label} and sign Bluey out on that desktop?`)) return;
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
