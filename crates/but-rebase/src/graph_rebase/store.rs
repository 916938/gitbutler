//! The editor's store, in two halves. The COMMIT half lives in [`CommitArena`]
//! (`arena.rs`): the owned commit graph with ordered parent arrays, settings, the
//! children index, and stable parent-entry ids — no reference knowledge at all. This
//! module holds the REFERENCE half — the ref table and the position layout — plus
//! [`EditorStore`], which composes the two and owns every method that must read across
//! them (classification, workspace-parent provenance, the [`EditorIndex`] dispatchers).
//! Nothing is ever deleted on either side (commits and references tombstone in place),
//! so ids stay stable. [`CommitSpec`] exists only at the API boundary.

use std::collections::{HashMap, HashSet};

use crate::graph_rebase::CommitSpec;
use crate::graph_rebase::arena::{CommitArena, CommitIndex, ParentEntry, ParentEntryId};

/// The stable identifier of an editor-graph entry — the editor's one union token, and
/// the only currency callers ever hold. Two namespaces: `Commit` points into the commit
/// arena (its parent list is its truth), `Ref` into the reference table (a position is
/// its truth).
///
/// The arms carry the SEALED per-namespace indices, so the union is publicly matchable —
/// which also answers "which kind is this?" statically — yet unforgeable: constructing an
/// arm requires an index only this editor issues. Namespace-specific operations take
/// [`CommitIndex`] or [`RefIndex`] instead, so "a reference in a parent array" is
/// unrepresentable rather than a runtime check.
///
/// Issued indices are valid for the editor's lifetime: entries are tombstoned in place,
/// never deleted, so a held index keeps resolving (honestly — see
/// [`Editor::is_removed`](crate::graph_rebase::Editor::is_removed)) through renames,
/// rewrites, and removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EditorIndex {
    /// A commit or its tombstone in the commit arena.
    Commit(CommitIndex),
    /// A reference (live or tombstoned) in the ref table.
    Ref(RefIndex),
}

impl EditorIndex {
    /// The commit-arena index, when this addresses a commit or tombstone.
    pub fn as_commit(self) -> Option<CommitIndex> {
        match self {
            EditorIndex::Commit(i) => Some(i),
            EditorIndex::Ref(_) => None,
        }
    }

    /// The ref-table index, when this addresses a reference.
    pub fn as_ref(self) -> Option<RefIndex> {
        match self {
            EditorIndex::Ref(i) => Some(i),
            EditorIndex::Commit(_) => None,
        }
    }
}

impl std::fmt::Display for EditorIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditorIndex::Commit(i) => write!(f, "{i}"),
            EditorIndex::Ref(i) => write!(f, "{i}"),
        }
    }
}

impl From<CommitIndex> for EditorIndex {
    fn from(n: CommitIndex) -> Self {
        EditorIndex::Commit(n)
    }
}

/// An entry in the reference table, live or tombstoned. References are edgeless — a position is
/// their truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RefIndex(pub(crate) usize);

impl From<RefIndex> for EditorIndex {
    fn from(r: RefIndex) -> Self {
        EditorIndex::Ref(r)
    }
}

impl std::fmt::Display for RefIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

/// One reference's editor state: name, mutability, liveness, convergence flag. Deletion
/// flips `live` but keeps the name and position — that matters: indices taken before the
/// deletion still resolve through the tombstoned record, and rebuilds copy it forward. Where a
/// reference SITS is not
/// state but list structure in the layout table, read via [`EditorStore::positioned_on`],
/// [`EditorStore::below_of`] and the derived queries in `positions` (`ref_depth` for rank,
/// `entering` for entering parent entries, `resolve_to_commit` for the commit through tombstones).
#[derive(Debug, Clone)]
pub(crate) struct RefState {
    /// The full reference name.
    pub refname: gix::refs::FullName,
    /// The name this reference had on disk when the editor was created, `None` for refs
    /// created mid-session — the set the rebase computes deletions against. The NAME is
    /// kept, not a flag: a rename changes `refname` but the on-disk ref to delete is the
    /// original. Only mutable-at-creation refs record it.
    pub created_as: Option<gix::refs::FullName>,
    /// Whether the rebase may move this reference.
    pub mutable: bool,
    /// `false` once the reference is deleted; the record stays.
    pub live: bool,
    /// More than one thing (parent entries and/or stacked refs) converged here — a merge. A
    /// creation-time signal distinct from the entering-parent entry count (a position can converge
    /// yet resolve to a single parent entry), so it is preserved here, never re-derived.
    pub ambiguous: bool,
}

/// The editor's carry: [`but_graph::ref_layout::GroupCarry`] over STABLE PARENT-ENTRY IDS — the
/// same shape the display side stores positionally, with stored `(child, parent number)`
/// coordinates resolved to ids at creation. Listed parent entries are always read against the
/// commit's live parent entries, so a statement whose parent entry was deleted is harmless dead weight, and an
/// unrelated parent entry appearing later at the same `(child, parent number)` coordinates gets a
/// fresh id, so it cannot revive that statement.
pub(crate) type GroupCarry = but_graph::ref_layout::GroupCarry<ParentEntryId>;

/// The editor's reference group: [`but_graph::ref_layout::RefGroup`] over stable parent entry
/// ids — an ordered bottom→top run of member NAMES sharing one [`GroupCarry`]. Order and
/// stacking are implied by the list: the member below is the previous entry, and a
/// member's rank is its index plus the height of whatever the group is attached to
/// (`positions::ref_depth`).
pub(crate) type RefGroup = but_graph::ref_layout::RefGroup<ParentEntryId>;

/// The reference half of the editor's store: the ref table, its name lookup, and the
/// position layout. The mirror of [`CommitArena`] — where that half knows nothing about
/// references, this half holds arena coordinates ([`CommitIndex`], [`ParentEntryId`])
/// as OPAQUE DATA: only [`EditorStore`]'s cross-store methods dereference them.
#[derive(Debug, Clone, Default)]
struct RefLedger {
    refs: Vec<RefState>,
    /// Each record's index by CURRENT name — names are unique across live and tombstoned
    /// records (re-creating a deleted name revives its record), which is why the layout
    /// table can identify members by name alone.
    ///
    /// Revival is load-bearing, not an accident: a delete-then-recreate within one edit
    /// session is the SAME reference to every name-keyed structure (layout membership, stale
    /// indices, the materialized ref edit), so the old record — position retained — must
    /// come back rather than a second record competing for the name.
    by_name: HashMap<gix::refs::FullName, RefIndex>,
    /// THE position store — the SAME [`but_graph::ref_layout::PositionTable`] the stored
    /// [`RefLayout`](but_graph::ref_layout::RefLayout) uses, keyed by commit indices and
    /// stable parent entry ids instead of commit ids: per STORED (unresolved) key, the reference
    /// groups standing on it. A reference's position (its `on`, its below, its rank, its
    /// entering parent entries) is all list structure there, read via [`EditorStore::positioned_on`],
    /// [`EditorStore::below_of`], `positions::ref_depth` and `positions::entering`.
    layout: but_graph::ref_layout::RefGroups<CommitIndex, ParentEntryId>,
}

/// The editor's graph: a [`but_graph::CommitGraph`] arena where COMMITS carry ordered
/// parent arrays, plus a table of [`RefState`]s where REFERENCES carry explicit positions —
/// the arena is the truth for commits, positions the truth for refs, with no overlap.
/// References are edgeless: creation authors their positions straight from the stored
/// [`RefLayout`](but_graph::ref_layout::RefLayout).
#[derive(Debug, Clone, Default)]
pub(crate) struct EditorStore {
    /// The commit table — the vanilla half of the store; see [`CommitArena`].
    commits: CommitArena,

    /// The reference table and position layout — the GitButler half; see [`RefLedger`].
    ledger: RefLedger,

    // ── Workspace-parent provenance (see [`WsParentKind`]). ──
    /// The workspace commit's REAL parents as ingested: `(parent entry, target)` per entry backed by
    /// an on-disk parent of the managed workspace commit. An entry stays faithful while its parent entry
    /// still targets what it targeted at ingestion; surgery that re-hangs or removes the parent entry
    /// revokes it. The rebase writes faithful entries verbatim, so an untouched merge reproduces
    /// however its parents relate to each other.
    pub(crate) ws_real_parents: Vec<(ParentEntryId, CommitIndex)>,
    /// The workspace commit's MINTED parents as ingested: the amended-list entries that exist
    /// only in the declaration, one per empty chain. They are lanes, not ancestry, and are never
    /// written as real parents (see [`WsParentKind::Minted`]).
    pub(crate) ws_minted_parents: Vec<(ParentEntryId, CommitIndex)>,
}

/// What a live parent of the managed workspace commit is, for the write rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WsParentKind {
    /// An ingested on-disk parent whose parent entry is untouched — written verbatim, so a rebase that
    /// changed nothing reproduces the merge byte for byte.
    Faithful,
    /// An ingested amended entry declaring an empty lane, parent entry untouched — NEVER written. An
    /// empty branch is not ancestry: git cannot repeat a parent, and order-dependent merge
    /// resolution can drop one contained in another, so the declaration is its only home.
    Minted,
    /// Produced or re-hung by surgery. The empty-lane rule decides: a duplicate of, or a commit
    /// contained in, another parent is not written.
    Surgical,
}

impl EditorStore {
    /// Adopt `arena` as the editor's arena. Full commit records (flags, refs, generation)
    /// survive, which handing the arena back out after a rebase depends on. Every parent
    /// parent entry must already point at a node in the graph — the editor's standing requirement —
    /// and the caller follows up with per-node settings via [`Self::set_step`].
    pub(crate) fn adopt(arena: but_graph::CommitGraph) -> Self {
        Self {
            commits: CommitArena::adopt(arena),
            ledger: RefLedger::default(),
            ws_real_parents: Vec::new(),
            ws_minted_parents: Vec::new(),
        }
    }

    /// Classify each live parent of the workspace commit, BY PARENT INDEX. Index-wise is the whole
    /// point: a real parent and the mint declaring an empty lane can name the same commit, so
    /// answering per target would call the mint faithful and write it — which is how a mint that
    /// reached disk once became permanent and let the merge grow on metadata-only operations.
    pub(crate) fn ws_parent_kinds(&self, ws: CommitIndex) -> Vec<WsParentKind> {
        let real: HashSet<(ParentEntryId, CommitIndex)> =
            self.ws_real_parents.iter().copied().collect();
        let minted: HashSet<(ParentEntryId, CommitIndex)> =
            self.ws_minted_parents.iter().copied().collect();
        self.parents(ws)
            .into_iter()
            .enumerate()
            .map(|(index, target)| {
                let Some(entry) = self.entry_id_at(ws, index) else {
                    return WsParentKind::Surgical;
                };
                if minted.contains(&(entry, target)) {
                    WsParentKind::Minted
                } else if real.contains(&(entry, target)) {
                    WsParentKind::Faithful
                } else {
                    WsParentKind::Surgical
                }
            })
            .collect()
    }

    /// The stable identity of the live parent entry at `(child, parent_number)`, if it exists.
    pub(crate) fn entry_id_at(
        &self,
        child: CommitIndex,
        parent_number: usize,
    ) -> Option<ParentEntryId> {
        self.commits.entry_id_at(child, parent_number)
    }

    /// THE arena, read-only — the write-through seam projects it after a rebase.
    pub(crate) fn arena(&self) -> &but_graph::CommitGraph {
        self.commits.graph()
    }

    /// Surrender the arena — the materialized commit graph, the editor's final product.
    pub(crate) fn into_arena(self) -> but_graph::CommitGraph {
        self.commits.into_graph()
    }

    /// Add the commit `spec` describes to the node arena and return its stable id.
    /// References do not belong here — use [`Self::add_reference`].
    pub(crate) fn add_commit(&mut self, spec: CommitSpec) -> CommitIndex {
        self.commits.add_commit(spec)
    }

    /// Add an entry born tombstoned: a placeholder that holds no commit. Only unit-test
    /// graph builders construct these; real removal tombstones an existing commit.
    #[cfg(test)]
    pub(crate) fn add_tombstone(&mut self) -> CommitIndex {
        self.commits.add_tombstone()
    }

    /// Overwrite the commit at `entry` per `spec` — id and settings both; revives a
    /// tombstone.
    pub(crate) fn set_commit(&mut self, entry: CommitIndex, spec: CommitSpec) {
        self.commits.set_commit(entry, spec);
    }

    /// Tombstone the node at `entry`: it stops holding a commit (settings go stale, not
    /// cleared).
    pub(crate) fn tombstone_commit(&mut self, entry: CommitIndex) {
        self.commits.tombstone_commit(entry);
    }

    /// The commit id of the commit at `entry` — `None` for tombstones and references. The
    /// cheap way to read just the id; use [`Self::commit_spec`] for the full spec.
    pub(crate) fn commit_id(&self, entry: impl Into<EditorIndex>) -> Option<gix::ObjectId> {
        match entry.into() {
            EditorIndex::Commit(i) => self.commits.commit_id(i),
            EditorIndex::Ref(_) => None,
        }
    }

    /// The spec of the commit at `entry`, assembled from the arena commit and its
    /// settings column; `None` for a tombstone.
    pub(crate) fn commit_spec(&self, entry: CommitIndex) -> Option<CommitSpec> {
        self.commits.commit_spec(entry)
    }

    /// Rewrite the commit id of the commit at `entry` IN PLACE — THE rebase write: the node id,
    /// its parent array, its settings, and every position naming it all survive unchanged.
    pub(crate) fn set_commit_id(&mut self, entry: CommitIndex, id: gix::ObjectId) {
        self.commits.set_commit_id(entry, id);
    }

    /// Overwrite the preserved parents of the commit at `entry` (see
    /// [`CommitSpec::preserved_parents`]).
    pub(crate) fn set_preserved_parents(
        &mut self,
        entry: CommitIndex,
        parents: Option<Vec<gix::ObjectId>>,
    ) {
        self.commits.set_preserved_parents(entry, parents);
    }

    /// `true` iff `entry` is a commit — `false` for tombstones and references.
    pub(crate) fn is_commit(&self, entry: impl Into<EditorIndex>) -> bool {
        self.commit_id(entry).is_some()
    }

    /// All node-arena ids (commits and tombstones), ascending — the type says
    /// references are not here; see [`Self::references`] and [`Self::ref_indices`].
    pub(crate) fn commit_indices(&self) -> impl Iterator<Item = CommitIndex> + '_ {
        self.commits.commit_indices()
    }

    /// The nodes that no other node lists as a parent — the childless tips, ascending.
    /// Callers (head discovery) want commits and tombstones only; references can't appear
    /// here by type.
    pub(crate) fn tips(&self) -> impl Iterator<Item = CommitIndex> + '_ {
        self.commits.tips()
    }

    /// Add a reference and return its stable id. Names are identity: adding a name that
    /// already has a record — a re-created deleted ref, say — RESURRECTS that record in
    /// place (its retained position stands until the caller re-places it).
    pub(crate) fn add_reference(
        &mut self,
        refname: gix::refs::FullName,
        mutable: bool,
        existed_at_creation: bool,
    ) -> RefIndex {
        let created_as = (existed_at_creation && mutable).then(|| refname.clone());
        if let Some(&existing) = self.ledger.by_name.get(&refname) {
            let record = &mut self.ledger.refs[existing.0];
            record.mutable = mutable;
            record.live = true;
            return existing;
        }
        let entry = RefIndex(self.ledger.refs.len());
        self.ledger.by_name.insert(refname.clone(), entry);
        self.ledger.refs.push(RefState {
            refname,
            created_as,
            mutable,
            live: true,
            ambiguous: false,
        });
        entry
    }

    /// Drop `entry`'s creation mark — for refs that existed but are not ours to delete
    /// (a foreign worktree's checkout).
    pub(crate) fn clear_existed_at_creation(&mut self, entry: RefIndex) {
        self.ledger.refs[entry.0].created_as = None;
    }

    /// The creation-time names of mutable references, live, dead, or since renamed —
    /// the deletion universe.
    pub(crate) fn creation_references(&self) -> impl Iterator<Item = &gix::refs::FullName> {
        self.ledger
            .refs
            .iter()
            .filter_map(|record| record.created_as.as_ref())
    }

    /// The record registered under `name`, live or dead.
    pub(crate) fn entry_of(&self, name: &gix::refs::FullNameRef) -> Option<RefIndex> {
        self.ledger.by_name.get(name).copied()
    }

    /// The CURRENT name of the record at `entry`, live or dead.
    fn name_of(&self, entry: RefIndex) -> &gix::refs::FullName {
        &self.ledger.refs[entry.0].refname
    }

    /// The reference payload at `entry` — `Some` iff it names a live (non-deleted) reference.
    pub(crate) fn reference(&self, entry: EditorIndex) -> Option<(&gix::refs::FullName, bool)> {
        let EditorIndex::Ref(i) = entry else {
            return None;
        };
        let record = self.ledger.refs.get(i.0)?;
        record.live.then_some((&record.refname, record.mutable))
    }

    /// `true` iff `entry` is a live reference.
    pub(crate) fn is_reference(&self, entry: impl Into<EditorIndex>) -> bool {
        self.reference(entry.into()).is_some()
    }

    /// All live references, ascending by id.
    pub(crate) fn references(
        &self,
    ) -> impl Iterator<Item = (RefIndex, &gix::refs::FullName, bool)> + '_ {
        self.ledger
            .refs
            .iter()
            .enumerate()
            .filter_map(|(i, record)| {
                record
                    .live
                    .then_some((RefIndex(i), &record.refname, record.mutable))
            })
    }

    /// All reference ids — live AND dead — ascending. Dead references still carry their
    /// retained name and position (see [`RefState`]).
    pub(crate) fn ref_indices(&self) -> impl Iterator<Item = RefIndex> + '_ {
        (0..self.ledger.refs.len()).map(RefIndex)
    }

    /// The full record of the reference at `entry`, including dead ones — rebuilds need the
    /// retained payload.
    pub(crate) fn state_of(&self, entry: EditorIndex) -> Option<&RefState> {
        match entry {
            EditorIndex::Ref(i) => self.ledger.refs.get(i.0),
            EditorIndex::Commit(_) => None,
        }
    }

    /// Rename (or resurrect) the reference at `entry` in place; its position is untouched —
    /// the layout table's member entry renames with it.
    pub(crate) fn set_reference(
        &mut self,
        entry: RefIndex,
        refname: gix::refs::FullName,
        mutable: bool,
    ) {
        let old_name = self.ledger.refs[entry.0].refname.clone();
        if old_name != refname {
            debug_assert!(
                !self.ledger.by_name.contains_key(&refname),
                "BUG: renaming {old_name} onto a name that already has a record: {refname}"
            );
            self.ledger.by_name.remove(&old_name);
            self.ledger.by_name.insert(refname.clone(), entry);
            self.ledger.layout.rename(old_name.as_ref(), &refname);
        }
        let record = &mut self.ledger.refs[entry.0];
        record.refname = refname;
        record.mutable = mutable;
        record.live = true;
    }

    /// Delete the reference at `entry`: it goes dead in place, retaining name and position
    /// (see [`RefState`]).
    pub(crate) fn tombstone_reference(&mut self, entry: RefIndex) {
        self.ledger.refs[entry.0].live = false;
    }

    /// The stored key the reference at `entry` stands on (a commit, or its tombstone after
    /// deletion), live or dead — `None` until placed.
    pub(crate) fn positioned_on(&self, entry: impl Into<EditorIndex>) -> Option<CommitIndex> {
        let entry = entry.into().as_ref()?;
        self.locate(entry).map(|(key, ..)| key)
    }

    /// The reference directly underneath `entry` in the physical stack — `None` when it
    /// sits directly on its commit (or holds no position at all; see [`Self::is_positioned`]).
    pub(crate) fn below_of(&self, entry: impl Into<EditorIndex>) -> Option<RefIndex> {
        let entry = entry.into().as_ref()?;
        self.ledger
            .layout
            .below_of(self.name_of(entry).as_ref())
            .and_then(|name| self.ledger.by_name.get(name.as_ref()).copied())
    }

    /// Whether the reference at `entry` holds a position — only unborn refs don't.
    pub(crate) fn is_positioned(&self, entry: impl Into<EditorIndex>) -> bool {
        entry
            .into()
            .as_ref()
            .is_some_and(|entry| self.locate(entry).is_some())
    }

    /// The preserved convergence flag of the reference at `entry` (see [`RefState`]).
    pub(crate) fn ambiguous_of(&self, entry: RefIndex) -> bool {
        self.ledger.refs[entry.0].ambiguous
    }

    /// Author a position for `entry`: `entering` is the carry intent — the parent entries meant to
    /// enter through it — classified against `on`'s CURRENT parent entries (see [`Self::classify`]).
    /// Members stacked on the entry ride along; only correct when the entry's parent entries are
    /// already complete.
    pub(crate) fn set_position(
        &mut self,
        entry: RefIndex,
        on: CommitIndex,
        entering: &[ParentEntry],
        ambiguous: bool,
        below: Option<RefIndex>,
    ) {
        let carry = self.classify(on, entering);
        let vacated = self.extract(entry);
        self.place(entry, on, carry, below, vacated);
        self.set_ambiguous(entry, ambiguous);
    }

    /// Splice `entry` out of the physical stack, dependents HEALING past it: members above it
    /// in its group close the gap, and groups attached to it re-hang onto what it sat on.
    /// The entry itself stays placed as a single-member BRANCH group at the same spot (same
    /// key, attached to its old below, carry copied) — the retained position a deletion
    /// leaves behind.
    pub(crate) fn splice(&mut self, entry: RefIndex) {
        let name = self.name_of(entry).clone();
        self.ledger.layout.splice(name.as_ref());
    }

    /// Join `entry` into the group of `mate` — copying the mate's key, carry, and ambiguity —
    /// sitting on `below`.
    pub(crate) fn join_group_of(
        &mut self,
        entry: RefIndex,
        mate: RefIndex,
        below: Option<RefIndex>,
    ) {
        let Some((key, ..)) = self.locate(mate) else {
            return;
        };
        let carry = self
            .ledger
            .layout
            .carry_of(self.name_of(mate).as_ref())
            .cloned()
            .expect("just located");
        let ambiguous = self.ledger.refs[mate.0].ambiguous;
        let vacated = self.extract(entry);
        self.place(entry, key, carry, below, vacated);
        self.set_ambiguous(entry, ambiguous);
    }

    /// Re-key `entry`'s position onto `onto`, carrying its CURRENT carry — as maintained
    /// through parent entry surgery. Below and ambiguity are preserved; members stacked on the entry
    /// ride along.
    pub(crate) fn rekey_position(&mut self, entry: RefIndex, onto: CommitIndex) {
        let Some(on) = self.positioned_on(entry) else {
            return;
        };
        if on == onto {
            return;
        }
        let below = self.below_of(entry);
        let carry = self.carry_of(entry).cloned().unwrap_or(GroupCarry::All);
        let vacated = self.extract(entry);
        self.place(entry, onto, carry, below, vacated);
    }

    /// Re-hang `entry` onto `below` — an adjacency statement only; its key, carry, and
    /// ambiguity are untouched, and members stacked on the entry ride along.
    pub(crate) fn set_below(&mut self, entry: RefIndex, below: Option<RefIndex>) {
        let Some(on) = self.positioned_on(entry) else {
            return;
        };
        if self.below_of(entry) == below {
            return;
        }
        let carry = self.carry_of(entry).cloned().unwrap_or(GroupCarry::All);
        let vacated = self.extract(entry);
        self.place(entry, on, carry, below, vacated);
    }

    /// Point a dead reference's kept position at `on` — the bare pointer that stale
    /// indices resolve through. Its old spot heals (dependents re-hang past it),
    /// and it lands as a bare root; live references re-place via the layout machinery
    /// instead.
    pub(crate) fn set_retained_position(&mut self, entry: RefIndex, on: CommitIndex) {
        debug_assert!(
            !self.is_reference(entry),
            "retained positions belong to dead references"
        );
        self.splice(entry);
        let vacated = self.extract(entry);
        self.place(entry, on, GroupCarry::None, None, vacated);
        self.set_ambiguous(entry, false);
    }

    /// The carry of the group holding the reference at `entry`, if it holds a position.
    pub(crate) fn carry_of(&self, entry: impl Into<EditorIndex>) -> Option<&GroupCarry> {
        let entry = entry.into().as_ref()?;
        self.ledger.layout.carry_of(self.name_of(entry).as_ref())
    }

    /// All positioned references — live AND dead — ascending by id.
    pub(crate) fn positioned_refs(&self) -> impl Iterator<Item = RefIndex> + '_ {
        // O(R²): locate() is a linear scan run once per ref, and the mutation helpers that
        // loop this are O(R²)–O(R³). Fine while R is human-scale; the tripwire makes the
        // "R stays small" assumption (see locate) announce itself before it becomes a cliff.
        debug_assert!(
            self.ledger.refs.len() < 4096,
            "positioned_refs is O(R²); R={} — de-linearize locate() before this scales",
            self.ledger.refs.len()
        );
        (0..self.ledger.refs.len())
            .map(RefIndex)
            .filter(|&entry| self.locate(entry).is_some())
    }

    /// Where `entry` sits in the layout table: `(key, group index, member index)`.
    /// Membership is by NAME — the record's current name is the table identity.
    /// Find a reference's position as `(commit, group, member index)`. THE HOT PRIMITIVE: a
    /// linear scan of the whole layout, deliberately left linear — unlike the arena's
    /// `children` index, which de-linearizes `children_of` over thousands of commits —
    /// because R, the refs in a workspace, is human-scale. If a workspace ever grows into
    /// the hundreds of branches, this is the primitive to index.
    fn locate(&self, entry: RefIndex) -> Option<(CommitIndex, usize, usize)> {
        self.ledger.layout.locate(self.name_of(entry).as_ref())
    }

    /// Classify a position's entering-parent entry intent against `on`'s CURRENT parent entries: empty is a
    /// root, the whole live set the shared `All` carry, any other set an `Entries` carry
    /// stating exactly those parent entries.
    fn classify(&self, on: CommitIndex, entering: &[ParentEntry]) -> GroupCarry {
        if entering.is_empty() {
            return GroupCarry::None;
        }
        let live = match crate::graph_rebase::positions::resolve_to_commit(self, on) {
            Some(commit) => crate::graph_rebase::positions::live_children_of(self, commit),
            None => Vec::new(),
        };
        let edge_set: HashSet<_> = entering.iter().copied().collect();
        let live_set: HashSet<_> = live.iter().copied().collect();
        if edge_set == live_set {
            GroupCarry::All
        } else {
            // The intent is authored positionally from live captures; STATED by id.
            let mut entries: Vec<ParentEntryId> = entering
                .iter()
                .filter_map(|&ParentEntry { child, number }| self.entry_id_at(child, number))
                .collect();
            entries.sort_unstable();
            entries.dedup();
            GroupCarry::Entries(entries)
        }
    }

    /// Take `entry` out of the table, its dependents RIDING: members stacked above it in its
    /// group split off as their own group attached to `entry` (they keep sitting on it,
    /// wherever it goes next), and groups attached to `entry` stay attached. The caller
    /// re-places the entry immediately.
    fn extract(&mut self, entry: RefIndex) -> Option<gix::refs::FullName> {
        let name = self.name_of(entry).clone();
        self.ledger.layout.extract(name.as_ref())
    }

    /// Put the (extracted or fresh) `entry` into the table at `key` via the shared
    /// [`but_graph::ref_layout::place_in_groups`] — the same algorithm the builder authors with.
    fn place(
        &mut self,
        entry: RefIndex,
        key: CommitIndex,
        carry: GroupCarry,
        attach: Option<RefIndex>,
        vacated: Option<gix::refs::FullName>,
    ) {
        let carry = self.normalize_carry(key, carry);
        let name = self.name_of(entry).clone();
        let attach = attach.map(|b| self.name_of(b).clone());
        self.ledger.layout.place(name, key, carry, attach, vacated);
    }

    /// An `Entries` carry naming the key's ENTIRE live parent entry set re-states as `All` — the same
    /// normalization `classify` applies at authoring. Placement is the only door to the
    /// table, so every live group holds a normalized carry and the twin-key rule in
    /// [`but_graph::ref_layout::place_in_groups`] never misses a resolve-equal twin behind
    /// a syntactically different statement.
    fn normalize_carry(&self, key: CommitIndex, carry: GroupCarry) -> GroupCarry {
        let GroupCarry::Entries(entries) = &carry else {
            return carry;
        };
        let live: Vec<ParentEntryId> =
            match crate::graph_rebase::positions::resolve_to_commit(self, key) {
                Some(commit) => crate::graph_rebase::positions::live_children_of(self, commit)
                    .into_iter()
                    .filter_map(|ParentEntry { child, number }| self.entry_id_at(child, number))
                    .collect(),
                None => return carry,
            };
        if !live.is_empty() && *entries == live {
            GroupCarry::All
        } else {
            carry
        }
    }

    /// The raw groups at `key`, for well-formedness failure reports.
    pub(crate) fn groups_at_for_debug(&self, key: CommitIndex) -> Option<&[RefGroup]> {
        self.ledger.layout.groups_at(key)
    }

    fn set_ambiguous(&mut self, entry: RefIndex, ambiguous: bool) {
        self.ledger.refs[entry.0].ambiguous = ambiguous;
    }

    /// Overwrite whether the rebase may move the reference at `entry`.
    pub(crate) fn set_ref_mutable(&mut self, entry: RefIndex, mutable: bool) {
        self.ledger.refs[entry.0].mutable = mutable;
    }

    /// The references carrying the parent entry `(child, parent number)` into `parent`: each carrying
    /// group answers with its TOP member — the below-walk covers the rest.
    pub(crate) fn entry_carriers(
        &self,
        parent: CommitIndex,
        entry: ParentEntry,
    ) -> impl Iterator<Item = RefIndex> + '_ {
        self.ledger
            .layout
            .groups_at(parent)
            .into_iter()
            .flatten()
            .filter(move |group| match &group.carry {
                GroupCarry::None => false,
                GroupCarry::All => true,
                GroupCarry::Entries(entries) => self
                    .entry_id_at(entry.child, entry.number)
                    .is_some_and(|id| entries.contains(&id)),
            })
            .filter_map(|group| group.members.last())
            .filter_map(|name| self.ledger.by_name.get(name).copied())
    }

    pub(crate) fn set_ref_ambiguous(&mut self, entry: RefIndex, ambiguous: bool) {
        self.set_ambiguous(entry, ambiguous);
    }

    /// Adopt `groups` wholesale as `key`'s reference groups — creation's id-mapped copy of
    /// the stored layout. Every member name must already be registered.
    pub(crate) fn insert_groups(&mut self, key: CommitIndex, groups: Vec<RefGroup>) {
        debug_assert!(
            groups
                .iter()
                .flat_map(|g| g.members.iter())
                .all(|name| self.ledger.by_name.contains_key(name.as_ref())),
            "BUG: ingest must register every reference before copying groups"
        );
        self.ledger.layout.insert_groups(key, groups);
    }

    /// The ordered parents of `entry` — parent number position is the parent order; references
    /// have none.
    pub(crate) fn parents(&self, entry: impl Into<EditorIndex>) -> Vec<CommitIndex> {
        match entry.into() {
            EditorIndex::Commit(i) => self.commits.parents(i),
            EditorIndex::Ref(_) => Vec::new(),
        }
    }

    /// How many parents `entry` has.
    pub(crate) fn parent_count(&self, entry: impl Into<EditorIndex>) -> usize {
        self.parents(entry).len()
    }

    /// Every parent entry that names `entry` as a parent, as sorted `(child, parent number)`
    /// pairs — answered from the maintained children index, not an arena scan.
    pub(crate) fn children_of(&self, entry: impl Into<EditorIndex>) -> &[ParentEntry] {
        match entry.into() {
            EditorIndex::Commit(i) => self.commits.children_of(i),
            EditorIndex::Ref(_) => &[],
        }
    }

    /// Append `parent` as `child`'s last parent; returns its parent number.
    pub(crate) fn push_parent(&mut self, child: CommitIndex, parent: CommitIndex) -> usize {
        self.commits.push_parent(child, parent)
    }

    /// Insert `parent` at `parent number` of `child` (clamped to the array end); later
    /// parent numbers shift up, their statements untouched — a statement names an
    /// [`ParentEntryId`], not a position. Returns the parent number actually used.
    pub(crate) fn insert_parent(
        &mut self,
        child: CommitIndex,
        parent_number: usize,
        parent: CommitIndex,
    ) -> usize {
        self.commits.insert_parent(child, parent_number, parent)
    }

    /// Remove `child`'s parent at `parent number`, returning it; later parent numbers
    /// shift down, their statements untouched, and statements naming the removed parent entry
    /// are dropped for good — an operation that wants them back must state them again.
    /// The ONE cross-store parent mutation: the removal is the arena's, the statement
    /// drop is the layout's.
    pub(crate) fn remove_parent(
        &mut self,
        child: CommitIndex,
        parent_number: usize,
    ) -> Option<CommitIndex> {
        let (target, removed) = self.commits.remove_parent(child, parent_number)?;
        self.retain_edges(|&id| id != removed);
        Some(target)
    }

    /// Re-point `child`'s parent at `parent number` onto `new_parent`. The parent entry keeps its
    /// [`ParentEntryId`], so groups stated on it follow the parent entry to its new target.
    pub(crate) fn replace_parent(
        &mut self,
        child: CommitIndex,
        parent_number: usize,
        new_parent: CommitIndex,
    ) {
        self.commits
            .replace_parent(child, parent_number, new_parent);
    }

    /// Move `from`'s whole parent array onto `to` (which must have none); the parent entries keep
    /// their identities, so statements follow without any rewrite.
    pub(crate) fn transplant_parents(&mut self, from: CommitIndex, to: CommitIndex) {
        self.commits.transplant_parents(from, to);
    }

    /// Re-target every parent-array entry naming `from` onto `to`, parent numbers preserved —
    /// the parent entries keep their ids, so statements naming them stay valid untouched.
    pub(crate) fn redirect_children(&mut self, from: CommitIndex, to: CommitIndex) {
        self.commits.redirect_children(from, to);
    }

    /// Empty `child`'s parent array, returning each parent with its retired parent entry id.
    /// Group statements naming the drained parent entries are DELIBERATELY untouched: the caller
    /// re-states the orphaned ids onto their new carrier itself via
    /// [`Self::restate_entries`] (the below-insert path re-hangs them onto the range's
    /// parent-most parent entry).
    pub(crate) fn drain_parents(
        &mut self,
        child: CommitIndex,
    ) -> Vec<(CommitIndex, ParentEntryId)> {
        self.commits.drain_parents(child)
    }

    /// Re-state carry statements from retired parent entry ids onto their successors — the ONE
    /// deliberate statement rewrite left (the drain-then-re-hang path); renumbering needs
    /// none, parent entry identity being stable.
    pub(crate) fn restate_entries(&mut self, restates: &[(ParentEntryId, ParentEntryId)]) {
        for group in self.ledger.layout.groups_mut() {
            let GroupCarry::Entries(entries) = &mut group.carry else {
                continue;
            };
            let mut changed = false;
            for entry in entries.iter_mut() {
                if let Some((_, new)) = restates.iter().find(|(old, _)| old == entry) {
                    *entry = *new;
                    changed = true;
                }
            }
            if changed {
                entries.sort_unstable();
                entries.dedup();
            }
        }
    }

    // --- The parent arrays ---
    //
    // Carry entries name parent entries by STABLE ID; renumbering never touches statements.
    // Removing a parent retires its id and drops its statements for good — an operation
    // that wants them back must state them again; the store never resurrects them by
    // accident (and an unrelated parent entry at the old coordinates can never impersonate one).

    /// Drop every stated carry parent entry id `keep` rejects.
    fn retain_edges(&mut self, keep: impl Fn(&ParentEntryId) -> bool) {
        for group in self.ledger.layout.groups_mut() {
            if let GroupCarry::Entries(entries) = &mut group.carry {
                entries.retain(&keep);
            }
        }
    }
}
