<script lang="ts">
	import { goto } from '$app/navigation';
	import { APP_STORE_FILE } from '$lib/app/constants';
	import { WEBUI_BASE_URL } from '$lib/stores';
	import { getStore } from '@tauri-apps/plugin-store';
	import type { i18n as i18nType } from 'i18next';
	import { getContext, onMount } from 'svelte';
	import type { Writable } from 'svelte/store';

	const i18n: Writable<i18nType> = getContext('i18n');

	console.debug('On setup page');

	let baseUrl = '';
	const onKeyDown = async (e: KeyboardEvent) => {
		if (e.key === 'Enter') {
			revealSplashScreen();
			$WEBUI_BASE_URL = baseUrl;
		}
	};

	$: if ($WEBUI_BASE_URL) {
		goto('/', { replaceState: true });
	}

	let splashScreen: HTMLElement | null;
	// Function to reveal the splash screen later
	const revealSplashScreen = () => {
		if (splashScreen) {
			splashScreen.style.display = ''; // Reset display to original
			splashScreen.dataset.visible = 'true'; // Update visibility state
		}
	};

	$: $i18n.t('Sign in');

	onMount(async () => {
		console.debug('SETUP PAGE MOUNTED');
		const store = await getStore(APP_STORE_FILE);
		for (const key of (await store?.keys()) || []) {
			if (key !== 'app_config' && key !== 'webui_base_url') {
				await store?.delete(key);
			}
		}
		await store?.save();
		console.log(await store?.entries());
		// Hide splash screen, since we haven't loaded yet we'll need to put it back later
		splashScreen = document.getElementById('splash-screen');
		if (splashScreen) {
			splashScreen.style.display = 'none'; // Hide the element
			splashScreen.dataset.visible = 'false'; // Store visibility state
		}
	});
</script>

<div class="w-full h-screen max-h-[100dvh] relative bg-white dark:bg-gray-900">
	<!-- <div class="w-full h-full absolute top-0 left-0 bg-white dark:bg-black z-[-1]"></div> -->

	<div class="w-full h-full flex items-center justify-center">
		<div class="w-full sm:max-w-md px-10 min-h-screen flex flex-col text-center">
			<div class="my-auto pb-10 w-full dark:text-gray-100">
				<h1 class="text-3xl font-bold mb-8 text-gray-900 dark:text-white">
					Welcome to IBEX
				</h1>

				<form
					class="flex flex-col justify-center space-y-4"
					on:submit|preventDefault={() => {
						revealSplashScreen();
						$WEBUI_BASE_URL = baseUrl;
					}}
				>
					<div class="text-center text-gray-600 dark:text-gray-400 mb-4">
						Please enter the base URL of your IBEX instance to get started
					</div>

					<div class="space-y-1">
						<div class=" text-sm font-medium text-left mb-1">
							WebUI Base URL
							<!-- {$i18n.t('WebUI Base URL')} -->
						</div>
						<input
							bind:value={baseUrl}
							autocorrect="off"
							autocomplete="url"
							spellcheck="false"
							on:keydown={onKeyDown}
							placeholder="http://localhost:3000"
							class="my-0.5 w-full text-sm outline-none bg-transparent"
						/>
					</div>

					<button
						class="bg-gray-700/5 hover:bg-gray-700/10 dark:bg-gray-100/5 dark:hover:bg-gray-100/10 dark:text-gray-300 dark:hover:text-white transition w-full rounded-full font-medium text-sm py-2.5"
						type="submit"
					>
						<!-- {$i18n.t('Get Started')} -->
						Get Started
					</button>
				</form>
			</div>
		</div>
	</div>
</div>
