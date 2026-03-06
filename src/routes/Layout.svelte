<script lang="ts">
	import 'tippy.js/dist/tippy.css';
	import '../app.css';
	import '../tailwind.css';

	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getBackendConfig } from '$lib/apis';
	import { getSessionUser } from '$lib/apis/auths';
	import reopenMainWindow from '$lib/app/actions/reopen-main-window';
	import { MAIN_WINDOW_LABEL, OPEN_IN_MAIN_WINDOW } from '$lib/app/constants';
	import Draggable from '$lib/components/desktop-app/Draggable.svelte';
	import i18n, { getLanguages, initI18n } from '$lib/i18n';
	import {
		activeUserCount,
		appConfig,
		appState,
		config,
		mobile,
		socket,
		theme,
		USAGE_POOL,
		user,
		WEBUI_BASE_URL,
		WEBUI_NAME
	} from '$lib/stores';
	import { bestMatchingLanguage, delay } from '$lib/utils';
	import { invoke } from '@tauri-apps/api/core';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { unregisterAll } from '@tauri-apps/plugin-global-shortcut';
	import { io } from 'socket.io-client';
	import { onMount, setContext, tick } from 'svelte';
	import { Toaster } from 'svelte-sonner';
	import { spring } from 'svelte/motion';

	let loadingProgress = spring(0, {
		stiffness: 0.05
	});

	// Initialize i18n
	setContext('i18n', i18n);

	let loaded = false;
	const IS_MAIN_WINDOW = getCurrentWindow().label === MAIN_WINDOW_LABEL;
	const BREAKPOINT = 768;

	$: console.log('Loaded changed', loaded);
	$: console.log('WEBUI_BASE_URL changed', $WEBUI_BASE_URL);

	/**
	 * Show a branded splash overlay before page reloads.
	 * Creates a fixed overlay matching the splash screen in app.html so the
	 * user sees a consistent loading experience instead of a blank flash.
	 */
	const showSplashOverlay = () => {
		// If the original splash-screen still exists, just show it
		const existing = document.getElementById('splash-screen');
		if (existing) {
			existing.style.display = 'flex';
			return;
		}
		// Otherwise, create a temporary overlay that matches the splash screen
		const isDark = document.documentElement.classList.contains('dark') ||
			(!document.documentElement.classList.contains('light') &&
				window.matchMedia('(prefers-color-scheme: dark)').matches);
		const overlay = document.createElement('div');
		overlay.id = 'splash-overlay';
		overlay.style.cssText = `
			position: fixed; z-index: 9999; top: 0; left: 0; width: 100%; height: 100%;
			display: flex; flex-direction: column; align-items: center; justify-content: center;
			background: ${isDark ? '#111' : '#fff'};
		`;
		overlay.innerHTML = `
			<img style="width:auto;height:5rem;${isDark ? 'filter:invert(1);' : ''}" src="/static/splash.png" />
			<div style="margin-top:1rem;font-size:1.5rem;font-weight:700;letter-spacing:0.15em;
				color:${isDark ? '#f0f0f0' : '#171717'};
				font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;">IBEX</div>
			<div style="margin-top:0.35rem;font-size:0.8rem;font-weight:400;letter-spacing:0.05em;
				color:#888;
				font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;">
				Integration Bridge for EXtended systems</div>
			<div style="margin-top:2rem;width:1.5rem;height:1.5rem;
				border:2px solid ${isDark ? '#333' : '#e5e7eb'};
				border-top-color:#3b82f6;border-radius:50%;
				animation:splash-spin 0.8s linear infinite;"></div>
		`;
		document.body.appendChild(overlay);
	};

	const setupSocket = () => {
		const _socket = io(`${$WEBUI_BASE_URL}` || undefined, {
			reconnection: true,
			reconnectionDelay: 1000,
			reconnectionDelayMax: 5000,
			randomizationFactor: 0.5,
			path: '/ws/socket.io',
			auth: { token: localStorage.token }
		});

		socket.set(_socket);

		_socket.on('connect_error', (err) => {
			console.log('connect_error', err);
		});

		_socket.on('connect', () => {
			console.log('connected', _socket.id);
		});

		_socket.on('reconnect_attempt', (attempt) => {
			console.log('reconnect_attempt', attempt);
		});

		_socket.on('reconnect_failed', () => {
			console.log('reconnect_failed');
		});

		_socket.on('disconnect', (reason, details) => {
			console.log(`Socket ${_socket.id} disconnected due to ${reason}`);
			if (details) {
				console.log('Additional details:', details);
			}
		});

		_socket.on('user-count', (data) => {
			console.log('user-count', data);
			activeUserCount.set(data.count);
		});

		_socket.on('usage', (data) => {
			console.log('usage', data);
			USAGE_POOL.set(data['models']);
		});
	};

	onMount(() => {
		console.log('Layout onMount called');
		const onResize = () => {
			if (window.innerWidth < BREAKPOINT) {
				mobile.set(true);
			} else {
				mobile.set(false);
			}
		};

		let unlistenReopen: UnlistenFn;
		let unlistenOpenInMainWindow: UnlistenFn;
		let unlistenStartupComplete: UnlistenFn;
		let unlistenServersRestarted: UnlistenFn;
		(async () => {
			console.log('Waiting 100ms for cross window stores to load...');
			await delay(100);
			console.log('They should be loaded now!');

			/////////////////////////////////
			// INITIALIZE APP STATE
			/////////////////////////////////

			// Reopen main window event listener
			unlistenReopen = await listen('reopen', async () => {
				await reopenMainWindow();
			});

			// IBEX: Listen for backend startup completion.
			// When the Tauri backend finishes auth, it injects the ibex-admin JWT
			// into localStorage (after clearing any stale token). We ALWAYS
			// re-authenticate here — the previous $user may have been from a
			// stale token (e.g., the default "User" account created by /auth
			// auto-signin in a previous session). The ibex-admin account has the
			// system prompt, MCP tool config, and admin role.
			unlistenStartupComplete = await listen('startup-complete', async () => {
				console.log('IBEX startup complete — re-authenticating with ibex-admin JWT');
				if (localStorage.token && $WEBUI_BASE_URL) {
					// Disconnect old socket (may have been connected with stale/wrong token)
					if ($socket) {
						$socket.disconnect();
					}
					// Setup socket with fresh ibex-admin token
					setupSocket();

					// ALWAYS re-fetch session user with the ibex-admin JWT.
					// Do NOT skip if $user is already set — it may be the wrong user.
					const sessionUser = await getSessionUser(localStorage.token).catch((err) => {
						console.error('IBEX: Failed to get session user:', err);
						return null;
					});
					if (sessionUser) {
						console.log('IBEX: Authenticated as', sessionUser.name, '(role:', sessionUser.role + ')');
						$user = sessionUser;
						$config = await getBackendConfig();

						// Navigate to main page if on auth page
						if (page.url.pathname === '/auth') {
							await goto('/');
						}
					}
				}
			});

			// IBEX: Listen for server restarts (settings save, tray menu).
			// restart_servers() may recreate the Docker container, push updated
			// tool connections, and re-authenticate — all of which change
			// server-side state. The simplest way to pick up ALL changes
			// (tools, models, config) is to inject the fresh JWT and reload.
			unlistenServersRestarted = await listen('servers-restarted', async () => {
				// Don't reload when the setup wizard or settings page is active.
				// These pages manage their own flow (e.g., the setup wizard calls
				// restart_servers and then navigates to goToChat itself). Reloading
				// here would interrupt that flow and loop back to /setup.
				if (page.url.pathname === '/setup' || page.url.pathname === '/settings') {
					console.log('IBEX: servers-restarted on', page.url.pathname, '— skipping reload');
					return;
				}

				console.log('IBEX: servers-restarted — refreshing page with updated config');
				try {
					const jwt: string | null = await invoke('get_jwt_token');
					if (jwt) {
						localStorage.setItem('token', jwt);
					}
				} catch (e) {
					console.warn('IBEX: Failed to get JWT after restart:', e);
				}
				// Show the branded splash screen BEFORE reloading so the user sees
				// a consistent loading experience instead of a blank/flashing screen.
				showSplashOverlay();
				// Brief delay for container + MCP servers to stabilize
				await delay(1000);
				window.location.reload();
			});

			//
			unlistenOpenInMainWindow = await listen(
				OPEN_IN_MAIN_WINDOW,
				async (event: { payload: { chatId: string } }) => {
					if (!event.payload || !event.payload.chatId) {
						console.warn('open in main window called without chatId');
						return;
					}
					await goto(`/c/${event.payload.chatId}`);
					console.log('Chat', event.payload.chatId, 'opened in main window');
					await getCurrentWindow().setFocus();
				}
			);

			console.log('Initial app state:', $appState, $appConfig);

			theme.set(localStorage.theme);

			mobile.set(window.innerWidth < BREAKPOINT);

			window.addEventListener('resize', onResize);

			// Check if the setup wizard should be shown.
			//
			// The backend's needs_setup command is authoritative — it checks if
			// any connectors are configured in ~/.ibex-mcp.env + Keychain.
			// WEBUI_BASE_URL (from Tauri stores.json) is a secondary signal:
			// it may be empty after reinstall or store reset even when connectors
			// are fully configured.
			console.log('WEBUI_BASE_URL before setup check:', $WEBUI_BASE_URL);
			let setupRequired = false;
			try {
				setupRequired = await invoke('needs_setup');
				console.log('needs_setup returned:', setupRequired);
			} catch (e) {
				console.warn('needs_setup check failed, falling back to WEBUI_BASE_URL:', e);
				setupRequired = $WEBUI_BASE_URL === '';
			}

			if (setupRequired) {
				console.log('Setup required', page.url.pathname);
				if (page.url.pathname !== '/setup') {
					console.log('Redirecting to /setup');
					await goto('/setup');
				}
				// Must set loaded=true so the <slot/> becomes visible.
				// Without this, stale stores.json (webui_base_url already set)
				// keeps the loading screen visible and the setup page never
				// renders — the user gets stuck on an infinite spinner.
				document.getElementById('splash-screen')?.remove();
				loaded = true;
				return;
			}

			// If WEBUI_BASE_URL is empty but setup is NOT required (connectors
			// exist), set it to the default. This handles reinstall or store
			// reset where stores.json lost the value.
			if ($WEBUI_BASE_URL === '') {
				console.log('WEBUI_BASE_URL empty but setup not required — setting default');
				WEBUI_BASE_URL.set('http://localhost:8080');
				await delay(50);
			}

			let backendConfig = null;
			try {
				backendConfig = await getBackendConfig();
				console.log('Backend config:', backendConfig);
			} catch (error) {
				console.error('Error loading backend config:', error);
			}
			// Initialize i18n even if we didn't get a backend config,
			// so `/error` can show something that's not `undefined`.

			initI18n();
			if (!localStorage.locale) {
				const languages = await getLanguages();
				const browserLanguages = navigator.languages
					? navigator.languages
					: // @ts-expect-error Compatibility with older Internet Explorer browsers
						[navigator.language || navigator.userLanguage];
				const lang = backendConfig?.default_locale
					? backendConfig.default_locale
					: bestMatchingLanguage(languages, browserLanguages, 'en-US');
				$i18n.changeLanguage(lang);
			}

			if (backendConfig) {
				// Save Backend Status to Store
				$config = backendConfig;
				$WEBUI_NAME = backendConfig.name;

				// IBEX: Settings and Setup pages render independently of auth
				const isIbexPage = page.url.pathname === '/settings' || page.url.pathname === '/setup';

				if ($config && !isIbexPage) {
					if (localStorage.token) {
						console.log('Token:', localStorage.token);

						// Setup socket only when we have a valid token.
						// Previously setupSocket() ran before the token check, causing
						// the socket to connect without auth → $socket.id undefined →
						// chat API sends session_id: undefined → backend rejects.
						setupSocket();

						// Get Session User Info
						const sessionUser = await getSessionUser(localStorage.token).catch((error) => {
							console.error(error);
							return null;
						});

						if (sessionUser) {
							// Save Session User to Store
							$user = sessionUser;
							$config = await getBackendConfig();
						} else {
							// Token is invalid — remove it and wait for Tauri backend
							// to inject a fresh JWT via the startup-complete event.
							localStorage.removeItem('token');
							console.log('IBEX: Invalid session token removed — waiting for backend auth');
						}
					} else {
						// IBEX: No token yet — the Tauri backend injects the JWT into
						// localStorage after authenticating with Open WebUI. The
						// startup-complete event listener (registered above) will
						// re-setup the socket and authenticate the session.
						// Do NOT redirect to /auth:
						// 1. That creates a DIFFERENT user (not ibex-admin) via auto-signin
						// 2. window.location.href destroys event listeners (full page reload)
						console.log('IBEX: No token in localStorage — waiting for Tauri backend to inject JWT');
					}
				}
			} else if (page.url.pathname === '/settings' || page.url.pathname === '/setup') {
				// IBEX: Settings/Setup can render even without backend
				console.log('IBEX page loading without backend config');
			} else {
				// Redirect to /error when Backend Not Detected
				await goto(`/error`);
			}

			await tick();

			if (
				document.documentElement.classList.contains('her') &&
				document.getElementById('progress-bar')
			) {
				loadingProgress.subscribe((value) => {
					const progressBar = document.getElementById('progress-bar');

					if (progressBar) {
						progressBar.style.width = `${value}%`;
					}
				});

				await loadingProgress.set(100);

				document.getElementById('splash-screen')?.remove();

				const audio = new Audio(`/audio/greeting.mp3`);
				const playAudio = () => {
					audio.play();
					document.removeEventListener('click', playAudio);
				};

				document.addEventListener('click', playAudio);

				loaded = true;
			} else {
				document.getElementById('splash-screen')?.remove();
				loaded = true;
			}
		})();

		return async () => {
			window.removeEventListener('resize', onResize);

			// Unregister all global shortcuts
			await unregisterAll();

			// Unlisten to Reopen event
			unlistenReopen();

			// Unlisten to Open in Main Window event
			unlistenOpenInMainWindow();

			// Unlisten to startup complete event
			if (unlistenStartupComplete) unlistenStartupComplete();

			// Unlisten to servers restarted event
			if (unlistenServersRestarted) unlistenServersRestarted();
		};
	});
</script>

<svelte:head>
	<title>{$WEBUI_NAME}</title>
</svelte:head>
<Draggable />
{#if loaded || $WEBUI_BASE_URL === ''}
	<slot />
{:else}
	<!-- Branded loading screen while connecting to backend after setup -->
	<div class="fixed inset-0 flex flex-col items-center justify-center bg-white dark:bg-gray-900 z-50">
		<img src="/static/splash.png" class="h-20 w-auto dark:invert" alt="IBEX" />
		<div class="mt-4 text-lg font-bold tracking-widest text-gray-800 dark:text-gray-100" style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; letter-spacing: 0.15em;">IBEX</div>
		<div class="mt-1 text-xs tracking-wide text-gray-400 dark:text-gray-500" style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;">Integration Bridge for EXtended systems</div>
		<div class="mt-8 w-6 h-6 border-2 border-gray-200 dark:border-gray-700 rounded-full animate-spin" style="border-top-color: #3b82f6;"></div>
	</div>
{/if}
{#if IS_MAIN_WINDOW}
	<Toaster
		theme={$theme.includes('dark')
			? 'dark'
			: $theme === 'system'
				? window.matchMedia('(prefers-color-scheme: dark)').matches
					? 'dark'
					: 'light'
				: 'light'}
		richColors
		position="top-center"
	/>
{/if}
