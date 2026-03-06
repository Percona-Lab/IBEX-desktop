<script lang="ts">
	import { onMount, getContext } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { toast } from 'svelte-sonner';
	import { user, WEBUI_BASE_URL } from '$lib/stores';
	import Spinner from '$lib/components/common/Spinner.svelte';
	import Tooltip from '$lib/components/common/Tooltip.svelte';

	const i18n = getContext('i18n');

	type ServerStatus = {
		name: string;
		port: number;
		running: boolean;
		healthy: boolean;
	};

	type DockerStatus =
		| 'NotInstalled'
		| 'NotRunning'
		| 'ContainerMissing'
		| 'ContainerStopped'
		| 'ContainerRunning'
		| 'Healthy';

	type AppHealth = 'Healthy' | 'Degraded' | 'Error' | 'Starting';

	let loading = true;
	let refreshing = false;
	let restarting = false;

	let serverStatuses: Record<string, ServerStatus> = {};
	let configuredConnectors: string[] = [];
	let dockerStatus: DockerStatus = 'ContainerMissing';
	let appHealth: AppHealth = 'Starting';

	// Open WebUI registered tools (from the backend API)
	let registeredTools: any[] = [];
	let toolsError: string | null = null;

	const CONNECTOR_META: Record<
		string,
		{ emoji: string; description: string; defaultPort: number }
	> = {
		slack: {
			emoji: '💬',
			description: 'Search messages, read channels, and browse threads',
			defaultPort: 3001
		},
		notion: {
			emoji: '📝',
			description: 'Search pages, read content, and query databases',
			defaultPort: 3002
		},
		jira: {
			emoji: '🎫',
			description: 'Search issues, read details, and browse projects',
			defaultPort: 3003
		},
		memory: {
			emoji: '🧠',
			description: 'Persistent memory across sessions via GitHub',
			defaultPort: 3004
		},
		servicenow: {
			emoji: '🔧',
			description: 'Query tables, get records, and list tables',
			defaultPort: 3005
		},
		salesforce: {
			emoji: '☁️',
			description: 'Run SOQL queries, get records, and search objects',
			defaultPort: 3006
		}
	};

	// All possible servers (including unconfigured) for toggle display
	const ALL_SERVERS = ['slack', 'notion', 'jira', 'memory', 'servicenow', 'salesforce'];

	async function loadStatus() {
		try {
			[serverStatuses, configuredConnectors, dockerStatus, appHealth] = await Promise.all([
				invoke<Record<string, ServerStatus>>('get_server_statuses'),
				invoke<string[]>('get_configured_connectors'),
				invoke<DockerStatus>('get_docker_status'),
				invoke<AppHealth>('get_health')
			]);
		} catch (err) {
			console.error('Failed to load server statuses:', err);
		}
	}

	async function loadRegisteredTools() {
		try {
			toolsError = null;
			const token = localStorage.token;
			if (!token || !$WEBUI_BASE_URL) {
				toolsError = 'No auth token available';
				return;
			}

			const res = await fetch(`${$WEBUI_BASE_URL}/api/tools/`, {
				method: 'GET',
				headers: {
					Accept: 'application/json',
					'Content-Type': 'application/json',
					authorization: `Bearer ${token}`
				}
			});

			if (res.ok) {
				registeredTools = await res.json();
			} else {
				const errBody = await res.json().catch(() => ({}));
				toolsError = errBody.detail || `HTTP ${res.status}`;
			}
		} catch (err: any) {
			toolsError = err?.message || 'Failed to fetch tools';
			console.error('Failed to load registered tools:', err);
		}
	}

	async function loadAll() {
		loading = true;
		await Promise.all([loadStatus(), loadRegisteredTools()]);
		loading = false;
	}

	async function refresh() {
		refreshing = true;
		await Promise.all([loadStatus(), loadRegisteredTools()]);
		refreshing = false;
	}

	async function restartServers() {
		restarting = true;
		try {
			await invoke('restart_servers');
			toast.success('Servers restarted successfully');

			// Wait a moment for servers to start up, then refresh
			await new Promise((r) => setTimeout(r, 3000));
			await refresh();
		} catch (err: any) {
			console.error('Failed to restart servers:', err);
			toast.error(`Failed to restart servers: ${err}`);
		} finally {
			restarting = false;
		}
	}

	function getHealthIcon(status: ServerStatus | undefined): string {
		if (!status) return '⚪';
		if (status.healthy) return '🟢';
		if (status.running) return '🟡';
		return '🔴';
	}

	function getHealthLabel(status: ServerStatus | undefined): string {
		if (!status) return 'Not started';
		if (status.healthy) return 'Healthy';
		if (status.running) return 'Starting…';
		return 'Stopped';
	}

	function getDockerIcon(status: DockerStatus): string {
		switch (status) {
			case 'Healthy':
				return '🟢';
			case 'ContainerRunning':
				return '🟡';
			case 'ContainerStopped':
			case 'ContainerMissing':
				return '🔴';
			case 'NotRunning':
			case 'NotInstalled':
				return '⛔';
			default:
				return '⚪';
		}
	}

	function getDockerLabel(status: DockerStatus): string {
		switch (status) {
			case 'Healthy':
				return 'Running & healthy';
			case 'ContainerRunning':
				return 'Container running (waiting for health check)';
			case 'ContainerStopped':
				return 'Container stopped';
			case 'ContainerMissing':
				return 'Container not created';
			case 'NotRunning':
				return 'Docker not running';
			case 'NotInstalled':
				return 'Docker not installed';
			default:
				return 'Unknown';
		}
	}

	function isConfigured(name: string): boolean {
		return configuredConnectors.includes(name);
	}

	onMount(() => {
		loadAll();

		// Auto-refresh when servers are restarted from tray
		const unlistenPromise = listen('servers-restarted', async () => {
			await new Promise((r) => setTimeout(r, 3000));
			await refresh();
		});

		return () => {
			unlistenPromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="flex flex-col gap-4">
	<!-- Header with actions -->
	<div class="flex items-center justify-between">
		<div>
			<div class="text-sm font-medium">{$i18n.t('Tools & Integrations')}</div>
			<div class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
				MCP tool servers and connector status
			</div>
		</div>
		<div class="flex items-center gap-2">
			<button
				class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition"
				on:click={refresh}
				disabled={refreshing || restarting}
			>
				{#if refreshing}
					<Spinner className="size-3" />
				{:else}
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 20 20"
						fill="currentColor"
						class="size-3.5"
					>
						<path
							fill-rule="evenodd"
							d="M15.312 11.424a5.5 5.5 0 01-9.201 2.466l-.312-.311h2.433a.75.75 0 000-1.5H4.28a.75.75 0 00-.75.75v3.955a.75.75 0 001.5 0v-2.134l.235.234A7 7 0 0015.312 11.424zM4.688 8.576a5.5 5.5 0 019.201-2.466l.312.311H11.77a.75.75 0 000 1.5h3.952a.75.75 0 00.75-.75V3.216a.75.75 0 00-1.5 0v2.134l-.235-.234A7 7 0 004.688 8.576z"
							clip-rule="evenodd"
						/>
					</svg>
				{/if}
				Refresh
			</button>
			<button
				class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg text-white {restarting
					? 'bg-purple-400 dark:bg-purple-600 cursor-wait'
					: 'bg-purple-500 hover:bg-purple-600 dark:bg-purple-600 dark:hover:bg-purple-500'} transition"
				on:click={restartServers}
				disabled={restarting}
			>
				{#if restarting}
					<Spinner className="size-3" />
					Restarting…
				{:else}
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 20 20"
						fill="currentColor"
						class="size-3.5"
					>
						<path
							fill-rule="evenodd"
							d="M15.312 11.424a5.5 5.5 0 0 1-9.201 2.466l-.312-.311h2.433a.75.75 0 0 0 0-1.5H4.28a.75.75 0 0 0-.75.75v3.955a.75.75 0 0 0 1.5 0v-2.134l.235.234A7 7 0 0 0 16.312 11.424zM4.688 8.576a5.5 5.5 0 0 1 9.201-2.466l.312.311H11.77a.75.75 0 0 1 0 1.5h3.952a.75.75 0 0 1 .75-.75V3.216a.75.75 0 0 1-1.5 0v2.134l-.235-.234A7 7 0 0 1 4.688 8.576z"
							clip-rule="evenodd"
						/>
					</svg>
					Restart Servers
				{/if}
			</button>
		</div>
	</div>

	{#if loading}
		<div class="flex justify-center py-8">
			<Spinner />
		</div>
	{:else}
		<!-- Overall Status -->
		<div
			class="flex items-center gap-3 p-3 rounded-xl border {appHealth === 'Healthy'
				? 'border-green-200 dark:border-green-800 bg-green-50 dark:bg-green-900/20'
				: appHealth === 'Degraded'
					? 'border-yellow-200 dark:border-yellow-800 bg-yellow-50 dark:bg-yellow-900/20'
					: appHealth === 'Error'
						? 'border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-900/20'
						: 'border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50'}"
		>
			<span class="text-lg">
				{appHealth === 'Healthy'
					? '✅'
					: appHealth === 'Degraded'
						? '⚠️'
						: appHealth === 'Error'
							? '❌'
							: '⏳'}
			</span>
			<div>
				<div class="text-sm font-medium">
					{appHealth === 'Healthy'
						? 'All systems operational'
						: appHealth === 'Degraded'
							? 'Some services degraded'
							: appHealth === 'Error'
								? 'System error'
								: 'Starting up…'}
				</div>
				<div class="text-xs text-gray-500 dark:text-gray-400">
					{configuredConnectors.length} connector{configuredConnectors.length !== 1
						? 's'
						: ''} configured
				</div>
			</div>
		</div>

		<!-- Docker Status -->
		<div class="rounded-xl border border-gray-200 dark:border-gray-700 overflow-hidden">
			<div
				class="px-4 py-3 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700"
			>
				<div class="text-sm font-medium flex items-center gap-2">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="currentColor"
						class="size-4 text-blue-500"
					>
						<path
							d="M11.644 1.59a.75.75 0 0 1 .712 0l9.75 5.25a.75.75 0 0 1 0 1.32l-9.75 5.25a.75.75 0 0 1-.712 0l-9.75-5.25a.75.75 0 0 1 0-1.32l9.75-5.25Z"
						/>
						<path
							d="m3.265 10.602 7.668 4.129a2.25 2.25 0 0 0 2.134 0l7.668-4.13 1.37.739a.75.75 0 0 1 0 1.32l-9.75 5.25a.75.75 0 0 1-.71 0l-9.75-5.25a.75.75 0 0 1 0-1.32l1.37-.738Z"
						/>
						<path
							d="m10.933 19.231-7.668-4.13-1.37.739a.75.75 0 0 0 0 1.32l9.75 5.25c.221.12.489.12.71 0l9.75-5.25a.75.75 0 0 0 0-1.32l-1.37-.738-7.668 4.13a2.25 2.25 0 0 1-2.134-.001Z"
						/>
					</svg>
					Open WebUI Container
				</div>
			</div>
			<div class="px-4 py-3 flex items-center gap-3">
				<span class="text-base">{getDockerIcon(dockerStatus)}</span>
				<div>
					<div class="text-sm">{getDockerLabel(dockerStatus)}</div>
					<div class="text-xs text-gray-500 dark:text-gray-400">
						localhost:8080 • ghcr.io/open-webui/open-webui:main
					</div>
				</div>
			</div>
		</div>

		<!-- MCP Tool Servers -->
		<div class="rounded-xl border border-gray-200 dark:border-gray-700 overflow-hidden">
			<div
				class="px-4 py-3 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700"
			>
				<div class="text-sm font-medium flex items-center gap-2">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 20 20"
						fill="currentColor"
						class="size-4 text-purple-500"
					>
						<path d="M14 6H6v8h8V6z" />
						<path
							fill-rule="evenodd"
							d="M9.25 3V1.75a.75.75 0 0 1 1.5 0V3h1.5V1.75a.75.75 0 0 1 1.5 0V3h.5A2.75 2.75 0 0 1 17 5.75v.5h1.25a.75.75 0 0 1 0 1.5H17v1.5h1.25a.75.75 0 0 1 0 1.5H17v1.5h1.25a.75.75 0 0 1 0 1.5H17v.5A2.75 2.75 0 0 1 14.25 17h-.5v1.25a.75.75 0 0 1-1.5 0V17h-1.5v1.25a.75.75 0 0 1-1.5 0V17h-1.5v1.25a.75.75 0 0 1-1.5 0V17h-.5A2.75 2.75 0 0 1 3 14.25v-.5H1.75a.75.75 0 0 1 0-1.5H3v-1.5H1.75a.75.75 0 0 1 0-1.5H3v-1.5H1.75a.75.75 0 0 1 0-1.5H3v-.5A2.75 2.75 0 0 1 5.75 3h.5V1.75a.75.75 0 0 1 1.5 0V3h1.5zM4.5 5.75c0-.69.56-1.25 1.25-1.25h8.5c.69 0 1.25.56 1.25 1.25v8.5c0 .69-.56 1.25-1.25 1.25h-8.5c-.69 0-1.25-.56-1.25-1.25v-8.5z"
							clip-rule="evenodd"
						/>
					</svg>
					MCP Tool Servers
				</div>
			</div>

			<div class="divide-y divide-gray-100 dark:divide-gray-700/50">
				{#each ALL_SERVERS as connector}
					{@const status = serverStatuses[connector]}
					{@const meta = CONNECTOR_META[connector]}
					{@const configured = isConfigured(connector)}
					<div
						class="px-4 py-3 flex items-center justify-between {configured
							? ''
							: 'opacity-50'}"
					>
						<div class="flex items-center gap-3">
							<span class="text-lg">{meta?.emoji || '🔌'}</span>
							<div>
								<div class="text-sm font-medium capitalize">
									{connector}
									{#if !configured}
										<span class="text-xs font-normal text-gray-400 ml-1"
											>(not configured)</span
										>
									{/if}
								</div>
								<div class="text-xs text-gray-500 dark:text-gray-400">
									{meta?.description || 'MCP connector'}
								</div>
							</div>
						</div>
						<div class="flex items-center gap-3">
							{#if configured}
								<div class="text-xs text-gray-400 dark:text-gray-500 font-mono">
									:{status?.port || meta?.defaultPort || '?'}
								</div>
								<Tooltip content={getHealthLabel(status)}>
									<span class="text-sm cursor-default">{getHealthIcon(status)}</span>
								</Tooltip>
							{:else}
								<div
									class="text-xs text-gray-400 dark:text-gray-500 italic"
								>
									Add credentials in Settings
								</div>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		</div>

		<!-- Open WebUI Registered Tools -->
		<div class="rounded-xl border border-gray-200 dark:border-gray-700 overflow-hidden">
			<div
				class="px-4 py-3 bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700"
			>
				<div class="text-sm font-medium flex items-center gap-2">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 20 20"
						fill="currentColor"
						class="size-4 text-orange-500"
					>
						<path
							fill-rule="evenodd"
							d="M13.5 4.938a7 7 0 1 1-9.006 1.737c.202-.257.59-.218.793.039.278.352.594.672.943.954.332.269.786-.049.773-.476a5.977 5.977 0 0 1 .572-2.759 6.026 6.026 0 0 1 2.486-2.665c.247-.14.55-.016.677.238A6.967 6.967 0 0 0 13.5 4.938zM14 12a4.002 4.002 0 0 1-3.3 3.94c-.328.053-.657-.11-.752-.434a5.015 5.015 0 0 1-.234-1.506c0-1.578.732-2.987 1.874-3.903.2-.16.49-.103.628.118.404.647.638 1.405.638 2.22A2.5 2.5 0 0 1 14 12z"
							clip-rule="evenodd"
						/>
					</svg>
					Registered Tools in Open WebUI
				</div>
			</div>

			{#if toolsError}
				<div class="px-4 py-4 text-sm text-gray-500 dark:text-gray-400">
					<p class="text-xs">Could not fetch tools: {toolsError}</p>
					<p class="text-xs mt-1">
						This is expected if Open WebUI is still starting up or if auth is not yet
						initialized.
					</p>
				</div>
			{:else if registeredTools.length === 0}
				<div class="px-4 py-4 text-center text-sm text-gray-500 dark:text-gray-400">
					<p class="mb-1">No tools registered</p>
					<p class="text-xs">
						Tools from MCP servers appear here once Open WebUI discovers them. If
						connectors are configured and healthy above, the tools should be available
						when starting a new chat.
					</p>
				</div>
			{:else}
				<div class="divide-y divide-gray-100 dark:divide-gray-700/50">
					{#each registeredTools as tool}
						<div class="px-4 py-2.5 flex items-center justify-between">
							<div>
								<div class="text-sm font-medium">{tool.name || tool.id}</div>
								{#if tool.meta?.description}
									<div
										class="text-xs text-gray-500 dark:text-gray-400 truncate max-w-md"
									>
										{tool.meta.description}
									</div>
								{/if}
							</div>
							<div class="text-xs text-gray-400 dark:text-gray-500 font-mono">
								{tool.id || ''}
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Help text -->
		<div class="text-xs text-gray-400 dark:text-gray-500 space-y-1 mt-1">
			<p>
				<strong>How it works:</strong> IBEX runs MCP tool servers as local Node.js processes.
				These are connected to Open WebUI via the
				<code class="text-xs">TOOL_SERVER_CONNECTIONS</code> environment variable on the Docker
				container.
			</p>
			<p>
				<strong>Restart Servers</strong> stops all MCP servers, updates the Docker container
				configuration, rebuilds the system prompt, and restarts everything. Use this after
				changing connector credentials in Settings.
			</p>
			<p>
				🟢 Healthy = responding to health checks. 🟡 Starting = process spawned but not yet
				healthy. 🔴 Stopped = process not running. ⚪ Not started = server wasn't launched.
			</p>
		</div>
	{/if}
</div>
