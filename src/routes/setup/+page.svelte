<script lang="ts">
	import { goto } from '$app/navigation';
	import { APP_STORE_FILE } from '$lib/app/constants';
	import { WEBUI_BASE_URL } from '$lib/stores';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getStore } from '@tauri-apps/plugin-store';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { onMount } from 'svelte';

	// Detect if opened from tray menu (re-run) vs first-time setup
	let isRerun = false;

	// ── Step Flow ──
	type Step = 'welcome' | 'dependencies' | 'slack' | 'notion' | 'jira' | 'servicenow' | 'salesforce' | 'memory' | 'launching' | 'done';

	const allSteps: Step[] = ['welcome', 'dependencies', 'slack', 'notion', 'jira', 'servicenow', 'salesforce', 'memory', 'launching', 'done'];

	let currentStep: Step = 'welcome';

	function nextStep(s: Step): Step {
		const idx = allSteps.indexOf(s);
		return idx < allSteps.length - 1 ? allSteps[idx + 1] : s;
	}

	function prevStep(s: Step): Step {
		const idx = allSteps.indexOf(s);
		return idx > 0 ? allSteps[idx - 1] : s;
	}

	// ── State ──
	let statusMessage = '';
	let errorMessage = '';
	let dockerOk = false;
	let vpnOk = false;
	let vpnChecking = true;
	let launchProgress = 0;

	// Connector config
	let config: any = {};

	// Track configure vs skip for each connector
	let connectorChoice: Record<string, 'pending' | 'configure' | 'skip'> = {
		slack: 'pending',
		notion: 'pending',
		jira: 'pending',
		servicenow: 'pending',
		salesforce: 'pending',
		memory: 'pending'
	};

	// ── Connector Definitions ──
	type ConnectorKey = 'slack' | 'notion' | 'jira' | 'servicenow' | 'salesforce' | 'memory';

	const connectorSteps: Array<{
		key: ConnectorKey;
		name: string;
		emoji: string;
		description: string;
		instructions: string;
		helpUrl: string;
		helpLabel: string;
		fields: Array<{ key: string; label: string; type: string; placeholder: string; suffix?: string }>;
	}> = [
		{
			key: 'slack',
			name: 'Slack',
			emoji: '💬',
			description: 'Search messages, read channels, and browse threads in your Slack workspace.',
			instructions: 'Create a Slack app at api.slack.com/apps, then go to OAuth & Permissions. Add these User Token Scopes: channels:read, channels:history, groups:read, groups:history, search:read, users:read. Then click "Install to Workspace" and copy the User OAuth Token (starts with xoxp-).',
			helpUrl: 'https://api.slack.com/apps',
			helpLabel: 'Open Slack Apps',
			fields: [
				{ key: 'slack_token', label: 'User OAuth Token', type: 'password', placeholder: 'xoxp-...' }
			]
		},
		{
			key: 'notion',
			name: 'Notion',
			emoji: '📝',
			description: 'Search pages, read content, and query databases in your Notion workspace.',
			instructions: 'Go to Notion Integrations and create a new internal integration. Copy the Internal Integration Secret. Then share your pages with the integration via each page\'s ••• menu > Connections.',
			helpUrl: 'https://www.notion.so/profile/integrations',
			helpLabel: 'Open Notion Integrations',
			fields: [
				{ key: 'notion_token', label: 'Integration Token', type: 'password', placeholder: 'ntn_...' }
			]
		},
		{
			key: 'jira',
			name: 'Jira',
			emoji: '🎫',
			description: 'Search issues with JQL, read details, and browse projects in Jira.',
			instructions: 'Go to your Atlassian account security settings and create an API token. You\'ll need your Jira subdomain (the part before .atlassian.net) and your Atlassian account email.',
			helpUrl: 'https://id.atlassian.com/manage-profile/security/api-tokens',
			helpLabel: 'Create API Token',
			fields: [
				{ key: 'jira_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.atlassian.net' },
				{ key: 'jira_email', label: 'Email', type: 'email', placeholder: 'you@company.com' },
				{ key: 'jira_api_token', label: 'API Token', type: 'password', placeholder: '' }
			]
		},
		{
			key: 'servicenow',
			name: 'ServiceNow',
			emoji: '🔧',
			description: 'Query tables, get records, and search incidents in your ServiceNow instance.',
			instructions: 'Enter your ServiceNow subdomain (the part before .service-now.com) and credentials for a user with API access. A dedicated service account with read-only access is recommended.',
			helpUrl: '',
			helpLabel: '',
			fields: [
				{ key: 'servicenow_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.service-now.com' },
				{ key: 'servicenow_username', label: 'Username', type: 'text', placeholder: '' },
				{ key: 'servicenow_password', label: 'Password', type: 'password', placeholder: '' }
			]
		},
		{
			key: 'salesforce',
			name: 'Salesforce',
			emoji: '☁️',
			description: 'Run SOQL queries, search records, and explore objects in your Salesforce org.',
			instructions: 'Enter your Salesforce subdomain, username, password, and security token. To get your security token: Salesforce → Settings → My Personal Information → Reset My Security Token (it will be emailed to you).',
			helpUrl: 'https://help.salesforce.com/s/articleView?id=sf.user_security_token.htm',
			helpLabel: 'Security Token Help',
			fields: [
				{ key: 'salesforce_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.my.salesforce.com' },
				{ key: 'salesforce_username', label: 'Username', type: 'email', placeholder: 'you@company.com' },
				{ key: 'salesforce_password', label: 'Password', type: 'password', placeholder: '' },
				{ key: 'salesforce_security_token', label: 'Security Token', type: 'password', placeholder: '' }
			]
		},
		{
			key: 'memory',
			name: 'Memory',
			emoji: '🧠',
			description: 'Persistent memory backed by a GitHub repo. IBEX remembers things across sessions.',
			instructions: 'Create a private GitHub repo for memory storage (e.g. ai-memory-yourname). Then create a fine-grained Personal Access Token scoped to that repo with Contents read & write permission.',
			helpUrl: 'https://github.com/settings/tokens?type=beta',
			helpLabel: 'Create GitHub Token',
			fields: [
				{ key: 'github_token', label: 'GitHub PAT', type: 'password', placeholder: 'ghp_...' },
				{ key: 'github_owner', label: 'Org or Username', type: 'text', placeholder: 'Percona-Lab' },
				{ key: 'github_repo', label: 'Repository', type: 'text', placeholder: 'ai-memory-yourname' }
			]
		}
	];

	function getConnectorDef(step: Step) {
		return connectorSteps.find(c => c.key === step);
	}

	const connectorKeys = connectorSteps.map(c => c.key);

	// ── Checks ──
	async function checkDocker() {
		try {
			const status = await invoke('get_docker_status');
			dockerOk = status === 'Healthy' || status === 'ContainerRunning' || status === 'ContainerStopped' || status === 'ContainerMissing';
			return dockerOk;
		} catch {
			dockerOk = false;
			return false;
		}
	}

	async function checkVpn() {
		vpnChecking = true;
		try {
			vpnOk = await invoke('check_network_connectivity') as boolean;
		} catch {
			vpnOk = false;
		} finally {
			vpnChecking = false;
		}
	}

	// ── Config ──
	async function saveConnectors() {
		try {
			// Convert subdomains → full URLs for backend
			if (config.jira_subdomain) {
				config.jira_domain = config.jira_subdomain.replace(/\.atlassian\.net$/i, '') + '.atlassian.net';
				delete config.jira_subdomain;
			}
			if (config.servicenow_subdomain) {
				config.servicenow_instance = config.servicenow_subdomain.replace(/\.service-now\.com$/i, '') + '.service-now.com';
				delete config.servicenow_subdomain;
			}
			if (config.salesforce_subdomain) {
				config.salesforce_instance_url = 'https://' + config.salesforce_subdomain.replace(/\.my\.salesforce\.com$/i, '') + '.my.salesforce.com';
				delete config.salesforce_subdomain;
			}
			await invoke('save_config', { newConfig: config });
		} catch (e) {
			console.error('Failed to save config:', e);
		}
	}

	// ── Connector Navigation ──
	function advanceFromConnector(key: ConnectorKey) {
		const next = nextStep(key as Step);
		if (next === 'launching') {
			startLaunch();
		} else {
			currentStep = next;
		}
	}

	function skipConnector(key: ConnectorKey) {
		connectorChoice[key] = 'skip';
		advanceFromConnector(key);
	}

	function backFromConnector(key: ConnectorKey) {
		// Reset choice so user sees Configure/Skip again if they come back
		connectorChoice[key] = 'pending';
		const prev = prevStep(key as Step);
		// In re-run mode, don't go back past the first connector
		if (isRerun && (prev === 'dependencies' || prev === 'welcome')) {
			return; // Already at first step
		}
		currentStep = prev;
	}

	// ── Launch ──
	async function startLaunch() {
		currentStep = 'launching';
		launchProgress = 0;
		errorMessage = '';

		// Save all connectors
		await saveConnectors();

		// Re-run mode: just restart MCP servers, don't re-launch everything
		if (isRerun) {
			statusMessage = 'Restarting connectors...';
			launchProgress = 50;
			try {
				await invoke('restart_servers');
				launchProgress = 100;
				statusMessage = 'Connectors updated!';
				setTimeout(() => { currentStep = 'done'; }, 500);
			} catch (e) {
				errorMessage = `Failed to restart servers: ${e}`;
				statusMessage = '';
			}
			return;
		}

		// First run: after backend startup completes, restart servers with
		// the newly configured connectors. The initial startup_sequence ran
		// with empty config (no connectors), so we need to:
		// 1. Recreate the Docker container with TOOL_SERVER_CONNECTIONS
		// 2. Start MCP server processes
		// 3. Re-push system prompt with default model
		async function finalizeFirstRun() {
			statusMessage = 'Starting connectors...';
			launchProgress = 70;
			try {
				await invoke('restart_servers');
			} catch (e) {
				console.warn('restart_servers failed:', e);
			}

			// Wait for Docker container to become healthy after recreation
			statusMessage = 'Waiting for services...';
			launchProgress = 85;
			try {
				await invoke('wait_for_docker_healthy');
			} catch (e) {
				console.warn('Docker health wait failed:', e);
			}

			launchProgress = 100;
			statusMessage = 'Ready!';
			setTimeout(() => { currentStep = 'done'; }, 500);
		}

		// Listen for startup status events
		const unlisten = await listen<string>('startup-status', (event) => {
			statusMessage = event.payload;
			launchProgress = Math.min(launchProgress + 15, 60);
		});

		const unlistenComplete = await listen<boolean>('startup-complete', async () => {
			await finalizeFirstRun();
		});

		const unlistenError = await listen<string>('startup-error', (event) => {
			errorMessage = event.payload;
			statusMessage = '';
		});

		// Startup may have already completed — check immediately
		try {
			const status: any = await invoke('get_startup_status');
			if (status.complete) {
				await finalizeFirstRun();
				return;
			}
			if (status.error) {
				errorMessage = status.error;
				statusMessage = '';
				return;
			}
		} catch (e) {
			console.error('Failed to check startup status:', e);
		}

		statusMessage = 'Starting IBEX services...';
		launchProgress = 5;
	}

	async function closeWindow() {
		try {
			const win = getCurrentWebviewWindow();
			await win.close();
		} catch {
			// Fallback: navigate to main chat
			goto('/', { replaceState: true });
		}
	}

	async function goToChat() {
		const store = await getStore(APP_STORE_FILE);
		await store?.set('webui_base_url', 'http://localhost:8080');
		await store?.save();

		// Inject fresh JWT before reload.
		// restart_servers() may have recreated the container, which generates
		// a new JWT. The old localStorage token could be stale.
		try {
			const jwt: string | null = await invoke('get_jwt_token');
			if (jwt) {
				localStorage.setItem('token', jwt);
			}
		} catch (e) {
			console.warn('Failed to get JWT for reload:', e);
		}

		// Show the branded splash screen before navigating so the user sees
		// a smooth transition instead of a blank/flashing screen.
		const splash = document.getElementById('splash-screen');
		if (splash) splash.style.display = 'flex';

		// Full page reload instead of SvelteKit client-side goto().
		// Layout.svelte's onMount returned early during /setup (no backend
		// init) and won't re-run on a client-side navigation. A full reload
		// forces Layout to re-initialize: fetch config, tools, models, set
		// up socket, etc. — so all 6 tool toggles and the correct default
		// model appear immediately.
		window.location.href = '/';
	}

	onMount(async () => {
		try {
			config = await invoke('get_config');
		} catch {
			config = {};
		}

		// Reverse-map full URLs → subdomains for the form
		if (config.jira_domain) {
			config.jira_subdomain = config.jira_domain.replace(/\.atlassian\.net$/i, '');
		}
		if (config.servicenow_instance) {
			config.servicenow_subdomain = config.servicenow_instance.replace(/\.service-now\.com$/i, '');
		}
		if (config.salesforce_instance_url) {
			config.salesforce_subdomain = config.salesforce_instance_url.replace(/^https?:\/\//i, '').replace(/\.my\.salesforce\.com$/i, '');
		}

		// Detect if this is a re-run (opened from tray "Connectors..." menu)
		try {
			const win = getCurrentWebviewWindow();
			if (win.label === 'connectors') {
				isRerun = true;
				// Skip welcome/dependencies — go straight to first connector
				currentStep = 'slack';
			}
		} catch {}

		const splash = document.getElementById('splash-screen');
		if (splash) {
			splash.style.display = 'none';
		}
	});
</script>

<div class="w-full h-screen max-h-[100dvh] bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100">
	<div class="w-full h-full flex items-center justify-center">
		<div class="w-full max-w-lg px-8">

			<!-- ═══ Progress Dots ═══ -->
			{#if currentStep !== 'welcome' && currentStep !== 'done'}
				<div class="flex justify-center gap-1.5 mb-8">
					{#each allSteps as step}
						{#if step !== 'welcome' && step !== 'done'}
							<div class="w-2 h-2 rounded-full transition-colors {
								currentStep === step
									? 'bg-blue-600'
									: allSteps.indexOf(step) < allSteps.indexOf(currentStep)
										? 'bg-blue-300 dark:bg-blue-700'
										: 'bg-gray-200 dark:bg-gray-700'
							}"></div>
						{/if}
					{/each}
				</div>
			{/if}

			<!-- ═══ Step: Welcome ═══ -->
			{#if currentStep === 'welcome'}
				<div class="text-center space-y-6 animate-in">
					<div class="flex justify-center mb-2">
						<img src="/ibex-icon.png" alt="IBEX" class="w-16 h-16" />
					</div>
					<h1 class="text-3xl font-bold">Welcome to IBEX</h1>
					<p class="text-xs font-medium tracking-wider text-gray-400 dark:text-gray-500 uppercase -mt-4">
						Integration Bridge for EXtended systems
					</p>
					<p class="text-gray-500 dark:text-gray-400">
						Your workplace AI assistant. IBEX connects to your team's tools —
						Slack, Jira, Notion, and more — so you can ask questions about your work.
					</p>
					<button
						on:click={() => { checkDocker(); checkVpn(); currentStep = 'dependencies'; }}
						class="px-8 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-full transition-colors"
					>
						Get Started
					</button>
				</div>
			{/if}

			<!-- ═══ Step: Dependencies ═══ -->
			{#if currentStep === 'dependencies'}
				<div class="space-y-6 animate-in">
					<h2 class="text-2xl font-bold text-center">Prerequisites</h2>
					<p class="text-gray-500 dark:text-gray-400 text-center">
						IBEX needs Docker Desktop to run the AI backend.
					</p>

					<div class="bg-gray-50 dark:bg-gray-800 rounded-lg p-4 space-y-3">
						<!-- Docker check -->
						<div class="flex items-center justify-between">
							<span class="font-medium">Docker Desktop</span>
							{#if dockerOk}
								<span class="text-green-500 font-medium">Installed ✓</span>
							{:else}
								<a href="https://docker.com/products/docker-desktop" target="_blank" rel="noopener"
									class="text-blue-500 hover:underline text-sm">
									Download →
								</a>
							{/if}
						</div>

						<!-- VPN check -->
						<div class="flex items-center justify-between">
							<span class="font-medium">Percona VPN</span>
							{#if vpnChecking}
								<span class="text-gray-400 text-sm">Checking...</span>
							{:else if vpnOk}
								<span class="text-green-500 font-medium">Connected ✓</span>
							{:else}
								<span class="text-yellow-500 font-medium">⚠ Not Connected</span>
							{/if}
						</div>
					</div>

					<!-- VPN warning -->
					{#if !vpnOk && !vpnChecking}
						<div class="bg-yellow-50 dark:bg-yellow-900/20 text-yellow-700 dark:text-yellow-400 rounded-lg p-3 text-sm">
							Could not reach Percona internal services. Please connect to the Percona VPN
							and click "Check Again". You can still proceed, but some features may not work.
						</div>
					{/if}

					{#if !dockerOk}
						<p class="text-sm text-gray-400 text-center">
							Install and start Docker Desktop, then click Check Again.
						</p>
					{/if}

					<div class="flex justify-between pt-4">
						<button
							on:click={() => currentStep = 'welcome'}
							class="px-6 py-2.5 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
						>
							Back
						</button>
						<div class="flex gap-3">
							{#if !dockerOk || !vpnOk}
								<button
									on:click={() => { checkDocker(); checkVpn(); }}
									class="px-6 py-2.5 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
								>
									Check Again
								</button>
							{/if}
							<button
								on:click={() => currentStep = 'slack'}
								disabled={!dockerOk}
								class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white font-medium rounded-lg transition-colors"
							>
								Next
							</button>
						</div>
					</div>
				</div>
			{/if}

			<!-- ═══ Connector Steps (Slack, Notion, Jira, Memory) ═══ -->
			{#each connectorSteps as def (def.key)}
				{#if currentStep === def.key}
					<div class="space-y-5 animate-in">
						<div class="text-center">
							<div class="text-4xl mb-2">{def.emoji}</div>
							<h2 class="text-2xl font-bold">{def.name}</h2>
							<p class="text-gray-500 dark:text-gray-400 text-sm mt-1">{def.description}</p>
						</div>

						{#if connectorChoice[def.key] === 'pending'}
							<!-- Choice: Configure or Skip -->
							<div class="flex justify-center gap-4 pt-4">
								<button
									on:click={() => connectorChoice[def.key] = 'configure'}
									class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
								>
									Configure
								</button>
								<button
									on:click={() => skipConnector(def.key)}
									class="px-6 py-2.5 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 border border-gray-300 dark:border-gray-600 rounded-lg transition-colors"
								>
									Skip
								</button>
							</div>

							<!-- Back link (hidden on first connector in re-run mode) -->
							{#if !(isRerun && def.key === connectorKeys[0])}
							<div class="flex justify-start pt-2">
								<button
									on:click={() => backFromConnector(def.key)}
									class="px-6 py-2.5 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors text-sm"
								>
									← Back
								</button>
							</div>
							{/if}
						{:else if connectorChoice[def.key] === 'configure'}
							<!-- Credential instructions -->
							<div class="bg-blue-50 dark:bg-blue-900/20 rounded-lg p-3 text-sm text-blue-800 dark:text-blue-300">
								<p>{def.instructions}</p>
								{#if def.helpUrl}
									<a href={def.helpUrl} target="_blank" rel="noopener"
										class="inline-block mt-1.5 text-blue-600 dark:text-blue-400 hover:underline font-medium">
										{def.helpLabel} ↗
									</a>
								{/if}
							</div>

							<!-- Input fields -->
							<div class="space-y-3">
								{#each def.fields as field}
									<div class="space-y-1">
										<label for={field.key} class="text-sm text-gray-600 dark:text-gray-300">
											{field.label}
										</label>
										{#if field.type === 'password'}
											<input
												id={field.key}
												type="password"
												bind:value={config[field.key]}
												placeholder={field.placeholder}
												class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none"
											/>
										{:else if field.type === 'email'}
											<input
												id={field.key}
												type="email"
												bind:value={config[field.key]}
												placeholder={field.placeholder}
												class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none"
											/>
										{:else}
											{#if field.suffix}
												<div class="flex items-center">
													<input
														id={field.key}
														type="text"
														bind:value={config[field.key]}
														placeholder={field.placeholder}
														class="flex-1 px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-l-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none"
													/>
													<span class="px-3 py-2 text-sm bg-gray-100 dark:bg-gray-700 border border-l-0 border-gray-200 dark:border-gray-600 rounded-r-lg text-gray-500 dark:text-gray-400 whitespace-nowrap">
														{field.suffix}
													</span>
												</div>
											{:else}
												<input
													id={field.key}
													type="text"
													bind:value={config[field.key]}
													placeholder={field.placeholder}
													class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none"
												/>
											{/if}
										{/if}
									</div>
								{/each}
							</div>

							<!-- Navigation -->
							<div class="flex justify-between pt-2">
								<button
									on:click={() => backFromConnector(def.key)}
									class="px-6 py-2.5 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
								>
									Back
								</button>
								<button
									on:click={() => advanceFromConnector(def.key)}
									class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
								>
									Next
								</button>
							</div>
						{/if}
					</div>
				{/if}
			{/each}

			<!-- ═══ Step: Launching ═══ -->
			{#if currentStep === 'launching'}
				<div class="text-center space-y-6 animate-in">
					<div class="text-5xl animate-pulse">🚀</div>
					<h2 class="text-2xl font-bold">{isRerun ? 'Updating Connectors' : 'Setting Up IBEX'}</h2>

					{#if errorMessage}
						<div class="bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg p-4 text-sm">
							{errorMessage}
						</div>
						<button
							on:click={() => currentStep = 'memory'}
							class="px-6 py-2.5 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
						>
							Back
						</button>
					{:else}
						<p class="text-gray-500 dark:text-gray-400">{statusMessage || 'Initializing...'}</p>

						<!-- Progress bar -->
						<div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2 overflow-hidden">
							<div
								class="bg-blue-600 h-full rounded-full transition-all duration-500"
								style="width: {launchProgress}%"
							></div>
						</div>
					{/if}
				</div>
			{/if}

			<!-- ═══ Step: Done ═══ -->
			{#if currentStep === 'done'}
				<div class="text-center space-y-6 animate-in">
					<div class="text-5xl">✅</div>
					<h2 class="text-2xl font-bold">{isRerun ? 'Connectors Updated!' : "You're All Set!"}</h2>
					<p class="text-gray-500 dark:text-gray-400">
						{isRerun
							? 'Your connector settings have been saved and servers restarted.'
							: 'IBEX is running and connected to your tools. Start asking questions about your work!'}
					</p>
					<button
						on:click={isRerun ? closeWindow : goToChat}
						class="px-8 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-full transition-colors"
					>
						{isRerun ? 'Close' : 'Start Chatting'}
					</button>
				</div>
			{/if}

		</div>
	</div>
</div>

<style>
	.animate-in {
		animation: fadeIn 0.3s ease-out;
	}

	@keyframes fadeIn {
		from { opacity: 0; transform: translateY(10px); }
		to { opacity: 1; transform: translateY(0); }
	}
</style>
