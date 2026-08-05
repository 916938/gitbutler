import {
	useAddReviewLabels,
	useRemoveReviewLabel,
	useRequestReview,
	useWithdrawReviewRequest,
} from "#ui/api/mutations.ts";
import {
	forgeInfoOptions,
	listCIChecksQueryOptions,
	listReviewSubmissionsQueryOptions,
	repoLabelsQueryOptions,
	reviewerCandidatesQueryOptions,
} from "#ui/api/queries.ts";
import { Badge, type BadgeVariant } from "#ui/components/Badge.tsx";
import { getButtonClassName } from "#ui/components/Button.tsx";
import { classes } from "#ui/components/classes.ts";
import { Icon } from "#ui/components/Icon.tsx";
import type { IconName } from "#ui/components/iconNames.ts";
import { nativeMenuItem, showNativeMenuFromTrigger } from "#ui/native-menu.ts";
import { formatRelativeTime } from "#ui/time.ts";
import type {
	ForgeReview,
	ForgeReviewLabel,
	ForgeReviewSubmission,
	ForgeReviewUser,
} from "@gitbutler/but-sdk";
import { useQuery } from "@tanstack/react-query";
import { Match } from "effect";
import type { FC, MouseEvent, ReactNode } from "react";
import styles from "./PullRequestPanel.module.css";

type ReviewStatus = "open" | "draft" | "merged" | "closed";

const reviewStatus = (review: ForgeReview): ReviewStatus =>
	review.mergedAt !== null
		? "merged"
		: review.closedAt !== null
			? "closed"
			: review.draft
				? "draft"
				: "open";

const absoluteDate = new Intl.DateTimeFormat(undefined, {
	dateStyle: "medium",
	timeStyle: "short",
});

const Section: FC<{ heading: string; action?: ReactNode; children: ReactNode }> = (p) => (
	<div className={styles.section}>
		<div className={styles.sectionHeader}>
			<h4 className={classes("text-13", styles.heading)}>{p.heading}</h4>
			{p.action}
		</div>
		{p.children}
	</div>
);

export const ReviewUser: FC<{ user: ForgeReviewUser }> = ({ user }) => (
	<div className={classes("text-13", styles.user)} title={user.name ?? undefined}>
		{user.avatarUrl !== null ? (
			<img src={user.avatarUrl} className={styles.avatar} alt="" />
		) : (
			<span className={styles.avatar} />
		)}
		{user.login}
	</div>
);

const Label: FC<{ label: ForgeReviewLabel }> = ({ label }) => {
	// GitHub sends bare hex color codes, GitLab prefixes them with `#`.
	const color =
		label.color === null ? null : label.color.startsWith("#") ? label.color : `#${label.color}`;

	return (
		<span className={classes("text-12", styles.label)} title={label.description ?? undefined}>
			{color !== null && <span className={styles.labelDot} style={{ backgroundColor: color }} />}
			{label.name}
		</span>
	);
};

const openLinkExternally = (evt: MouseEvent<HTMLAnchorElement>): void => {
	evt.preventDefault();
	void window.lite.openInWebBrowser(evt.currentTarget.href);
};

/** Compact CI summary for the panel: one line, plus links for problem jobs. */
const ChecksSection: FC<{ projectId: string; reference: string }> = ({ projectId, reference }) => {
	const { data } = useQuery(
		listCIChecksQueryOptions({ projectId, reference, polling: "priority" }),
	);
	const aggregate = data?.aggregate ?? null;
	if (aggregate === null) return null;

	const passed = aggregate.success.length + aggregate.neutral.length + aggregate.skipped.length;
	const [summary, icon, color] = Match.value(aggregate.status).pipe(
		Match.withReturnType<[string, IconName, string]>(),
		Match.when("success", () => [
			`${passed}/${aggregate.total} passed`,
			"tick-circle",
			"var(--scale-safe-50)",
		]),
		Match.when("failure", () => [
			`${aggregate.failure.length + aggregate.timedOut.length}/${aggregate.total} failed`,
			"cross-circle",
			"var(--scale-danger-50)",
		]),
		Match.when("cancelled", () => ["Some cancelled", "cross-circle", "var(--scale-danger-50)"]),
		Match.when("action_required", () => ["Action required", "warning", "var(--scale-warn-50)"]),
		Match.when("in_progress", () => ["In progress", "spinner", "var(--text-3)"]),
		Match.when("unknown", () => ["Unknown", "question-circle", "var(--text-3)"]),
		Match.exhaustive,
	);

	const problemChecks = [...aggregate.failure, ...aggregate.timedOut, ...aggregate.actionRequired];

	return (
		<Section heading="Checks">
			<div className={classes("text-13", styles.checksSummary)}>
				<Icon name={icon} style={{ color }} />
				{summary}
			</div>

			{problemChecks.length > 0 && (
				<div className={styles.checksProblems}>
					{problemChecks.map((check) => (
						<a
							key={check.id}
							href={check.htmlUrl}
							onClick={openLinkExternally}
							className={classes("text-12", styles.link)}
						>
							{check.name}
						</a>
					))}
				</div>
			)}
		</Section>
	);
};

/** Dismissals collapse to "commented", so they never appear as a verdict. */
type ReviewerVerdict = "approved" | "changesRequested" | "commented" | "awaiting";

type ReviewerRow = { user: ForgeReviewUser; verdict: ReviewerVerdict };

/**
 * One row per reviewer: everyone still requested (awaiting) plus everyone
 * who submitted a review, carrying their effective verdict. A comment-only
 * submission never overrides an earlier approval or change request, and a
 * dismissal drops the verdict back to commented.
 */
const reviewerRows = (
	requested: Array<ForgeReviewUser>,
	submissions: Array<ForgeReviewSubmission>,
): Array<ReviewerRow> => {
	const byLogin = new Map<string, ReviewerRow>();
	for (const submission of submissions) {
		if (submission.author === null) continue;
		const existing = byLogin.get(submission.author.login);
		const verdict = Match.value(submission.state).pipe(
			Match.withReturnType<ReviewerVerdict>(),
			Match.when("approved", () => "approved"),
			Match.when("changesRequested", () => "changesRequested"),
			Match.when("commented", () => existing?.verdict ?? "commented"),
			Match.when("dismissed", () => "commented"),
			Match.exhaustive,
		);
		byLogin.set(submission.author.login, { user: submission.author, verdict });
	}
	for (const user of requested)
		if (!byLogin.has(user.login)) byLogin.set(user.login, { user, verdict: "awaiting" });
	return [...byLogin.values()];
};

const verdictBits = (verdict: ReviewerVerdict): [IconName, string, string] =>
	Match.value(verdict).pipe(
		Match.withReturnType<[IconName, string, string]>(),
		Match.when("approved", () => ["tick-circle", "var(--scale-safe-50)", "Approved"]),
		Match.when("changesRequested", () => [
			"cross-circle",
			"var(--scale-danger-50)",
			"Requested changes",
		]),
		Match.when("commented", () => ["eye", "var(--text-3)", "Commented"]),
		Match.when("awaiting", () => ["clock", "var(--text-3)", "Awaiting review"]),
		Match.exhaustive,
	);

export const PullRequestPanel: FC<{ projectId: string; review: ForgeReview }> = ({
	projectId,
	review,
}) => {
	const { data: reviewers } = useQuery({
		...listReviewSubmissionsQueryOptions({ projectId, reviewId: review.number }),
		select: (submissions) => reviewerRows(review.reviewers, submissions),
	});
	// Until submissions load, show the requested reviewers without verdicts.
	const reviewerList =
		reviewers ?? review.reviewers.map((user): ReviewerRow => ({ user, verdict: "awaiting" }));

	// Label/reviewer management is GitHub-only for now; the pickers only
	// appear once their option lists have loaded.
	const { data: forgeInfo } = useQuery(forgeInfoOptions(projectId));
	const canManage = forgeInfo?.name === "github" && review.mergedAt === null;
	const { data: repoLabels } = useQuery({
		...repoLabelsQueryOptions(projectId),
		enabled: canManage,
	});
	const { data: reviewerCandidates } = useQuery({
		...reviewerCandidatesQueryOptions(projectId),
		enabled: canManage,
	});
	const { mutate: addReviewLabels } = useAddReviewLabels();
	const { mutate: removeReviewLabel } = useRemoveReviewLabel();
	const { mutate: requestReview } = useRequestReview();
	const { mutate: withdrawReviewRequest } = useWithdrawReviewRequest();

	const openLabelMenu = (evt: MouseEvent<HTMLButtonElement>) => {
		if (repoLabels === undefined) return;
		void showNativeMenuFromTrigger(
			evt.currentTarget,
			repoLabels.map((label) => {
				const applied = review.labels.some((existing) => existing.name === label.name);
				return nativeMenuItem({
					label: label.name,
					checked: applied,
					onSelect: () =>
						applied
							? removeReviewLabel({ projectId, reviewId: review.number, label: label.name })
							: addReviewLabels({ projectId, reviewId: review.number, labels: [label.name] }),
				});
			}),
		);
	};

	const openReviewerMenu = (evt: MouseEvent<HTMLButtonElement>) => {
		if (reviewerCandidates === undefined) return;
		void showNativeMenuFromTrigger(
			evt.currentTarget,
			reviewerCandidates
				// The author can't review their own PR.
				.filter((candidate) => candidate.login !== review.author?.login)
				.map((candidate) => {
					const requested = review.reviewers.some((reviewer) => reviewer.login === candidate.login);
					return nativeMenuItem({
						label: candidate.login,
						checked: requested,
						onSelect: () =>
							requested
								? withdrawReviewRequest({
										projectId,
										reviewId: review.number,
										logins: [candidate.login],
									})
								: requestReview({
										projectId,
										reviewId: review.number,
										logins: [candidate.login],
									}),
					});
				}),
		);
	};

	const pickerButton = (label: string, onClick: (evt: MouseEvent<HTMLButtonElement>) => void) => (
		<button
			aria-label={label}
			className={getButtonClassName({ variant: "outline", size: "small", iconOnly: true })}
			onClick={onClick}
			type="button"
		>
			<Icon name="plus" />
		</button>
	);

	const [statusLabel, statusVariant] = Match.value(reviewStatus(review)).pipe(
		Match.withReturnType<[string, BadgeVariant]>(),
		Match.when("open", () => ["Open", "safe"]),
		Match.when("draft", () => ["Draft", "lightGray"]),
		Match.when("merged", () => ["Merged", "purple"]),
		Match.when("closed", () => ["Closed", "danger"]),
		Match.exhaustive,
	);

	const createdAtMs = review.createdAt === null ? null : Date.parse(review.createdAt);

	const handleOpen = (evt: MouseEvent<HTMLAnchorElement>): void => {
		evt.preventDefault();
		void window.lite.openInWebBrowser(review.htmlUrl);
	};

	return (
		<aside className={styles.panel}>
			<Section heading="Status">
				<div className={styles.statusRow}>
					<Badge variant={statusVariant}>{statusLabel}</Badge>
					<a href={review.htmlUrl} onClick={handleOpen} className={classes("text-13", styles.link)}>
						{review.unitSymbol}
						{review.number}
						<Icon name="arrow-up-right" />
					</a>
				</div>
			</Section>

			{review.author !== null && (
				<Section heading="Author">
					<ReviewUser user={review.author} />
				</Section>
			)}

			{(reviewerList.length > 0 || reviewerCandidates !== undefined) && (
				<Section
					heading="Reviewers"
					action={
						reviewerCandidates !== undefined && pickerButton("Request a review", openReviewerMenu)
					}
				>
					{reviewerList.map(({ user, verdict }) => {
						const [icon, color, label] = verdictBits(verdict);
						return (
							<div key={user.id} className={styles.reviewerRow} title={label}>
								<ReviewUser user={user} />
								<Icon name={icon} style={{ color }} />
							</div>
						);
					})}
				</Section>
			)}

			{(review.labels.length > 0 || repoLabels !== undefined) && (
				<Section
					heading="Labels"
					action={repoLabels !== undefined && pickerButton("Edit labels", openLabelMenu)}
				>
					<div className={styles.labels}>
						{review.labels.map((label) => (
							<Label key={label.name} label={label} />
						))}
					</div>
				</Section>
			)}

			{forgeInfo?.capabilities.checks === true && (
				<ChecksSection projectId={projectId} reference={review.sourceBranch} />
			)}

			<Section heading="Branches">
				<div className={classes("text-12", styles.branches)}>
					<Icon name="branch" />
					{review.sourceBranch} → {review.targetBranch}
				</div>
			</Section>

			{createdAtMs !== null && (
				<Section heading="Created">
					<span
						className={classes("text-13", styles.created)}
						title={absoluteDate.format(createdAtMs)}
					>
						{formatRelativeTime(createdAtMs)}
					</span>
				</Section>
			)}
		</aside>
	);
};
