<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';

	const NON_DRAGGING_TAGS = [
		'INPUT',
		'TEXTAREA',
		'BUTTON',
		'SELECT',
		'A',
		'VIDEO',
		'AUDIO',
		'IMG',
		'CANVAS',
		'IFRAME',
		'SVG',
		'P',
		'SPAN',
		'H1',
		'H2',
		'H3',
		'H4',
		'H5',
		'H6',
		'PRE',
		'CODE',
		'SUMMARY',
		'DETAILS',
		'OL',
		'UL',
		'LI',
		'DD',
		'DT'
	];

	const hasOnlyRawText = (element: HTMLElement) => {
		// Check if all child nodes are text nodes and not empty
		return Array.from(element.childNodes).every(
			(node) => node.nodeType === Node.TEXT_NODE && node.textContent?.trim() !== ''
		);
	};

	// Check if an element or any of its ancestors (up to 10 levels) is non-draggable.
	// This prevents window dragging when clicking on a <div> inside a <button> or <a>.
	const isInsideNonDraggable = (el: HTMLElement): boolean => {
		let current: HTMLElement | null = el;
		let depth = 0;
		while (current && depth < 10) {
			if (NON_DRAGGING_TAGS.includes(current.tagName?.toUpperCase())) {
				return true;
			}
			current = current.parentElement;
			depth++;
		}
		return false;
	};

	const onPointerDown = async (event: PointerEvent) => {
		if (!(event?.target instanceof HTMLElement)) {
			console.debug('Pointer down on non-HTMLElement', event?.target);
			return;
		} else if (event.button !== 0) {
			console.debug('Pointer down with non-left mouse button');
			return;
		}

		// Check if the target or any ancestor is a non-draggable element.
		// This ensures clicks on <div> children inside <button> or <a> tags
		// are not captured as window drag events.
		if (isInsideNonDraggable(event.target)) {
			console.debug('Pointer down on or inside non-draggable element');
			return;
		} else if (
			event.target?.tagName === 'DIV' &&
			(event.target.hasAttribute('contenteditable') || hasOnlyRawText(event.target))
		) {
			console.debug('Pointer down on editable or text-containing element');
			return;
		} else if (false) {
			// TODO else if is transparent, rgba 0 0 0 0 or has no background set whatsoever
			console.debug('Pointer down on invisible element');
			return;
		}
		await getCurrentWindow().startDragging();
	};
</script>

<svelte:window on:pointerdown={onPointerDown} />
