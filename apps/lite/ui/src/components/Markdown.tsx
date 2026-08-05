import { guiSettingsQueryOptions } from "#ui/api/queries.ts";
import { classes } from "#ui/components/classes.ts";
import { Icon } from "#ui/components/Icon.tsx";
import { defaultSettings } from "#ui/settings.ts";
import { useQuery } from "@tanstack/react-query";
import type { CSSProperties, FC, MouseEvent } from "react";
import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { BundledLanguage, ThemedToken } from "shiki";
import styles from "./Markdown.module.css";

const openExternally = (evt: MouseEvent<HTMLAnchorElement>): void => {
	evt.preventDefault();
	const url = evt.currentTarget.href;
	if (url.startsWith("http://") || url.startsWith("https://"))
		void window.lite.openInWebBrowser(url);
};

/**
 * Every element the markdown/GFM grammar can produce. Anything outside this
 * list is unwrapped to its children, so a plugin change can never widen what
 * reaches the DOM without this contract changing too.
 */
const allowedElements = [
	"a",
	"blockquote",
	"br",
	"code",
	"del",
	"em",
	"h1",
	"h2",
	"h3",
	"h4",
	"h5",
	"h6",
	"hr",
	"img",
	"input",
	"li",
	"ol",
	"p",
	"pre",
	"section",
	"strong",
	"sup",
	"table",
	"tbody",
	"td",
	"th",
	"thead",
	"tr",
	"ul",
];

/**
 * Fenced code with a language tag, highlighted through shiki's token API.
 * Tokens render as React spans — never HTML strings — so the no-innerHTML
 * property of this component is preserved. Colors come out as CSS variables
 * resolved with `light-dark()`, matching the app's theming, using the same
 * theme pair as the diff viewer. Unknown languages fall back to plain text.
 */
const CodeBlock: FC<{ language: string; code: string }> = ({ language, code }) => {
	const { data: themeCfg } = useQuery({
		...guiSettingsQueryOptions,
		select: (cfg) => cfg.syntaxHighlighting,
	});
	const light = themeCfg?.light ?? defaultSettings.syntaxHighlighting.light;
	const dark = themeCfg?.dark ?? defaultSettings.syntaxHighlighting.dark;

	const [tokens, setTokens] = useState<Array<Array<ThemedToken>> | null>(null);

	useEffect(() => {
		const effect = { cancelled: false };
		void (async () => {
			try {
				const { codeToTokens } = await import("shiki");
				const result = await codeToTokens(code, {
					// Invalid names reject and we keep the plain fallback.
					lang: language as BundledLanguage,
					themes: { light, dark },
					defaultColor: false,
					cssVariablePrefix: "--shiki-",
				});
				if (!effect.cancelled) setTokens(result.tokens);
			} catch {
				if (!effect.cancelled) setTokens(null);
			}
		})();
		return () => {
			effect.cancelled = true;
		};
	}, [code, language, light, dark]);

	if (tokens === null) return <code>{code}</code>;

	return (
		<code className={styles.highlighted}>
			{tokens.map((line, lineIdx) => (
				// Lines are positional; there is no stable identity to key on.
				// oxlint-disable-next-line react/no-array-index-key
				<span key={lineIdx}>
					{line.map((token, tokenIdx) => (
						// oxlint-disable-next-line react/no-array-index-key
						<span key={tokenIdx} style={token.htmlStyle as CSSProperties | undefined}>
							{token.content}
						</span>
					))}
					{"\n"}
				</span>
			))}
		</code>
	);
};

const fencedLanguage = (className: string | undefined): string | undefined =>
	/language-([\w+#-]+)/.exec(className ?? "")?.[1];

/**
 * Renders forge-flavored markdown with GitHub-or-stricter restrictions:
 *
 * - Raw HTML is never parsed (no `rehype-raw`), so no scripts, styles, or
 *   `style=` attributes exist — GitHub sanitizes a tag subset, we allow none.
 * - Elements are pinned to {@link allowedElements}.
 * - URLs pass react-markdown's default transform (`javascript:`/`data:`/
 *   `file:` are stripped); links additionally only open via the system
 *   browser, and the Electron shell blocks all in-app navigation.
 * - Markdown images are never fetched: the `img` override below renders a
 *   link, so no `<img>` element is ever created (zero requests — like
 *   GitHub's camo proxy, minus the proxy).
 */
export const Markdown: FC<{ children: string }> = ({ children }) => (
	<div className={classes("text-13", "text-body", styles.markdown)}>
		<ReactMarkdown
			remarkPlugins={[remarkGfm]}
			allowedElements={allowedElements}
			unwrapDisallowed
			components={{
				// oxlint-disable-next-line jsx-a11y/anchor-has-content, jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- href and children arrive via the spread; it stays a real anchor.
				a: ({ node: _node, ...props }) => <a {...props} onClick={openExternally} />,
				code: ({ node: _node, className, children, ...props }) => {
					const language = fencedLanguage(className);
					return language !== undefined && typeof children === "string" ? (
						<CodeBlock language={language} code={children.replace(/\n$/, "")} />
					) : (
						<code className={className} {...props}>
							{children}
						</code>
					);
				},
				img: ({ node: _node, src, alt }) =>
					typeof src === "string" && src !== "" ? (
						<a href={src} onClick={openExternally} className={styles.imageLink}>
							<Icon name="paperclip" />
							{typeof alt === "string" && alt !== "" ? alt : "image"}
						</a>
					) : null,
			}}
		>
			{children}
		</ReactMarkdown>
	</div>
);
