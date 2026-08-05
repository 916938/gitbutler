import { classes } from "#ui/components/classes.ts";
import type { CSSProperties, FC, ReactNode } from "react";
import { useLayoutEffect, useRef, useState } from "react";
import styles from "./Clamped.module.css";

/**
 * Caps content at `maxHeight` with a fade and a Show more/less toggle.
 * The toggle and fade only appear when content is actually cut off, so
 * borderline-height content doesn't get its last line dimmed.
 */
export const Clamped: FC<{
	/** Any CSS length, e.g. `"240px"` or `"80vh"`. */
	maxHeight: string;
	/** Re-measure when this changes; pass the content's source data. */
	measureKey: unknown;
	children: ReactNode;
}> = ({ maxHeight, measureKey, children }) => {
	const [expanded, setExpanded] = useState(false);
	const [overflows, setOverflows] = useState(false);
	const bodyRef = useRef<HTMLDivElement | null>(null);

	// Detect whether the clamp actually hides content. Not re-measured on
	// resize — a width change rarely flips the outcome at these heights.
	useLayoutEffect(() => {
		if (expanded) return;
		const el = bodyRef.current;
		if (el) setOverflows(el.scrollHeight > el.clientHeight + 1);
	}, [measureKey, expanded]);

	return (
		<>
			<div
				ref={bodyRef}
				style={{ "--clamp-max-height": maxHeight } as CSSProperties}
				className={classes(
					!expanded && styles.clamped,
					!expanded && overflows && styles.overflowing,
				)}
			>
				{children}
			</div>
			{(overflows || expanded) && (
				<button
					className={classes("text-12", styles.toggle)}
					onClick={() => setExpanded(!expanded)}
					type="button"
				>
					{expanded ? "Show less" : "Show more"}
				</button>
			)}
		</>
	);
};
