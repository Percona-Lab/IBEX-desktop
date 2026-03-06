<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';

	// Connector configuration form state
	let config: any = {};
	let saving = false;
	let restarting = false;
	let message = '';
	let messageType: 'success' | 'error' = 'success';
	let health: string = 'Starting';
	let connectors: string[] = [];
	let dockerStatus: string = 'Unknown';
	let serverStatuses: Record<string, any> = {};

	// Collapsible sections
	let expandedSections: Record<string, boolean> = {};

	const connectorDefs: Array<{
		key: string;
		name: string;
		fields: Array<{ key: string; label: string; type: string; placeholder: string; suffix?: string }>;
		helpUrl: string;
		helpText: string;
	}> = [
		{
			key: 'slack',
			name: 'Slack',
			fields: [
				{ key: 'slack_token', label: 'User OAuth Token', type: 'password', placeholder: 'xoxp-...' }
			],
			helpUrl: 'https://api.slack.com/apps',
			helpText: 'Create app \u2192 OAuth & Permissions \u2192 User Token Scopes: search:read, channels:history, channels:read, users:read \u2192 Install to Workspace \u2192 Copy User OAuth Token'
		},
		{
			key: 'notion',
			name: 'Notion',
			fields: [
				{ key: 'notion_token', label: 'Integration Token', type: 'password', placeholder: 'ntn_...' }
			],
			helpUrl: 'https://www.notion.so/profile/integrations',
			helpText: 'New integration \u2192 Copy Internal Integration Secret \u2192 Add integration to pages via \u00b7\u00b7\u00b7 menu \u2192 Connections'
		},
		{
			key: 'jira',
			name: 'Jira',
			fields: [
				{ key: 'jira_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.atlassian.net' },
				{ key: 'jira_email', label: 'Email', type: 'email', placeholder: 'you@company.com' },
				{ key: 'jira_api_token', label: 'API Token', type: 'password', placeholder: '' }
			],
			helpUrl: 'https://id.atlassian.com/manage-profile/security/api-tokens',
			helpText: 'Create API token from your Atlassian account security settings'
		},
		{
			key: 'servicenow',
			name: 'ServiceNow',
			fields: [
				{ key: 'servicenow_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.service-now.com' },
				{ key: 'servicenow_username', label: 'Username', type: 'text', placeholder: '' },
				{ key: 'servicenow_password', label: 'Password', type: 'password', placeholder: '' }
			],
			helpUrl: '',
			helpText: 'Enter the part before .service-now.com'
		},
		{
			key: 'salesforce',
			name: 'Salesforce',
			fields: [
				{ key: 'salesforce_subdomain', label: 'Subdomain', type: 'text', placeholder: 'yourcompany', suffix: '.my.salesforce.com' },
				{ key: 'salesforce_username', label: 'Username', type: 'email', placeholder: 'you@company.com' },
				{ key: 'salesforce_password', label: 'Password', type: 'password', placeholder: '' },
				{ key: 'salesforce_security_token', label: 'Security Token', type: 'password', placeholder: '' }
			],
			helpUrl: 'https://help.salesforce.com/s/articleView?id=sf.user_security_token.htm',
			helpText: 'Settings → My Personal Information → Reset My Security Token'
		},
		{
			key: 'memory',
			name: 'Memory (GitHub)',
			fields: [
				{ key: 'github_token', label: 'GitHub PAT', type: 'password', placeholder: 'ghp_...' },
				{ key: 'github_owner', label: 'Org or Username', type: 'text', placeholder: 'Percona-Lab' },
				{ key: 'github_repo', label: 'Repository', type: 'text', placeholder: 'ai-memory-yourname' }
			],
			helpUrl: 'https://github.com/settings/tokens?type=beta',
			helpText: 'Create a private repo \u2192 Fine-grained PAT \u2192 Scope to your org \u2192 Select repo \u2192 Permissions: Contents \u2192 Read and write'
		}
	];

	function isConfigured(def: typeof connectorDefs[0]): boolean {
		return def.fields.every(f => config[f.key] && config[f.key].trim() !== '');
	}

	function reverseMapSubdomains() {
		if (config.jira_domain) {
			config.jira_subdomain = config.jira_domain.replace(/\.atlassian\.net$/i, '');
		}
		if (config.servicenow_instance) {
			config.servicenow_subdomain = config.servicenow_instance.replace(/\.service-now\.com$/i, '');
		}
		if (config.salesforce_instance_url) {
			config.salesforce_subdomain = config.salesforce_instance_url.replace(/^https?:\/\//i, '').replace(/\.my\.salesforce\.com$/i, '');
		}
	}

	function convertSubdomains() {
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
	}

	async function loadState() {
		try {
			config = await invoke('get_config');
			reverseMapSubdomains();
			health = await invoke('get_health');
			connectors = await invoke('get_configured_connectors');
			dockerStatus = await invoke('get_docker_status');
			serverStatuses = await invoke('get_server_statuses');
		} catch (e) {
			console.error('Failed to load state:', e);
		}
	}

	async function saveConfig() {
		saving = true;
		message = '';
		try {
			convertSubdomains();
			await invoke('save_config', { newConfig: config });
			message = 'Configuration saved! Click "Restart Servers" to apply changes.';
			messageType = 'success';
			await loadState();
		} catch (e: any) {
			message = `Error: ${e}`;
			messageType = 'error';
		} finally {
			saving = false;
		}
	}

	async function restartServers() {
		restarting = true;
		message = '';
		try {
			await invoke('restart_servers');
			message = 'Servers restarted successfully!';
			messageType = 'success';
			await loadState();
		} catch (e: any) {
			message = `Restart error: ${e}`;
			messageType = 'error';
		} finally {
			restarting = false;
		}
	}

	async function saveAndRestart() {
		saving = true;
		restarting = true;
		message = '';
		try {
			convertSubdomains();
			await invoke('save_config', { newConfig: config });
			await invoke('restart_servers');
			message = 'Configuration saved and servers restarted!';
			messageType = 'success';
			await loadState();
		} catch (e: any) {
			message = `Error: ${e}`;
			messageType = 'error';
		} finally {
			saving = false;
			restarting = false;
		}
	}

	function toggleSection(key: string) {
		expandedSections[key] = !expandedSections[key];
	}

	onMount(() => {
		loadState();

		// Auto-refresh state every 10 seconds
		const interval = setInterval(loadState, 10000);

		return () => clearInterval(interval);
	});
</script>

<div class="min-h-screen bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100">
	<!-- Header -->
	<div class="sticky top-0 z-10 bg-white/80 dark:bg-gray-900/80 backdrop-blur-sm border-b border-gray-200 dark:border-gray-700">
		<div class="max-w-2xl mx-auto px-6 py-4 flex items-center justify-between">
			<h1 class="text-xl font-semibold">IBEX Settings</h1>
			<div class="flex items-center gap-3">
				<span class="text-sm px-2 py-1 rounded-full {
					health === 'Healthy' ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400' :
					health === 'Degraded' ? 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400' :
					health === 'Starting' ? 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400' :
					'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
				}">
					{health}
				</span>
			</div>
		</div>
	</div>

	<div class="max-w-2xl mx-auto px-6 py-6 space-y-6">
		<!-- Status Overview -->
		<div class="bg-gray-50 dark:bg-gray-800 rounded-lg p-4 space-y-2">
			<h2 class="text-sm font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide">Status</h2>
			<div class="grid grid-cols-2 gap-3 text-sm">
				<div class="flex items-center gap-2">
					<span class="{dockerStatus === 'Healthy' ? 'text-green-500' : 'text-red-500'}">●</span>
					Docker: {dockerStatus}
				</div>
				{#each connectors as c}
					<div class="flex items-center gap-2">
						<span class="text-green-500">●</span>
						{c}
					</div>
				{/each}
			</div>
		</div>

		<!-- Server Statuses -->
		{#if Object.keys(serverStatuses).length > 0}
			<div class="bg-gray-50 dark:bg-gray-800 rounded-lg p-4 space-y-2">
				<div class="flex items-center justify-between">
					<h2 class="text-sm font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide">Servers</h2>
					<button
						on:click={restartServers}
						disabled={restarting}
						class="text-xs px-3 py-1 text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded transition-colors disabled:opacity-50"
					>
						{restarting ? 'Restarting...' : 'Restart All'}
					</button>
				</div>
				<div class="space-y-1 text-sm">
					{#each Object.entries(serverStatuses) as [name, status]}
						<div class="flex items-center justify-between">
							<span class="flex items-center gap-2">
								<span class="{status.healthy ? 'text-green-500' : status.running ? 'text-yellow-500' : 'text-red-500'}">●</span>
								{name}
							</span>
							<span class="text-gray-400">:{status.port}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Message -->
		{#if message}
			<div class="rounded-lg p-3 text-sm {messageType === 'success' ? 'bg-green-50 text-green-800 dark:bg-green-900/20 dark:text-green-400' : 'bg-red-50 text-red-800 dark:bg-red-900/20 dark:text-red-400'}">
				{message}
			</div>
		{/if}

		<!-- Connectors -->
		<h2 class="text-sm font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide">Connectors</h2>

		{#each connectorDefs as def}
			<div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
				<!-- Section header (clickable) -->
				<button
					class="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
					on:click={() => toggleSection(def.key)}
				>
					<div class="flex items-center gap-3">
						<span class="{isConfigured(def) ? 'text-green-500' : 'text-gray-300 dark:text-gray-600'}">
							{isConfigured(def) ? '\u2713' : '\u25cb'}
						</span>
						<span class="font-medium">{def.name}</span>
					</div>
					<svg
						class="w-5 h-5 text-gray-400 transition-transform {expandedSections[def.key] ? 'rotate-180' : ''}"
						fill="none" viewBox="0 0 24 24" stroke="currentColor"
					>
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
					</svg>
				</button>

				<!-- Expanded section -->
				{#if expandedSections[def.key]}
					<div class="px-4 pb-4 space-y-3 border-t border-gray-100 dark:border-gray-700">
						{#if def.helpText}
							<p class="text-xs text-gray-500 dark:text-gray-400 mt-3">
								{def.helpText}
								{#if def.helpUrl}
									<a href={def.helpUrl} target="_blank" rel="noopener" class="text-blue-500 hover:underline ml-1">
										Setup guide \u2192
									</a>
								{/if}
							</p>
						{/if}

						{#each def.fields as field}
							<div class="space-y-1">
								<label for={field.key} class="text-sm font-medium text-gray-600 dark:text-gray-300">
									{field.label}
								</label>
								{#if field.type === 'password'}
									<input
										id={field.key}
										type="password"
										bind:value={config[field.key]}
										placeholder={field.placeholder}
										class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition"
									/>
								{:else if field.type === 'email'}
									<input
										id={field.key}
										type="email"
										bind:value={config[field.key]}
										placeholder={field.placeholder}
										class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition"
									/>
								{:else}
									{#if field.suffix}
										<div class="flex items-center">
											<input
												id={field.key}
												type="text"
												bind:value={config[field.key]}
												placeholder={field.placeholder}
												class="flex-1 px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-l-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition"
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
											class="w-full px-3 py-2 text-sm border border-gray-200 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 focus:ring-2 focus:ring-blue-500 focus:border-transparent outline-none transition"
										/>
									{/if}
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/each}

		<!-- Action buttons -->
		<div class="flex justify-between items-center pt-2">
			<button
				on:click={saveConfig}
				disabled={saving}
				class="px-6 py-2.5 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50 font-medium text-sm rounded-lg transition-colors"
			>
				{saving ? 'Saving...' : 'Save Only'}
			</button>
			<button
				on:click={saveAndRestart}
				disabled={saving || restarting}
				class="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white font-medium text-sm rounded-lg transition-colors"
			>
				{saving || restarting ? 'Applying...' : 'Save & Restart'}
			</button>
		</div>

		<!-- Footer -->
		<div class="text-xs text-gray-400 dark:text-gray-500 text-center pt-4 pb-8">
			Credentials encrypted in macOS Keychain · Config: ~/.ibex-mcp.env
		</div>
	</div>
</div>
