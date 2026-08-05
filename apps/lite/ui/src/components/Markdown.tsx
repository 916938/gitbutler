import { classes } from "#ui/components/classes.ts";
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
 * Renders forge-flavored markdown. Raw HTML is not rendered (react-markdown
 * drops it), and links open in the system browser instead of navigating the
 * app.
 */
export const Markdown: FC<{ children: string }> = ({ children }) => (
	<div className={classes("text-13", "text-body", styles.markdown)}>
		<ReactMarkdown
			remarkPlugins={[remarkGfm]}
			components={{
				// oxlint-disable-next-line jsx-a11y/anchor-has-content, jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- href and children arrive via the spread; it stays a real anchor.
				a: ({ node: _node, ...props }) => <a {...props} onClick={openExternally} />,
			}}
		>
			{children}
		</ReactMarkdown>
	</div>
);
