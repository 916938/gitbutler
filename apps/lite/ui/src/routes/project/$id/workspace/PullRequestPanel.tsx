import { Badge, type BadgeVariant } from "#ui/components/Badge.tsx";
import { classes } from "#ui/components/classes.ts";
import { Icon } from "#ui/components/Icon.tsx";
import { formatRelativeTime } from "#ui/time.ts";
import type { ForgeReview, ForgeReviewLabel, ForgeReviewUser } from "@gitbutler/but-sdk";
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

const Section: FC<{ heading: string; children: ReactNode }> = (p) => (
	<div className={styles.section}>
		<h4 className={classes("text-13", styles.heading)}>{p.heading}</h4>
		{p.children}
	</div>
);

const User: FC<{ user: ForgeReviewUser }> = ({ user }) => (
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

export const PullRequestPanel: FC<{ review: ForgeReview }> = ({ review }) => {
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
					<User user={review.author} />
				</Section>
			)}

			{review.reviewers.length > 0 && (
				<Section heading="Reviewers">
					{review.reviewers.map((reviewer) => (
						<User key={reviewer.id} user={reviewer} />
					))}
				</Section>
			)}

			{review.labels.length > 0 && (
				<Section heading="Labels">
					<div className={styles.labels}>
						{review.labels.map((label) => (
							<Label key={label.name} label={label} />
						))}
					</div>
				</Section>
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
