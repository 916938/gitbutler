//! Where each reference sits, stored as position data rather than as graph parent entries.
//!
//! A commit carries parent entries; a reference carries
//! NONE. Instead every reference stands in the layout table (`EditorStore::layout`):
//! per stored key, an ordered list of groups (`RefGroup`), each a bottom→top run of references
//! sharing one [`GroupCarry`]. A position reads back through the editor's accessors: `on`
//! ([`EditorStore::positioned_on`], the stored key it stands on — a commit, or its tombstone
//! after deletion; [`resolve_to_commit`] follows it down), `below` ([`EditorStore::below_of`], the
//! reference directly underneath; `None` = on the commit), and `ambiguous`
//! ([`EditorStore::ambiguous_of`], this position is a merge).
//!
//! THE MODULE LAW: reads and laws only — every function here takes `&EditorStore`.
//! Anything that mutates the store (takes `&mut`) is a layout WRITE and belongs in
//! `ref_ops`, whatever vocabulary its contract is written in.
//!
//! Keeping references out of the parent entry graph is deliberate: a parent entry running THROUGH a reference
//! would make it bear connectivity it shouldn't — gluing a commit's history onto whatever else
//! the reference happens to touch.
//!
//! Vocabulary used throughout this module and `ref_ops`:
//! - **parent entry** — one entry of a commit's parent list, named from the child side as
//!   `(child, parent number)`, since parent lists live in their child. A commit's INCOMING
//!   entries are its children's entries pointing at it.
//! - **enters through** — an incoming entry of a commit ENTERS THROUGH a reference when it
//!   descends into that reference's position (see [`entering`]). This distinguishes
//!   co-located references and commits out which merge group a reference belongs to.
//! - **group** — references stacked on one commit, ordered by their below walk ([`ref_depth`]).
//!   Groups are shallow in practice (≤3 observed).
//! - **carry** — a group's claim over incoming entries
//!   ([`GroupCarry`]): `None` (a root group, nothing descends into it), `All` (every entry
//!   into the commit, present and future), or `Entries` (exactly the listed ones).
//! - **statement** — one listed entry of an `Entries` carry, naming a parent entry by its
//!   STABLE id: it follows the entry through renumbering and re-pointing, and dies with it —
//!   an unrelated entry later reusing the same coordinates cannot revive it.
//! - **rider** — a group member standing above another reference. Riders keep their
//!   entering statements verbatim when the ground moves, which is why surgery must move the
//!   underlying entries WITH them or the graph silently bypasses an interposed commit.
//! - **settle** (`ref_ops::settle_group_lower`) — re-anchor a split-off lower group slice
//!   onto the entry that now enters it.
//! - **land** (`ref_ops::land_stack_above`) — hang a carried stack above a known top
//!   reference, so it follows that top through later moves.
//! - **mint** — a workspace-parent entry that exists only in the stack declaration (an
//!   empty lane), never written as real ancestry; see `EditorStore::ws_minted_parents`.

use crate::graph_rebase::commits::{CommitIndex, ParentEntry};
use crate::graph_rebase::store::GroupCarry;
use crate::graph_rebase::store::RefIndex;
use crate::graph_rebase::{EditorIndex, EditorStore};

/// Resolve `entry` to the commit it stands for: a commit is itself, a tombstone follows its
/// preserved first parent entry downward, a reference goes via its stored position — dead references
/// via their RETAINED position, which stale indices normalize through (unborn refs carry
/// none and resolve to nothing).
pub(crate) fn resolve_to_commit(
    graph: &EditorStore,
    entry: impl Into<EditorIndex>,
) -> Option<CommitIndex> {
    // A reference resolves via its (retained) stored position; unborn refs carry none and
    // resolve to nothing.
    let mut node = match entry.into() {
        EditorIndex::Commit(i) => i,
        entry @ EditorIndex::Ref(_) => graph.positioned_on(entry.as_ref()?)?,
    };
    // Tombstones descend their preserved first parent entry. Like ref_depth, the bound guards the
    // acyclic invariant — a broken invariant announces itself loudly in debug instead of
    // returning a silent wrong answer.
    let mut steps = 0usize;
    loop {
        if graph.is_commit(node) {
            return Some(node);
        }
        node = *graph.parents(node).first()?;
        steps += 1;
        if steps >= 10_000 {
            debug_assert!(false, "tombstone descent cycle resolving {node:?}");
            return None;
        }
    }
}

/// A commit's incoming parent entries as sorted `(child, parent number)` pairs. The groups on the commit
/// divide these among themselves (their [`GroupCarry`]); [`entering`] reads one group's share.
pub(crate) fn live_children_of(graph: &EditorStore, commit: CommitIndex) -> Vec<ParentEntry> {
    graph
        .children_of(commit)
        .iter()
        .copied()
        .filter(|&ParentEntry { child, .. }| graph.is_commit(child))
        .collect()
}

/// The parent entries currently entering through the reference at `entry`: the group's own carry
/// statement (kept aligned by the parent number mutators), ordered and filtered by the resolved commit's
/// live parent entries so a stale carry parent entry never reaches a consumer.
pub(crate) fn entering(graph: &EditorStore, entry: impl Into<EditorIndex>) -> Vec<ParentEntry> {
    let Some(entry) = entry.into().as_ref() else {
        return Vec::new();
    };
    let Some(on) = graph.positioned_on(entry) else {
        return Vec::new();
    };
    let Some(carry) = graph.carry_of(entry) else {
        return Vec::new();
    };
    let entries = match resolve_to_commit(graph, on) {
        Some(commit) => live_children_of(graph, commit),
        None => Vec::new(),
    };
    match carry {
        GroupCarry::None => Vec::new(),
        GroupCarry::All => entries,
        GroupCarry::Entries(stated) => entries
            .into_iter()
            .filter(|&ParentEntry { child, number }| {
                graph
                    .entry_id_at(child, number)
                    .is_some_and(|id| stated.contains(&id))
            })
            .collect(),
    }
}

/// The members of `ref_node`'s group — every reference with the same resolved commit and the
/// same (derived) entering parent entries.
pub(crate) fn group_members(
    graph: &EditorStore,
    ref_node: impl Into<EditorIndex>,
) -> Vec<RefIndex> {
    let Some(ref_node) = ref_node.into().as_ref() else {
        return vec![];
    };
    if !graph.is_positioned(ref_node) {
        return vec![];
    }
    let commit = resolve_to_commit(graph, ref_node);
    let entering_here = entering(graph, ref_node);
    graph
        .positioned_refs()
        .filter(|&entry| {
            entering(graph, entry) == entering_here && resolve_to_commit(graph, entry) == commit
        })
        .collect()
}

/// Does `entry` enter a positioned group that resolves to `commit` — i.e. does it reach
/// the commit THROUGH a reference's group rather than plainly?
pub(crate) fn enters_group_resolving_to(
    graph: &EditorStore,
    entry: ParentEntry,
    commit: CommitIndex,
) -> bool {
    graph
        .positioned_refs()
        .any(|r| entering(graph, r).contains(&entry) && resolve_to_commit(graph, r) == Some(commit))
}

/// Every reference whose stored `on`, followed through tombstones, resolves to `commit`.
/// Order is unspecified (ascending entry id).
pub(crate) fn refs_resolving_to(graph: &EditorStore, commit: CommitIndex) -> Vec<RefIndex> {
    graph
        .positioned_refs()
        .filter(|&entry| resolve_to_commit(graph, entry) == Some(commit))
        .collect()
}

/// The references reachable from `start`, given the COMMIT set it reached. When `start` is
/// itself a reference, it counts too.
pub(crate) fn refs_reachable_with(
    graph: &EditorStore,
    start: EditorIndex,
    commits: &std::collections::HashSet<CommitIndex>,
) -> Vec<RefIndex> {
    // Match by commit id as well as entry: a graph can hold the same commit twice (in a
    // stack and in the target's history), and a reference counts as reached when its commit
    // was reached under either entry. Deleting a branch that merges back in relies on this.
    let reached_ids: std::collections::HashSet<gix::ObjectId> = commits
        .iter()
        .filter_map(|entry| graph.commit_id(*entry))
        .collect();
    let mut out = Vec::new();
    for entry in graph.positioned_refs() {
        // Node-based reachability, commit-equivalent across duplicate groups.
        let commit_reached = resolve_to_commit(graph, entry).is_some_and(|commit| {
            commits.contains(&commit)
                || graph
                    .commit_id(commit)
                    .is_some_and(|id| reached_ids.contains(&id))
        });
        if commit_reached || EditorIndex::from(entry) == start {
            out.push(entry);
        }
    }
    out
}

/// The reference's depth above its commit — the length of its below walk (0 = directly on
/// the commit). This IS the rank: order among co-located references is adjacency, not a number.
pub(crate) fn ref_depth(graph: &EditorStore, entry: impl Into<EditorIndex>) -> usize {
    let Some(entry) = entry.into().as_ref() else {
        return 0;
    };
    let mut depth = 0usize;
    let mut cursor = graph.below_of(entry);
    while let Some(b) = cursor {
        depth += 1;
        if depth > 10_000 {
            debug_assert!(false, "below walk cycle at ref {entry}");
            return depth;
        }
        cursor = graph.below_of(b);
    }
    depth
}

/// A group about to be entered by a new parent entry, captured BEFORE that parent entry exists so
/// `ref_ops::apply_group_join` never reads a half-updated store.
pub(crate) struct GroupJoin {
    /// The joining members: the reference and the group-mates its below walk rests on —
    /// each with its position captured (`on`, `below`, `ambiguous`). Root groups (no
    /// entering parent entries) at one commit are distinct siblings, so only the reference itself
    /// joins.
    pub(crate) members: Vec<(RefIndex, CommitIndex, Option<RefIndex>, bool)>,
    /// The parent entries entering the group at capture time.
    pub(crate) entering: Vec<ParentEntry>,
}

/// Capture `ref_node`'s group for a coming join — call BEFORE the joining parent entry is added.
pub(crate) fn prepare_group_join(graph: &EditorStore, ref_node: RefIndex) -> GroupJoin {
    let capture = |entry: RefIndex| {
        graph
            .positioned_on(entry)
            .map(|on| (entry, on, graph.below_of(entry), graph.ambiguous_of(entry)))
    };
    let Some(captured) = capture(ref_node) else {
        return GroupJoin {
            members: Vec::new(),
            entering: Vec::new(),
        };
    };
    let is_root = matches!(graph.carry_of(ref_node), Some(GroupCarry::None));
    let members = if is_root {
        vec![captured]
    } else {
        // Walk the below walk keeping members of this group — the physical stack may pass
        // through other groups' refs.
        let group = group_members(graph, ref_node);
        let mut members = vec![captured];
        let mut cursor = graph.below_of(ref_node);
        while let Some(b) = cursor {
            if !graph.is_positioned(b) {
                break;
            }
            if group.contains(&b) {
                members.extend(capture(b));
            }
            cursor = graph.below_of(b);
        }
        members
    };
    GroupJoin {
        members,
        entering: entering(graph, ref_node),
    }
}

/// Every reference has a well-formed position, and positions are unique wherever order
/// matters topologically — within groups entered by a child parent entry (non-empty
/// [`entering`]). Several root groups above one commit are fine: they have no meaningful
/// order, and display sorts them by name.
///
/// Wired at editor creation AND at rebase entry, so every graph shape the suite produces —
/// including post-mutation shapes — continuously validates the position model.
pub(crate) fn assert_positions_total(graph: &EditorStore) -> anyhow::Result<()> {
    assert_below_wellformed(graph)?;
    assert_on_mirrors_table(graph)?;
    type OrderedPositionKey = (Option<CommitIndex>, Vec<ParentEntry>, usize);
    let mut seen: std::collections::HashMap<OrderedPositionKey, RefIndex> = Default::default();
    for entry in graph.references().map(|(entry, _, _)| entry) {
        // No stored position is only legitimate for unborn refs (no commit below at creation).
        if !graph.is_positioned(entry) {
            continue;
        }
        let entering = entering(graph, entry);
        if entering.is_empty() {
            continue;
        }
        let commit = resolve_to_commit(graph, entry);
        let rank = ref_depth(graph, entry);
        if let Some(previous) = seen.insert((commit, entering.clone(), rank), entry) {
            let name = |entry: RefIndex| match graph.reference(entry.into()) {
                Some((refname, _)) => refname.to_string(),
                None => "removed".to_string(),
            };
            let groups = commit.and_then(|p| graph.groups_at_for_debug(p));
            anyhow::bail!(
                "BUG: references {previous} ({}) and {entry} ({}) collide at position \
                 (commit {commit:?}, entering {entering:?}, rank {rank})\ngroups: {groups:#?}",
                name(previous),
                name(entry)
            );
        }
    }
    Ok(())
}

/// Every stored `below` of a LIVE reference names a positioned reference resolving to the SAME
/// commit, and the below walk is acyclic. Tombstoned refs keep their stored position for
/// retention reads but are spliced out of the physical stack, so only live refs are graded.
/// The vanilla fact mirrors the table exactly: every reference's stored `on` equals the
/// site key the (slow) full-table scan finds for it — `None` on both sides for the
/// unplaced. The mirror is what `locate` trusts, so this clause is what licenses it.
fn assert_on_mirrors_table(graph: &EditorStore) -> anyhow::Result<()> {
    for (entry, name, key) in graph.ref_positions_for_law() {
        let scanned = graph.locate_by_scan_for_law(name.as_ref()).map(|(k, ..)| k);
        if key != scanned {
            anyhow::bail!(
                "position mirror out of step for {name} ({entry}): record says {key:?}, table scan says {scanned:?}"
            );
        }
    }
    Ok(())
}

fn assert_below_wellformed(graph: &EditorStore) -> anyhow::Result<()> {
    let name = |entry: RefIndex| match graph.reference(entry.into()) {
        Some((refname, _)) => refname.to_string(),
        None => "removed".to_string(),
    };
    for entry in graph.positioned_refs() {
        if !graph.is_reference(entry) {
            continue;
        }
        let commit = resolve_to_commit(graph, entry);
        let mut depth = 0usize;
        let mut cursor = graph.below_of(entry);
        while let Some(b) = cursor {
            if !graph.is_positioned(b) {
                anyhow::bail!("BUG: ref {entry}: below {b} is not a positioned reference");
            }
            if resolve_to_commit(graph, b) != commit {
                anyhow::bail!(
                    "BUG: ref {entry} ({}): below {b} ({}) resolves to a different commit",
                    name(entry),
                    name(b)
                );
            }
            depth += 1;
            if depth > 10_000 {
                anyhow::bail!("BUG: ref {entry}: below walk cycle");
            }
            cursor = graph.below_of(b);
        }
    }
    Ok(())
}
