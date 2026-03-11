// Helper function to find matching closing tag
// openTag can be a prefix like '<details' to match '<details>' and '<details attr="...">'
function findMatchingClosingTag(src, openTag, closeTag) {
	let depth = 1;
	// Skip past the first opening tag
	const firstClose = src.indexOf('>');
	let index = firstClose + 1;
	while (depth > 0 && index < src.length) {
		if (src.startsWith(closeTag, index)) {
			depth--;
			if (depth === 0) break;
			index += closeTag.length;
		} else if (src.startsWith(openTag, index) && !src.startsWith(closeTag, index)) {
			// Nested open tag (but not a closing tag that shares the prefix)
			depth++;
			index++;
		} else {
			index++;
		}
	}
	return depth === 0 ? index + closeTag.length : -1;
}

function detailsTokenizer(src) {
	// Match <details> with optional attributes (e.g. <details type="reasoning" done="true">)
	const detailsRegex = /^<details(\s[^>]*)?>(\n|$)/;
	const summaryRegex = /^<summary>(.*?)<\/summary>\n/;

	const openMatch = detailsRegex.exec(src);
	if (openMatch) {
		const endIndex = findMatchingClosingTag(src, '<details', '</details>');
		if (endIndex === -1) return;

		const fullMatch = src.slice(0, endIndex);
		// Remove opening tag (variable length) and closing </details>
		const openTagEnd = src.indexOf('>') + 1;
		let content = fullMatch.slice(openTagEnd, -10).trim(); // -10 for </details>

		// Parse attributes from the opening tag
		const attrs = openMatch[1] || '';
		const typeMatch = attrs.match(/type="([^"]*)"/);
		const isThinking = typeMatch && typeMatch[1] === 'reasoning';

		let summary = '';
		const summaryMatch = summaryRegex.exec(content);
		if (summaryMatch) {
			summary = summaryMatch[1].trim();
			content = content.slice(summaryMatch[0].length).trim();
		}

		// For thinking/reasoning blocks, set a default summary
		if (isThinking && !summary) {
			summary = 'Thinking';
		}

		return {
			type: 'details',
			raw: fullMatch,
			summary: summary,
			text: content,
			isThinking: isThinking
		};
	}
}

function detailsStart(src) {
	return src.match(/^<details[\s>]/) ? 0 : -1;
}

function detailsRenderer(token) {
	return `<details>
  ${token.summary ? `<summary>${token.summary}</summary>` : ''}
  ${token.text}
  </details>`;
}

function detailsExtension() {
	return {
		name: 'details',
		level: 'block',
		start: detailsStart,
		tokenizer: detailsTokenizer,
		renderer: detailsRenderer
	};
}

export default function (options = {}) {
	return {
		extensions: [detailsExtension(options)]
	};
}
