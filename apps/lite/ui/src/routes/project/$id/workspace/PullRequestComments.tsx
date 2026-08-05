import { useCreateReviewComment } from "#ui/api/mutations.ts";
import {
	listReviewCommentsQueryOptions,
	listReviewSubmissionsQueryOptions,
} from "#ui/api/queries.ts";
import { getButtonClassName } from "#ui/components/Button.tsx";
import { Clamped } from "#ui/components/Clamped.tsx";
import { classes } from "#ui/components/classes.ts";
import { FieldTextareaStyles } from "#ui/components/Field.tsx";
import { Icon } from "#ui/components/Icon.tsx";
import { Markdown } from "#ui/components/Markdown.tsx";
import { ReviewUser } from "#ui/routes/project/$id/workspace/PullRequestPanel.tsx";
import { formatRelativeTime } from "#ui/time.ts";
import type { ForgeReview, ForgeReviewComment, ForgeReviewSubmission } from "@gitbutler/but-sdk";
import { useQuery } from "@tanstack/react-query";
import { type FC, useState } from "react";
import styles from "./PullRequestComments.module.css";

const Comment: FC<{ comment: ForgeReviewComment }> = ({ comment }) => {
	const createdAtMs = comment.createdAt === null ? null : Date.parse(comment.createdAt);

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
			<Clamped maxHeight="240px" measureKey={comment.body}>
				<Markdown>{comment.body}</Markdown>
			</Clamped>
		</div>
	);
};

const submissionVerdictText: Record<ForgeReviewSubmission["state"], string> = {
	approved: "approved these changes",
	changesRequested: "requested changes",
	commented: "reviewed",
	dismissed: "had their review dismissed",
};

const Submission: FC<{ submission: ForgeReviewSubmission }> = ({ submission }) => {
	const submittedAtMs = submission.submittedAt === null ? null : Date.parse(submission.submittedAt);

	return (
		<div className={styles.event}>
			<div className={styles.commentMeta}>
				{submission.author !== null && <ReviewUser user={submission.author} />}
				<span className={classes("text-12", styles.eventText)}>
					{submissionVerdictText[submission.state]}
				</span>
				{submittedAtMs !== null && (
					<span className={classes("text-12", styles.commentTime)}>
						{formatRelativeTime(submittedAtMs)}
					</span>
				)}
			</div>
			{submission.body !== null && (
				<div className={styles.eventBody}>
					<Markdown>{submission.body}</Markdown>
				</div>
			)}
		</div>
	);
};

type TimelineItem =
	| { kind: "opened"; at: number; review: ForgeReview }
	| { kind: "comment"; at: number; comment: ForgeReviewComment }
	| { kind: "submission"; at: number; submission: ForgeReviewSubmission };

const parseTimestamp = (value: string | null): number => {
	if (value === null) return 0;
	const ms = Date.parse(value);
	return Number.isNaN(ms) ? 0 : ms;
};

const timelineItems = (
	review: ForgeReview,
	comments: Array<ForgeReviewComment> | undefined,
	submissions: Array<ForgeReviewSubmission> | undefined,
): Array<TimelineItem> => {
	const items: Array<TimelineItem> = [];
	if (review.createdAt !== null)
		items.push({ kind: "opened", at: parseTimestamp(review.createdAt), review });
	for (const comment of comments ?? [])
		items.push({ kind: "comment", at: parseTimestamp(comment.createdAt), comment });
	for (const submission of submissions ?? [])
		items.push({ kind: "submission", at: parseTimestamp(submission.submittedAt), submission });
	return items.sort((a, b) => a.at - b.at);
};

export const PullRequestComments: FC<{ projectId: string; review: ForgeReview }> = ({
	projectId,
	review,
}) => {
	const reviewId = review.number;
	const { data: comments, isPending } = useQuery(
		listReviewCommentsQueryOptions({ projectId, reviewId }),
	);
	const { data: submissions } = useQuery(
		listReviewSubmissionsQueryOptions({ projectId, reviewId }),
	);
	const { isPending: isPosting, mutate: createReviewComment } = useCreateReviewComment();
	const [draft, setDraft] = useState("");

	const handleSubmit = () => {
		const body = draft.trim();
		if (body === "" || isPosting) return;
		createReviewComment({ projectId, reviewId, body }, { onSuccess: () => setDraft("") });
	};

	const items = timelineItems(review, comments, submissions);

	return (
		<div className={styles.comments}>
			<h4 className={classes("text-14", styles.commentsHeading)}>Activity</h4>

			{isPending ? (
				<div className={classes("text-13", styles.commentsEmpty)}>Loading…</div>
			) : (
				<div className={styles.commentList}>
					{items.map((item) =>
						item.kind === "opened" ? (
							<div key="opened" className={styles.event}>
								<div className={styles.commentMeta}>
									{item.review.author !== null && <ReviewUser user={item.review.author} />}
									<span className={classes("text-12", styles.eventText)}>
										opened this pull request
									</span>
									<span className={classes("text-12", styles.commentTime)}>
										{formatRelativeTime(item.at)}
									</span>
								</div>
							</div>
						) : item.kind === "comment" ? (
							<Comment key={`comment-${item.comment.id}`} comment={item.comment} />
						) : (
							<Submission key={`submission-${item.submission.id}`} submission={item.submission} />
						),
					)}
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
