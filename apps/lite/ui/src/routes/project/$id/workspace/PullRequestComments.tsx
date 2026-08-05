import { useCreateReviewComment } from "#ui/api/mutations.ts";
import { listReviewCommentsQueryOptions } from "#ui/api/queries.ts";
import { getButtonClassName } from "#ui/components/Button.tsx";
import { classes } from "#ui/components/classes.ts";
import { FieldTextareaStyles } from "#ui/components/Field.tsx";
import { Icon } from "#ui/components/Icon.tsx";
import { Markdown } from "#ui/components/Markdown.tsx";
import { ReviewUser } from "#ui/routes/project/$id/workspace/PullRequestPanel.tsx";
import { formatRelativeTime } from "#ui/time.ts";
import type { ForgeReviewComment } from "@gitbutler/but-sdk";
import { useQuery } from "@tanstack/react-query";
import { type FC, useLayoutEffect, useRef, useState } from "react";
import styles from "./PullRequestComments.module.css";

const Comment: FC<{ comment: ForgeReviewComment }> = ({ comment }) => {
	const createdAtMs = comment.createdAt === null ? null : Date.parse(comment.createdAt);

	const [expanded, setExpanded] = useState(false);
	const [overflows, setOverflows] = useState(false);
	const bodyRef = useRef<HTMLDivElement | null>(null);

	// Detect whether the clamped body actually hides content; the toggle only
	// shows when it does. Re-measured when the body changes, not on resize —
	// a width change rarely flips the outcome at this height.
	useLayoutEffect(() => {
		if (expanded) return;
		const el = bodyRef.current;
		if (el) setOverflows(el.scrollHeight > el.clientHeight + 1);
	}, [comment.body, expanded]);

	return (
		<div className={styles.comment}>
			<div className={styles.commentMeta}>
				{comment.author !== null && <ReviewUser user={comment.author} />}
				{createdAtMs !== null && (
					<span className={classes("text-12", styles.commentTime)}>
						{formatRelativeTime(createdAtMs)}
					</span>
				)}
			</div>
			<div
				ref={bodyRef}
				className={classes(
					styles.commentBody,
					!expanded && styles.commentBodyClamped,
					!expanded && overflows && styles.commentBodyOverflowing,
				)}
			>
				<Markdown>{comment.body}</Markdown>
			</div>
			{(overflows || expanded) && (
				<button
					className={classes("text-12", styles.commentExpand)}
					onClick={() => setExpanded(!expanded)}
					type="button"
				>
					{expanded ? "Show less" : "Show more"}
				</button>
			)}
		</div>
	);
};

export const PullRequestComments: FC<{ projectId: string; reviewId: number }> = ({
	projectId,
	reviewId,
}) => {
	const { data: comments, isPending } = useQuery(
		listReviewCommentsQueryOptions({ projectId, reviewId }),
	);
	const { isPending: isPosting, mutate: createReviewComment } = useCreateReviewComment();
	const [draft, setDraft] = useState("");

	const handleSubmit = () => {
		const body = draft.trim();
		if (body === "" || isPosting) return;
		createReviewComment({ projectId, reviewId, body }, { onSuccess: () => setDraft("") });
	};

	return (
		<div className={styles.comments}>
			<h4 className={classes("text-14", styles.commentsHeading)}>
				Comments
				{comments !== undefined && comments.length > 0 && ` (${comments.length})`}
			</h4>

			{isPending ? (
				<div className={classes("text-13", styles.commentsEmpty)}>Loading…</div>
			) : comments === undefined || comments.length === 0 ? (
				<div className={classes("text-13", styles.commentsEmpty)}>No comments yet.</div>
			) : (
				<div className={styles.commentList}>
					{comments.map((comment) => (
						<Comment key={comment.id} comment={comment} />
					))}
				</div>
			)}

			<div className={styles.composer}>
				<FieldTextareaStyles
					placeholder="Write a comment…"
					value={draft}
					onChange={(evt) => setDraft(evt.currentTarget.value)}
					disabled={isPosting}
				/>
				<div className={styles.composerActions}>
					<button
						className={getButtonClassName({ variant: "pop" })}
						disabled={isPosting || draft.trim() === ""}
						onClick={handleSubmit}
						type="button"
					>
						{isPosting && <Icon name="spinner" />}
						Comment
					</button>
				</div>
			</div>
		</div>
	);
};
