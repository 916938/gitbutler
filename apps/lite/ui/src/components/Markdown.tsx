import { classes } from "#ui/components/classes.ts";
import { Icon } from "#ui/components/Icon.tsx";
import type { FC, MouseEvent } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
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
 * Renders forge-flavored markdown with GitHub-or-stricter restrictions:
 *
 * - Raw HTML is never parsed (no `rehype-raw`), so no scripts, styles, or
 *   `style=` attributes exist — GitHub sanitizes a tag subset, we allow none.
 * - Elements are pinned to {@link allowedElements}.
 * - URLs pass react-markdown's default transform (`javascript:`/`data:`/
 *   `file:` are stripped); links additionally only open via the system
 *   browser, and the Electron shell blocks all in-app navigation.
 * - Images are never fetched (the CSP has no remote `img-src`); they render
 *   as links instead, like GitHub's camo proxy but with zero requests.
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
