if (!window.__BLUEY_SITE_BOOTED__) {
  window.__BLUEY_SITE_BOOTED__ = true;

    const policyApp = document.getElementById('policyApp');
    const accountApp = document.getElementById('accountApp');
    const downloadApp = document.getElementById('downloadApp');
    const productSite = document.getElementById('productSite');
    const downloadRoutes = new Set(['/download', '/install']);
    const accountRoutes = new Set(['/account', '/reload', '/link', '/login', '/device', '/verify-email', '/password-reset']);
    const policyRoutes = new Set(['/docs/privacy', '/docs/terms', '/docs/disguise']);
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
    const isAccountRoute = accountRoutes.has(currentPath);
    const isPolicyRoute = policyRoutes.has(currentPath);
    const isDownloadRoute = downloadRoutes.has(currentPath);
    let pendingSignupEmail = '';
    let accountAuthMode = 'login';
    let squareCard = null;
    let squareCardEnvironment = '';
    let squareCardSetupPromise = null;
    let refreshAccountTokenPromise = null;
    let currentAccountEmail = '';
    const AUTO_RELOAD_MIN_CENTS = 1500;
    const AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 1000;
    const AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 3000;
    const LEGACY_AUTO_RELOAD_DEFAULT_THRESHOLD_CENTS = 500;
    const LEGACY_AUTO_RELOAD_DEFAULT_AMOUNT_CENTS = 1500;
    const MANUAL_RELOAD_MIN_CENTS = 1500;
    const MANUAL_RELOAD_AMOUNT_CENTS = 3000;
    let captchaConfigPromise = null;
    let captchaConfig = { provider: null, site_key: null };
    let signupTurnstileWidgetId = null;
    let signupTurnstileToken = '';

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
      return localStorage.getItem('bluey_access_token') || '';
    }

    function pendingDeviceCode() {
      const params = new URLSearchParams(location.search);
      const code = params.get('user_code') || params.get('device_code') || '';
      return code.trim().toUpperCase();
    }

    function setAccountToken(auth) {
      localStorage.setItem('bluey_access_token', auth.access_token);
      localStorage.setItem('bluey_refresh_token', auth.refresh_token || '');
      syncAccountNav();
    }

    function clearAccountToken() {
      localStorage.removeItem('bluey_access_token');
      localStorage.removeItem('bluey_refresh_token');
      currentAccountEmail = '';
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

    async function revokeBrowserSession() {
      const previousRefreshToken = localStorage.getItem('bluey_refresh_token') || '';
      try {
        if (previousRefreshToken) {
          await refreshAccountToken();
        }
      } catch {
        // Sign-out should always clear the local browser, even if refresh is
        // unavailable during a deploy or network blip.
      }
      const refreshToken = localStorage.getItem('bluey_refresh_token') || previousRefreshToken;
      if (!accountToken() && !refreshToken) return;
      try {
        await apiJson('/auth/logout', {
          method: 'POST',
          body: JSON.stringify({ refresh_token: refreshToken || null }),
          skipAuthRefresh: true,
        });
      } catch {
        // Always let local sign-out complete. Stale or already-revoked server
        // tokens are harmless once the browser copy is cleared.
      }
    }

    async function signOut() {
      await revokeBrowserSession();
      clearAccountToken();
      if (isAccountRoute) {
        loadAccount().catch((error) => accountMessage(error.message));
      }
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

    function closeProfileMenu() {
      const button = document.getElementById('accountProfileButton');
      const menu = document.getElementById('accountProfileMenu');
      if (button) button.setAttribute('aria-expanded', 'false');
      if (menu) menu.hidden = true;
    }

    function toggleProfileMenu() {
      const button = document.getElementById('accountProfileButton');
      const menu = document.getElementById('accountProfileMenu');
      if (!button || !menu) return;
      const open = menu.hidden;
      menu.hidden = !open;
      button.setAttribute('aria-expanded', open ? 'true' : 'false');
    }

    function filePreviewHref(route) {
      return `${window.location.pathname}?route=${encodeURIComponent(normalizeRoutePath(route) || '/')}`;
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
        const refreshToken = localStorage.getItem('bluey_refresh_token') || '';
        if (!refreshToken) return '';
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
        setAccountToken(body);
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

    function accountMessage(text, auth = false) {
      const el = document.getElementById(auth ? 'accountAuthMessage' : 'accountMessage');
      if (el) el.textContent = text || '';
    }

    function requireSignupTerms() {
      const terms = document.getElementById('signupTerms');
      if (!terms || terms.checked) return true;
      accountMessage('Accept Terms and Privacy to create an account.', true);
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
      const terms = document.getElementById('signupTermsLabel');
      const primary = document.getElementById('accountPrimaryButton');
      const create = document.getElementById('createAccountButton');
      const password = document.getElementById('accountPassword');
      if (title) title.textContent = accountAuthMode === 'signup' ? 'Create account' : 'Sign in';
      if (copy) {
        copy.textContent = accountAuthMode === 'signup'
          ? 'Create a Bluey account, verify your email, then the desktop links automatically.'
          : 'Use your Bluey account. If the desktop opened this page, it links automatically after sign in.';
      }
      if (terms) terms.hidden = accountAuthMode !== 'signup';
      if (primary) primary.textContent = accountAuthMode === 'signup' ? 'Send verification code' : 'Sign in';
      if (create) create.textContent = accountAuthMode === 'signup' ? 'Sign in instead' : 'Create account';
      if (password) {
        password.autocomplete = accountAuthMode === 'signup' ? 'new-password' : 'current-password';
        password.placeholder = accountAuthMode === 'signup' ? 'Create a password' : 'Password';
      }
      if (accountAuthMode !== 'signup') {
        setSignupOtpMode(false);
      }
      refreshSignupCaptcha().catch((error) => accountMessage(error.message, true));
      accountMessage('', true);
    }

    function setRailCommand(authed, email = '') {
      const label = document.getElementById('railCommandLabel');
      const copy = document.getElementById('railCommandCopy');
      if (!label || !copy) return;
      if (authed) {
        label.textContent = 'signed in';
        copy.textContent = email
          ? `${email} is ready for credits, saved sessions, and cloud answers. Run bluey on to open the overlay.`
          : 'Account ready for credits, saved sessions, and cloud answers. Run bluey on to open the overlay.';
      } else {
        label.textContent = 'bluey on';
        copy.textContent = 'Start Bluey from Terminal. Sign in only when you need cloud answers, credits, or saved sessions.';
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
        copy.innerHTML = 'Check balance, add credits, view saved sessions, and manage your desktop connection. $30 becomes $30 Bluey credits; Auto Reload is optional.';
      } else {
        title.textContent = 'Try Bluey';
        copy.innerHTML = 'Start from Terminal, sign in when needed, and keep your work available from the overlay.';
      }
    }

    function setSignupOtpMode(enabled, email = '') {
      const label = document.getElementById('signupOtpLabel');
      const otp = document.getElementById('signupOtp');
      const confirm = document.getElementById('confirmSignupButton');
      const create = document.getElementById('createAccountButton');
      if (!label || !otp || !confirm || !create) return;
      pendingSignupEmail = enabled ? email : '';
      label.hidden = !enabled;
      confirm.hidden = !enabled;
      create.textContent = enabled ? 'Resend code' : (accountAuthMode === 'signup' ? 'Sign in instead' : 'Create account');
      if (!enabled) otp.value = '';
    }

    function recoveryMessage(text) {
      const el = document.getElementById('recoveryMessage');
      if (el) el.textContent = text || '';
    }

    function renderDeviceLinkHint() {
      const code = pendingDeviceCode();
      const el = document.getElementById('deviceLinkHint');
      if (!el) return;
      if (!code) {
        el.hidden = true;
        return;
      }
      el.hidden = false;
      el.replaceChildren();
      const title = document.createElement('strong');
      title.textContent = 'Linking desktop Bluey';
      const body = document.createElement('span');
      body.append('Code ');
      const codeEl = document.createElement('code');
      codeEl.textContent = code;
      body.append(codeEl, accountToken()
        ? '. Confirm before this browser links the desktop app.'
        : '. Sign in or create an account here, then confirm the desktop link.');
      el.append(title, body);
      if (accountToken() && sessionStorage.getItem(`bluey_device_approved_${code}`) !== '1') {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'account-button secondary compact';
        button.textContent = 'Connect desktop';
        button.addEventListener('click', () => {
          sessionStorage.setItem(`bluey_device_confirmed_${code}`, '1');
          approvePendingDevice()
            .then(() => loadAccount())
            .catch((error) => accountMessage(`Desktop link failed: ${error.message}`));
        });
        el.append(button);
      }
    }

    async function approvePendingDevice() {
      const code = pendingDeviceCode();
      if (!code || !accountToken()) return false;
      const storageKey = `bluey_device_approved_${code}`;
      if (sessionStorage.getItem(storageKey) === '1') return true;
      if (sessionStorage.getItem(`bluey_device_confirmed_${code}`) !== '1') {
        accountMessage('Confirm the desktop link before connecting this account.');
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
      if (!email || !password) {
        accountMessage('Email and password are required.', true);
        return;
      }
      if (mode === 'signup' && !requireSignupTerms()) return;
      accountMessage(mode === 'signup' ? 'Creating account...' : 'Signing in...', true);
      const auth = await apiJson(`/auth/${mode === 'signup' ? 'signup' : 'login'}`, {
        method: 'POST',
        body: JSON.stringify({ email, password }),
      });
      setAccountToken(auth);
      accountMessage('', true);
      await loadAccount();
    }

    async function startSignupOtp() {
      const email = document.getElementById('accountEmail').value.trim();
      const password = document.getElementById('accountPassword').value;
      if (!email || !password) {
        accountMessage('Email and password are required.', true);
        return;
      }
      if (!requireSignupTerms()) return;
      await loadCaptchaConfig();
      const turnstileToken = signupTurnstilePayload();
      if (captchaConfig.provider === 'turnstile' && captchaConfig.site_key && !turnstileToken) {
        accountMessage('Complete the security check to create an account.', true);
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
      accountMessage(`Code sent to ${result.email || email}. Enter it below to finish creating the account.`, true);
      document.getElementById('signupOtp')?.focus();
    }

    async function confirmSignupOtp() {
      const email = (pendingSignupEmail || document.getElementById('accountEmail').value).trim();
      const otp = document.getElementById('signupOtp').value.trim();
      if (!email || !otp) {
        accountMessage('Enter the 6-digit verification code.', true);
        return;
      }
      if (!requireSignupTerms()) return;
      accountMessage('Verifying code...', true);
      const auth = await apiJson('/auth/signup/confirm', {
        method: 'POST',
        body: JSON.stringify({ email, otp, device_fingerprint: browserTrialDeviceId() }),
      });
      setAccountToken(auth);
      setSignupOtpMode(false);
      accountMessage('', true);
      await loadAccount();
    }

    async function startPasswordReset(email) {
      recoveryMessage('Sending reset email...');
      await apiJson('/auth/password-reset/start', {
        method: 'POST',
        body: JSON.stringify({ email }),
      });
      recoveryMessage('If the account exists, a reset link has been sent.');
    }

    async function confirmPasswordReset(token, newPassword) {
      recoveryMessage('Updating password...');
      await apiJson('/auth/password-reset/confirm', {
        method: 'POST',
        body: JSON.stringify({ token, new_password: newPassword }),
      });
      recoveryMessage('Password updated. You can sign in now.');
    }

    async function deleteAccount() {
      const email = currentAccountEmail || 'this Bluey account';
      if (!window.confirm(`Delete ${email}? This permanently removes the account and cannot be undone.`)) {
        return;
      }
      accountMessage('Deleting account...');
      await apiJson('/account/delete', {
        method: 'POST',
        body: JSON.stringify({}),
      });
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
      document.getElementById('usageHint').textContent = `${money(usage.total_cents_spent)} spent in ${usage.period_days || 7} days.`;
      document.getElementById('tierValue').textContent = usage.tier_label || '--';
      const projectedDays = Math.round(usage.projected_days_remaining || 0);
      document.getElementById('projectionHint').textContent = projectedDays > 0
        ? `~${projectedDays} days at current rate.`
        : 'Projection appears after usage.';
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

    async function setupSquareCard(me) {
      const setup = document.getElementById('squareCardSetup');
      const container = document.getElementById('squareCardContainer');
      if (!setup || !container) return;
      if (!me?.square_application_id || !me?.square_location_id) return;
      const environment = me.square_environment || 'sandbox';
      if (squareCard && squareCardEnvironment === environment) return;
      if (squareCardSetupPromise) return squareCardSetupPromise;

      squareCardSetupPromise = (async () => {
        await loadSquareSdk(environment);
        if (!window.Square?.payments) {
          throw new Error('Square card form is unavailable.');
        }
        const payments = window.Square.payments(me.square_application_id, me.square_location_id);
        container.replaceChildren();
        squareCard = await payments.card();
        squareCardEnvironment = environment;
        await squareCard.attach('#squareCardContainer');
      })().finally(() => {
        squareCardSetupPromise = null;
      });
      return squareCardSetupPromise;
    }

    function renderAutoReload(me) {
      const card = document.getElementById('autoReloadCard');
      const hint = document.getElementById('autoReloadHint');
      const method = document.getElementById('autoReloadMethod');
      const toggle = document.getElementById('autoReloadToggle');
      const setup = document.getElementById('squareCardSetup');
      const saveButton = document.getElementById('saveSquareCardButton');
      const thresholdInput = document.getElementById('autoReloadThreshold');
      const amountInput = document.getElementById('autoReloadAmount');
      const rule = document.getElementById('autoReloadRule');
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
      const canSaveSquareCard = me?.billing_provider === 'square'
        && Boolean(me.square_application_id)
        && Boolean(me.square_location_id);
      const hasSavedMethod = Boolean(me?.auto_topup_available);
      const shouldShowSquareSetup = !hasSavedMethod && canSaveSquareCard;

      card.classList.toggle('is-on', Boolean(me?.auto_topup_enabled));
      toggle.checked = Boolean(me?.auto_topup_enabled);
      toggle.disabled = !hasSavedMethod;
      setup.hidden = !shouldShowSquareSetup;
      if (saveButton) {
        saveButton.disabled = !shouldShowSquareSetup;
        saveButton.textContent = 'Save card';
      }
      if (thresholdInput) thresholdInput.value = centsToDollars(thresholdCents);
      if (amountInput) amountInput.value = centsToDollars(amountCents);
      if (rule) {
        rule.textContent = `Optional backup: below ${threshold}, charge ${amount} and add ${amount} credits after payment succeeds.`;
      }

      if (me?.auto_topup_enabled) {
        hint.textContent = `On. Bluey adds ${amount} credits when your balance drops below ${threshold}.`;
      } else if (hasSavedMethod) {
        hint.textContent = `Off. Turn on only if you want Bluey to add ${amount} credits below ${threshold}.`;
      } else if (shouldShowSquareSetup) {
        hint.textContent = 'Off by default. Save a card only if you want Bluey to reload automatically.';
      } else {
        hint.textContent = me?.auto_topup_unavailable_reason || 'Add credits manually any time. Auto Reload is optional.';
      }

      method.textContent = me?.saved_payment_method_label
        ? `Saved card: ${me.saved_payment_method_label}`
        : shouldShowSquareSetup
          ? 'No saved card yet'
          : '';

      if (shouldShowSquareSetup) {
        setupSquareCard(me).catch((error) => accountMessage(error.message));
      }
    }

    function readAutoReloadSettings() {
      const threshold = dollarsToCents(document.getElementById('autoReloadThreshold')?.value || 10);
      const amount = dollarsToCents(document.getElementById('autoReloadAmount')?.value || 30);
      if (!Number.isFinite(threshold) || !Number.isFinite(amount)) {
        throw new Error('Enter valid Auto Reload dollar amounts.');
      }
      if (amount < AUTO_RELOAD_MIN_CENTS) {
        throw new Error('Auto Reload amount must be at least $15.');
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
      if (!rule) return;
      try {
        const settings = readAutoReloadSettings();
        rule.textContent = `Optional backup: below ${money(settings.auto_topup_threshold_cents)}, charge ${money(settings.auto_topup_amount_cents)} and add ${money(settings.auto_topup_amount_cents)} credits after payment succeeds.`;
      } catch (error) {
        rule.textContent = error.message;
      }
    }

    function readManualReloadCents() {
      const input = document.getElementById('manualReloadAmount');
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

    function updateManualReloadDraftCopy() {
      const rule = document.getElementById('manualReloadRule');
      if (!rule) return;
      try {
        const amount = readManualReloadCents();
        rule.textContent = `${money(amount)} reload gives ${money(amount)} Bluey credits. Checkout opens in a new tab; press Refresh balance here after payment succeeds.`;
      } catch (error) {
        rule.textContent = error.message;
      }
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
        ? `Auto Reload is on. Bluey will add ${money(settings.auto_topup_amount_cents)} when balance drops below ${money(settings.auto_topup_threshold_cents)}.`
        : 'Auto Reload is off. You can add credits manually whenever you need them.');
      return me;
    }

    async function saveSquareCard() {
      const button = document.getElementById('saveSquareCardButton');
      if (!squareCard) {
        throw new Error('Card form is still loading. Try again in a moment.');
      }
      const previous = button?.textContent || 'Save card';
      if (button) {
        button.disabled = true;
        button.textContent = 'Saving...';
      }
      try {
        const result = await squareCard.tokenize();
        if (result.status !== 'OK') {
          const details = (result.errors || []).map((error) => error.message).filter(Boolean).join(' ');
          throw new Error(details || 'Card could not be saved.');
        }
        let me = await apiJson('/billing/square/card', {
          method: 'POST',
          body: JSON.stringify({ source_id: result.token }),
        });
        renderAutoReload(me);
        me = await updateAutoReload(true);
        renderAutoReload(me);
      } finally {
        if (button) {
          button.disabled = false;
          button.textContent = previous;
        }
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

    function deviceIconLabel(kind) {
      const value = String(kind || '').toLowerCase();
      if (value.includes('desktop') || value.includes('device')) return 'Bluey';
      if (value.includes('browser')) return 'Host';
      return 'Host';
    }

    function deviceStatus(device) {
      const lastValue = device.last_used_at || device.created_at || '';
      const lastTime = lastValue ? new Date(String(lastValue).replace(' ', 'T')).getTime() : 0;
      const isRecent = lastTime && Date.now() - lastTime < 1000 * 60 * 60 * 24;
      return isRecent ? 'Active' : 'Linked';
    }

    function renderLinkedDevices(payload, message = '') {
      const list = document.getElementById('linkedDevicesList');
      if (!list) return;
      list.replaceChildren();
      const devices = Array.isArray(payload?.devices) ? payload.devices : [];
      const countLabel = document.getElementById('linkedDeviceCount');
      const removeAllButton = document.getElementById('removeAllDevicesButton');
      if (countLabel) {
        countLabel.textContent = devices.length === 0
          ? 'No host activity'
          : devices.length === 1
          ? '1 active host session'
          : `${devices.length} active host sessions`;
      }
      if (removeAllButton) {
        removeAllButton.disabled = devices.length === 0;
      }
      if (!devices.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = message || 'No host login activity yet. Sign in from Bluey desktop to add this machine.';
        list.append(empty);
        return;
      }

      for (const device of devices) {
        const row = document.createElement('article');
        row.className = 'device-row';

        const body = document.createElement('div');
        body.className = 'device-main';
        const icon = document.createElement('div');
        icon.className = 'device-icon';
        icon.textContent = deviceIconLabel(device.kind);
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
        const lastSeen = device.last_used_at
          ? `Last active ${formatDeviceTime(device.last_used_at)}`
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
      accountMessage(`${label || 'Device'} removed.`);
      return devices;
    }

    async function revokeAllLinkedDevices() {
      const devices = await apiJson('/account/devices', {
        method: 'DELETE',
      });
      renderLinkedDevices(devices);
      accountMessage('Host access removed.');
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
        empty.textContent = 'No saved sessions yet. Keep Bluey on while signed in; sessions sync automatically, then refresh here.';
        list.append(empty);
        return;
      }

      for (const session of sessions) {
        const row = document.createElement('article');
        row.className = 'session-row';

        const body = document.createElement('div');
        const title = document.createElement('strong');
        title.className = 'session-title';
        title.textContent = session.title || `Session ${shortSessionId(session.session_id)}`;
        const meta = document.createElement('span');
        meta.className = 'session-meta';
        meta.textContent = [
          session.status || 'saved',
          `${session.transcript_count || 0} transcript`,
          `${session.response_count || 0} answer(s)`,
          `${session.context_count || 0} context`,
          formatSessionTime(session.updated_at_ms || session.last_active_at_ms),
        ].join(' - ');
        body.append(title, meta);

        const button = document.createElement('button');
        button.className = 'account-button ghost';
        button.type = 'button';
        button.dataset.sessionId = session.session_id || '';
        button.textContent = 'View';
        row.append(body, button);
        list.append(row);
      }
    }

    function renderCloudSessionsLoading() {
      const list = document.getElementById('cloudSessionsList');
      if (!list) return;
      list.replaceChildren();
      const loading = document.createElement('div');
      loading.className = 'session-empty';
      loading.textContent = 'Loading saved sessions...';
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

    function latestRecord(records) {
      return Array.isArray(records) && records.length ? records[records.length - 1] : null;
    }

    function appendBundlePreview(detail, label, text) {
      if (!text) return;
      const pre = document.createElement('pre');
      pre.textContent = `${label}\n${text}`;
      detail.append(pre);
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

      const title = document.createElement('strong');
      title.textContent = bundle.session?.title || `Session ${shortSessionId(sessionId)}`;
      const meta = document.createElement('span');
      meta.textContent = [
        `${bundle.transcript_segments?.length || 0} transcript segment(s)`,
        `${bundle.cue_responses?.length || 0} answer(s)`,
        `${bundle.context_artifacts?.length || 0} context item(s)`,
      ].join(' - ');
      detail.append(title, meta);

      if (bundle.session?.answer_style) {
        appendBundlePreview(detail, 'Answer style', bundle.session.answer_style);
      }
      const transcript = latestRecord(bundle.transcript_segments);
      if (transcript?.text) {
        appendBundlePreview(detail, `Latest transcript (${transcript.speaker || 'speaker'})`, transcript.text);
      }
      const answer = latestRecord(bundle.cue_responses);
      if (answer?.text) {
        appendBundlePreview(detail, 'Latest answer', answer.text);
      }
      if (Array.isArray(bundle.context_artifacts) && bundle.context_artifacts.length) {
        appendBundlePreview(
          detail,
          'Attached context',
          bundle.context_artifacts
            .slice(-5)
            .map((artifact) => `${artifact.kind || 'context'}: ${artifact.title || artifact.artifact_id}`)
            .join('\n')
        );
      }
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
      document.getElementById('accountSignOut').hidden = true;
      if (!authed) return;
      const refreshButton = document.getElementById('refreshAccountButton');
      if (refreshButton) refreshButton.disabled = true;
      accountMessage('Loading account...');
      renderCloudSessionsLoading();
      const linkedDevicesPromise = loadLinkedDevices().catch((error) => {
        renderLinkedDevices({ devices: [] }, `Could not load host activity: ${error.message}`);
        return null;
      });
      const sessionsPromise = loadCloudSessions().then((sessions) => {
        setCloudSessionDetail('');
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
        document.getElementById('accountEmailLabel').textContent = me.email || 'Bluey account';
        const profileEmailLabel = document.getElementById('profileEmailLabel');
        if (profileEmailLabel) profileEmailLabel.textContent = me.email || 'Bluey account';
        setRailCommand(true, me.email || '');
        const balanceValue = document.getElementById('balanceValue');
        const balanceCard = balanceValue?.closest('.balance-kpi');
        if (balanceValue) {
          balanceValue.textContent = money(me.balance_cents);
          balanceCard?.classList.toggle('balance-critical', me.balance_cents < 500);
          balanceCard?.classList.toggle('balance-low', me.balance_cents >= 500 && me.balance_cents < 1000);
        }
        document.getElementById('balanceHint').textContent = me.trial_seconds_remaining > 0
          ? `${Math.round(me.trial_seconds_remaining / 60)} trial minutes left. Credits are used after the trial when paid cloud work is needed.`
          : 'When balance reaches $0, paid cloud work pauses until you add credits.';
        renderUsage(usage);
        renderAutoReload(me);
        updateManualReloadDraftCopy();
        const adminSection = document.getElementById('adminAbuseSection');
        if (adminSection) adminSection.hidden = !me.is_admin;
        if (me.is_admin) {
          loadAdminAbuse().catch((error) => {
            renderAdminAbuse({}, `Could not load trial abuse events: ${error.message}`);
          });
        }
        void linkedDevicesPromise;
        void sessionsPromise;
        accountMessage(new URLSearchParams(location.search).get('reload') === 'success'
          ? 'Credits added. If the balance still looks old, checkout is finishing; press Refresh balance in a moment.'
          : '');
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
        accountMessage(error.message, true);
        return;
      }
      accountMessage(`Opening ${money(amountCents)} checkout...`);
      try {
        const checkout = await apiJson('/billing/checkout', {
          method: 'POST',
          body: JSON.stringify({ amount_cents: amountCents }),
        });
        const opened = window.open(checkout.checkout_url, '_blank', 'noopener,noreferrer');
        if (!opened) {
          accountMessage('Checkout was blocked by the browser. Allow popups for bluey.sh, then click Add credits again.');
          return;
        }
        accountMessage('Checkout opened in a new tab. Complete payment there, then return here and press Refresh balance.');
      } catch (error) {
        accountMessage(`${error.message}. Billing may not be fully configured yet.`);
      }
    }

    function initAccountApp() {
      if (isPolicyRoute) {
        productSite.hidden = true;
        accountApp.hidden = true;
        downloadApp.hidden = true;
        policyApp.hidden = false;
        document.getElementById('privacyPolicy').hidden = currentPath !== '/docs/privacy';
        document.getElementById('termsPolicy').hidden = currentPath !== '/docs/terms';
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

      document.getElementById('accountForm').addEventListener('submit', (event) => {
        event.preventDefault();
        if (accountAuthMode === 'signup') {
          startSignupOtp().catch((error) => accountMessage(error.message, true));
        } else {
          accountAuth('login').catch((error) => accountMessage(error.message, true));
        }
      });
      document.getElementById('createAccountButton').addEventListener('click', () => {
        if (pendingSignupEmail) {
          startSignupOtp().catch((error) => accountMessage(error.message, true));
          return;
        }
        if (accountAuthMode === 'signup') {
          setAccountAuthMode('login');
        } else {
          setAccountAuthMode('signup');
          document.getElementById('accountEmail')?.focus();
        }
      });
      document.getElementById('confirmSignupButton').addEventListener('click', () => {
        confirmSignupOtp().catch((error) => accountMessage(error.message, true));
      });
      document.getElementById('reloadButton').addEventListener('click', () => {
        startReload();
      });
      document.getElementById('refreshAccountButton')?.addEventListener('click', () => {
        loadAccount().catch((error) => accountMessage(error.message));
      });
      document.getElementById('manualReloadAmount')?.addEventListener('input', () => {
        updateManualReloadDraftCopy();
      });
      document.getElementById('autoReloadToggle')?.addEventListener('change', (event) => {
        const enabled = Boolean(event.target.checked);
        event.target.disabled = true;
        updateAutoReload(enabled)
          .catch((error) => {
            accountMessage(error.message);
            event.target.checked = !enabled;
          })
          .finally(() => {
            loadAccount().catch((error) => accountMessage(error.message));
          });
      });
      ['autoReloadThreshold', 'autoReloadAmount'].forEach((id) => {
        document.getElementById(id)?.addEventListener('change', () => {
          const toggle = document.getElementById('autoReloadToggle');
          if (!toggle?.checked) {
            updateAutoReloadDraftCopy();
            return;
          }
          updateAutoReload(true)
            .catch((error) => accountMessage(error.message))
            .finally(() => loadAccount().catch((error) => accountMessage(error.message)));
        });
      });
      document.getElementById('saveSquareCardButton')?.addEventListener('click', () => {
        saveSquareCard().catch((error) => accountMessage(error.message));
      });
      document.getElementById('refreshDevicesButton')?.addEventListener('click', () => {
        renderLinkedDevices({ devices: [] }, 'Loading host activity...');
        loadLinkedDevices().catch((error) => {
          renderLinkedDevices({ devices: [] }, `Could not load host activity: ${error.message}`);
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
        if (!window.confirm('Remove all host access for this account?')) return;
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
      document.getElementById('accountProfileButton')?.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
        toggleProfileMenu();
      });
      document.getElementById('accountProfileMenu')?.addEventListener('click', (event) => {
        event.stopPropagation();
      });
      document.getElementById('deleteAccountButton')?.addEventListener('click', () => {
        closeProfileMenu();
        deleteAccount().catch((error) => accountMessage(error.message));
      });
      document.getElementById('refreshSessionsButton').addEventListener('click', () => {
        setCloudSessionDetail('');
        loadCloudSessions().catch((error) => setCloudSessionDetail(`Could not load saved sessions: ${error.message}`));
      });
      document.getElementById('cloudSessionsList').addEventListener('click', (event) => {
        const button = event.target.closest('button[data-session-id]');
        if (!button) return;
        loadCloudSessionDetail(button.dataset.sessionId).catch((error) => {
          setCloudSessionDetail(`Could not load saved session: ${error.message}`);
        });
      });
      document.getElementById('passwordResetStartForm').addEventListener('submit', (event) => {
        event.preventDefault();
        const email = document.getElementById('resetEmail').value.trim();
        if (!email) return recoveryMessage('Email is required.');
        startPasswordReset(email).catch((error) => recoveryMessage(error.message));
      });
      document.getElementById('passwordResetConfirmForm').addEventListener('submit', (event) => {
        event.preventDefault();
        const token = new URLSearchParams(location.search).get('token') || '';
        const password = document.getElementById('resetPassword').value;
        if (!token) return recoveryMessage('Reset token is missing.');
        if (!password) return recoveryMessage('New password is required.');
        confirmPasswordReset(token, password).catch((error) => recoveryMessage(error.message));
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
      document.getElementById('accountSignOut').hidden = true;
      closeProfileMenu();

      const params = new URLSearchParams(location.search);
      const token = params.get('token') || '';
      const isVerify = currentPath === '/verify-email';
      document.getElementById('recoveryTitle').textContent = isVerify ? 'Verify email' : 'Reset password';
      document.getElementById('recoveryCopy').textContent = isVerify
        ? 'Bluey will verify this email token and return you to sign-in.'
        : token
          ? 'Choose a new password for this Bluey account.'
          : 'Enter your account email and Bluey will send a password reset link.';
      document.getElementById('passwordResetStartForm').hidden = isVerify || Boolean(token);
      document.getElementById('passwordResetConfirmForm').hidden = isVerify || !token;

      if (isVerify) {
        if (!token) {
          recoveryMessage('Verification token is missing.');
        } else {
          confirmEmailVerification(token).catch((error) => recoveryMessage(error.message));
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
    }

    document.addEventListener('click', async (event) => {
      if (!event.target.closest('#accountProfile')) {
        closeProfileMenu();
      }

      const signOutButton = event.target.closest('[data-sign-out]');
      if (signOutButton) {
        event.preventDefault();
        closeProfileMenu();
        await signOut();
        return;
      }

      const button = event.target.closest('[data-copy]');
      if (!button) return;
      const text = button.dataset.copy || '';
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
        const old = button.textContent;
        button.textContent = 'Copied';
        setTimeout(() => {
          button.textContent = old;
        }, 1200);
      } catch {
        button.textContent = 'Copy failed';
      }
    });

    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        closeProfileMenu();
      }
    });

    initFilePreview();
    bootBlueyTerminal();
    syncAccountNav();
    initDownloadInstructions();
    initAccountApp();
}
