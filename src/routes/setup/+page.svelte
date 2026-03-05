<script lang="ts">
	import { goto } from '$app/navigation';
	import { APP_STORE_FILE } from '$lib/app/constants';
	import { WEBUI_BASE_URL } from '$lib/stores';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getStore } from '@tauri-apps/plugin-store';
	import { onMount } from 'svelte';

	type Step = 'welcome' | 'dependencies' | 'connectors' | 'launching' | 'done';

	let currentStep: Step = 'welcome';
	let statusMessage = '';
	let errorMessage = '';
	let dockerOk = false;
	let launchProgress = 0;

	// Connector config (minimal for first run)
	let config: any = {};
	let expandedConnectors: Record<string, boolean> = {};

	const connectorDefs = [
		{
			key: 'slack', name: 'Slack',
			fields: [{ key: 'slack_token', label: 'User OAuth Token', type: 'password', placeholder: 'xoxp-...' }]
		},
		{
			key: 'notion', name: 'Notion',
			fields: [{ key: 'notion_token', label: 'Integration Token', type: 'password', placeholder: 'ntn_...' }]
		},
		{
			key: 'jira', name: 'Jira',
			fields: [
				{ key: 'jira_domain', label: 'Domain', type: 'text', placeholder: 'yourcompany.atlassian.net' },
				{ key: 'jira_email', label: 'Email', type: 'email', placeholder: 'you@company.com' },
				{ key: 'jira_api_token', label: 'API Token', type: 'password', placeholder: '' }
			]
		},
		{
			key: 'memory', name: 'Memory (GitHub)',
			fields: [
				{ key: 'github_token', label: 'GitHub PAT', type: 'password', placeholder: 'ghp_...' },
				{ key: 'github_owner', label: 'Org/Username', type: 'text', placeholder: 'Percona-Lab' },
				{ key: 'github_repo', label: 'Repository', type: 'text', placeholder: 'ai-memory-yourname' }
			]
		}
	];

	function hasAnyConnector(): boolean {
		return connectorDefs.some(def =>
			def.fields.some(f => config[f.key] && config[f.key].trim() !== '')
		);
	}

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

	async function saveConnectors() {
		try {
			await invoke('save_config', { newConfig: config });
		} catch (e) {
			console.error('Failed to save config:', e);
		}
	}

	async function startLaunch() {
		currentStep = 'launching';
		launchProgress = 0;
		errorMessage = '';

		// Save connectors first
		await saveConnectors();

		// Listen for startup status events
		const unlisten = await listen<string>('startup-status', (event) => {
			statusMessage = event.payload;
			launchProgress = Math.min(launchProgress + 15, 90);
		});

		const unlistenComplete = await listen<boolean>('startup-complete', () => {
			launchProgress = 100;
			statusMessage = 'Ready!';
			setTimeout(() => {
				currentStep = 'done';
			}, 500);
		});

		const unlistenError = await listen<string>('startup-error', (event) => {
			errorMessage = event.payload;
			statusMessage = '';
		});

		// The startup sequence runs automatically in lib.rs setup()
		// But if we got here, it may have already failed or not started yet
		// Wait a moment, then check
		setTimeout(async () => {
			if (launchProgress === 0) {
				statusMessage = 'Starting IBEX services...';
				launchProgress = 5;
			}
		}, 1000);
	}

	async function goToChat() {
		// Set the base URL so the app navigates to the main chat
		const store = await getStore(APP_STORE_FILE);
		await store?.set('webui_base_url', 'http://localhost:8080');
		await store?.save();
		$WEBUI_BASE_URL = 'http://localhost:8080';
		goto('/', { replaceState: true });
	}

	onMount(async () => {
		// Load existing config if any
		try {
			config = await invoke('get_config');
		} catch {
			config = {};
		}

		// Hide splash screen
		const splash = document.getElementById('splash-screen');
		if (splash) {
			splash.style.display = 'none';
		}
	});
</script>

<div class="w-full h-screen max-h-[100dvh] bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100">
	<div class="w-full h-full flex items-center justify-center">
		<div class="w-full max-w-lg px-8">

			<!-- Step: Welcome -->
			{#if currentStep === 'welcome'}
				<div class="text-center space-y-6 animate-in">
					<div class="text-6xl mb-2">🦌</div>
					<h1 class="text-3xl font-bold">Welcome to IBEX</h1>
					<p class="text-gray-500 dark:text-gray-400">
						Your workplace AI assistant. IBEX connects to your team's tools —
						Slack, Jira, Notion, and more — so you can ask questions about your work.
					</p>
					<button
						on:click={() => { checkDocker(); currentStep = 'dependencies'; }}
						class="px-8 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-full transition-colors"
					>
						Get Started
					</button>
				</div>
			{/if}

			<!-- Step: Dependencies -->
			{#if currentStep === 'dependencies'}
				<div class="space-y-6 animate-in">
					<h2 class="text-2xl font-bold text-center">Prerequisites</h2>
					<p class="text-gray-500 dark:text-gray-400 text-center">
						IBEX needs Docker Desktop to run the AI backend.
					</p>

					<div class="bg-gray-50 dark:bg-gray-800 rounded-lg p-4 space-y-3">
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
					</div>

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
							{#if !dockerOk}
								<button
									on:click={checkDocker}
									class="px-6 py-2.5 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
								>
									Check Again
								</button>
							{/if}
							<button
								on:click={() => currentStep = 'connectors'}
								disabled={!dockerOk}
								class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white font-medium rounded-lg transition-colors"
							>
								Next
							</button>
						</div>
					</div>
				</div>
			{/if}

			<!-- Step: Connectors -->
			{#if currentStep === 'connectors'}
				<div class="space-y-4 animate-in">
					<h2 class="text-2xl font-bold text-center">Connect Your Tools</h2>
					<p class="text-gray-500 dark:text-gray-400 text-center text-sm">
						Add credentials for the tools you want IBEX to access.
						You can add more later in Settings.
					</p>

					{#each connectorDefs as def}
						<div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
							<button
								class="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
								on:click={() => expandedConnectors[def.key] = !expandedConnectors[def.key]}
							>
								<span class="font-medium">{def.name}</span>
								<svg
									class="w-4 h-4 text-gray-400 transition-transform {expandedConnectors[def.key] ? 'rotate-180' : ''}"
									fill="none" viewBox="0 0 24 24" stroke="currentColor"
								>
									<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
								</svg>
							</button>

							{#if expandedConnectors[def.key]}
								<div class="px-4 pb-4 space-y-3 border-t border-gray-100 dark:border-gray-700">
									{#each def.fields as field}
										<div class="space-y-1 mt-2">
											<label for={field.key} class="text-sm text-gray-600 dark:text-gray-300">
												{field.label}
											</label>
											<input
												id={field.key}
												type={field.type}
												bind:value={config[field.key]}
												placeholder={field.placeholder}
												class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none"
											/>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					{/each}

					<div class="flex justify-between pt-4">
						<button
							on:click={() => currentStep = 'dependencies'}
							class="px-6 py-2.5 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
						>
							Back
						</button>
						<button
							on:click={startLaunch}
							class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-colors"
						>
							{hasAnyConnector() ? 'Launch IBEX' : 'Skip & Launch'}
						</button>
					</div>
				</div>
			{/if}

			<!-- Step: Launching -->
			{#if currentStep === 'launching'}
				<div class="text-center space-y-6 animate-in">
					<div class="text-5xl animate-pulse">🚀</div>
					<h2 class="text-2xl font-bold">Setting Up IBEX</h2>

					{#if errorMessage}
						<div class="bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400 rounded-lg p-4 text-sm">
							{errorMessage}
						</div>
						<button
							on:click={() => currentStep = 'dependencies'}
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

			<!-- Step: Done -->
			{#if currentStep === 'done'}
				<div class="text-center space-y-6 animate-in">
					<div class="text-5xl">✅</div>
					<h2 class="text-2xl font-bold">You're All Set!</h2>
					<p class="text-gray-500 dark:text-gray-400">
						IBEX is running and connected to your tools.
						Start asking questions about your work!
					</p>
					<button
						on:click={goToChat}
						class="px-8 py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-full transition-colors"
					>
						Start Chatting
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
