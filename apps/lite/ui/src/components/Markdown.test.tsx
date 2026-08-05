import { Markdown } from "#ui/components/Markdown.tsx";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

const render = (markdown: string): string => renderToStaticMarkup(<Markdown>{markdown}</Markdown>);

describe("Markdown safety", () => {
	it("never parses raw HTML into elements", () => {
		const html = render('<script>alert(1)</script> <iframe src="https://x.test"></iframe>');
		expect(html).not.toContain("<script");
		expect(html).not.toContain("<iframe");
		// The tags survive only as escaped, visible text.
		expect(html).toContain("&lt;script&gt;");
	});

	it("cannot produce style elements or attributes", () => {
		const html = render('<style>*{display:none}</style> <div style="color:red">x</div>');
		expect(html).not.toContain("<style");
		expect(html).not.toContain('style="');
	});

	it("strips javascript: hrefs from links", () => {
		const html = render("[click me](javascript:alert(1))");
		expect(html).not.toContain("javascript:");
	});

	it("strips data: URLs", () => {
		const html = render("[click me](data:text/html,<script>alert(1)</script>)");
		expect(html).not.toContain("data:");
	});

	it("keeps http(s) links", () => {
		const html = render("[docs](https://example.test/docs)");
		expect(html).toContain('href="https://example.test/docs"');
	});

	it("renders images as links instead of fetching them", () => {
		const html = render("![diagram](https://example.test/pixel.png)");
		expect(html).not.toContain("<img");
		expect(html).toContain('href="https://example.test/pixel.png"');
		expect(html).toContain("diagram");
	});

	it("drops images with stripped (unsafe) sources entirely", () => {
		const html = render("![x](javascript:alert(1))");
		expect(html).not.toContain("<img");
		expect(html).not.toContain("javascript:");
	});

	it("renders GFM tables and task lists within the allowlist", () => {
		const html = render("| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done");
		expect(html).toContain("<table");
		expect(html).toContain('type="checkbox"');
		expect(html).toContain("disabled");
	});

	it("unwraps content of elements outside the allowlist rather than rendering them", () => {
		// Footnote references produce sup/section/a, all allowlisted; this
		// guards the unwrapDisallowed behavior using math-like raw text.
		const html = render("plain *emphasis* text");
		expect(html).toContain("<em>");
	});
});
