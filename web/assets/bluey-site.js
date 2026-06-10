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

    function money(cents) {
      return `$${(Number(cents || 0) / 100).toFixed(2)}`;
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
      syncAccountNav();
    }

    function signOut() {
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

    async function apiJson(path, options = {}) {
      const headers = new Headers(options.headers || {});
      headers.set('Content-Type', 'application/json');
      const token = accountToken();
      if (token) headers.set('Authorization', `Bearer ${token}`);
      const response = await fetch(path, { ...options, headers });
      let body = null;
      try {
        body = await response.json();
      } catch {
        body = null;
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
        copy.innerHTML = 'Manage credits, saved sessions, and your desktop connection. Credits are used only when Bluey uses cloud help.';
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
      body.append(codeEl, '. Sign in or create an account here; Bluey will connect automatically.');
      el.append(title, body);
    }

    async function approvePendingDevice() {
      const code = pendingDeviceCode();
      if (!code || !accountToken()) return false;
      const storageKey = `bluey_device_approved_${code}`;
      if (sessionStorage.getItem(storageKey) === '1') return true;
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
      accountMessage('Sending verification code...', true);
      const result = await apiJson('/auth/signup/start', {
        method: 'POST',
        body: JSON.stringify({ email, password }),
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
        body: JSON.stringify({ email, otp }),
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
      if (!rows.length) {
        list.innerHTML = '<div class="usage-row"><span>No paid requests yet</span><span>Ready</span><span>$0.00</span></div>';
        return;
      }
      list.innerHTML = rows.map((entry) => `
        <div class="usage-row">
          <span>${entry.task_type || 'general'}</span>
          <span>${entry.count || 0} cue${entry.count === 1 ? '' : 's'}</span>
          <span>${money(entry.cost_cents)}</span>
        </div>
      `).join('');
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

    function renderCloudSessions(payload) {
      const list = document.getElementById('cloudSessionsList');
      if (!list) return;
      list.replaceChildren();
      const sessions = Array.isArray(payload?.sessions) ? payload.sessions : [];
      if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'session-empty';
        empty.textContent = 'No saved sessions yet. Run bluey cloud sync after a local session, then refresh here.';
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

    async function loadAccount() {
      if (currentPath === '/verify-email' || currentPath === '/password-reset') {
        renderRecoveryRoute();
        return;
      }
      const authed = Boolean(accountToken());
      setAccountChrome(authed);
      document.getElementById('accountAuthCard').hidden = authed;
      document.getElementById('accountPreviewCard').hidden = true;
      document.getElementById('accountDashboard').hidden = !authed;
      document.getElementById('accountRecoveryCard').hidden = true;
      document.getElementById('accountSignOut').hidden = !authed;
      if (!authed) return;
      accountMessage('Loading account...');
      const [me, usage, sessions] = await Promise.all([
        apiJson('/account/me'),
        apiJson('/account/usage'),
        loadCloudSessions().catch((error) => {
          renderCloudSessions({ sessions: [] });
          setCloudSessionDetail(`Could not load saved sessions: ${error.message}`);
          return null;
        }),
      ]);
      document.getElementById('accountEmailLabel').textContent = me.email || 'Bluey account';
      setRailCommand(true, me.email || '');
      document.getElementById('balanceValue').textContent = money(me.balance_cents);
      document.getElementById('balanceHint').textContent = me.trial_seconds_remaining > 0
        ? `${Math.round(me.trial_seconds_remaining / 60)} trial minutes remaining.`
        : 'Paid cloud requests stop at $0. Add credits when ready.';
      renderUsage(usage);
      if (sessions) setCloudSessionDetail('');
      accountMessage(new URLSearchParams(location.search).get('reload') === 'success'
        ? 'Credits added. If the balance has not updated yet, checkout is still finishing.'
        : '');
      try {
        const approved = await approvePendingDevice();
        if (!approved) await openDesktopDeepLinkIfNeeded();
      } catch (error) {
        accountMessage(`Account signed in, but desktop handoff failed: ${error.message}`);
      }
    }

    async function startReload() {
      accountMessage('Opening secure checkout...');
      try {
        const checkout = await apiJson('/billing/checkout', {
          method: 'POST',
          body: JSON.stringify({ amount_cents: 3000 }),
        });
        window.location.href = checkout.checkout_url;
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

      const params = new URLSearchParams(location.search);
      if (params.get('access_token')) {
        setAccountToken({
          access_token: params.get('access_token'),
          refresh_token: params.get('refresh_token') || '',
        });
        history.replaceState(null, '', location.pathname);
      }

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
        clearAccountToken();
        accountMessage('Session expired. Sign in again to reconnect Bluey.', true);
        loadAccount();
      });
    }

    function renderRecoveryRoute() {
      setAccountChrome(false);
      document.getElementById('accountAuthCard').hidden = true;
      document.getElementById('accountPreviewCard').hidden = true;
      document.getElementById('accountDashboard').hidden = true;
      document.getElementById('accountRecoveryCard').hidden = false;
      document.getElementById('accountSignOut').hidden = true;

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
      const signOutButton = event.target.closest('[data-sign-out]');
      if (signOutButton) {
        event.preventDefault();
        signOut();
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

    initFilePreview();
    bootBlueyTerminal();
    syncAccountNav();
    initDownloadInstructions();
    initAccountApp();
}
