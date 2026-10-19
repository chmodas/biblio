//! Guards that keep hostile `BibTeX` input from overflowing the stack, running without end or
//! panicking.
//!
//! The underlying parser resolves LaTeX commands, `@string` abbreviations, `crossref` and `xdata`
//! by recursion, with no limit on depth and no check for cycles. A stack overflow aborts the
//! process and cannot be caught, so the guards run on the raw bibliography, before anything is
//! resolved, and drop what would recurse too deeply or without end. Each guard is iterative, so the
//! guard itself cannot overflow. As a last resort, a panic in the parser is caught, and the entry
//! it happened on is skipped.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use biblatex::{
    Bibliography, ChunksExt, EntryType, Pair, RawBibliography, RawChunk, RawEntry, Spanned,
};

use crate::Diagnostic;

/// The deepest brace nesting allowed in a field value or `@string` definition.
///
/// The parser recurses for each command argument (`\x{…}`), using a few stack frames per level. A
/// 2 MiB thread in a debug build survives about 700 levels, even at the end of a long chain of
/// `@string` definitions, so this leaves a wide margin.
pub(super) const MAX_NESTING: usize = 128;

/// The deepest brace nesting in `text`, counting every `{` and `}` and never going below zero.
///
/// Backslashes are not treated as escapes, because the parser does not treat them as escapes when
/// it finds a command's argument. This counts plain groups and escaped braces too, so it is an
/// upper bound on how deeply the parser recurses.
fn nesting(text: &str) -> usize {
    let mut depth = 0_usize;
    let mut deepest = 0;
    for byte in text.bytes() {
        match byte {
            b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Whether any literal part of a field value nests deeper than [`MAX_NESTING`]. Each part is parsed
/// on its own, so their depths do not add up; references to `@string`s are judged where the
/// `@string` is defined.
fn nests_too_deeply(value: &[Spanned<RawChunk<'_>>]) -> bool {
    value.iter().any(|chunk| match chunk.v {
        RawChunk::Normal(text) => nesting(text) > MAX_NESTING,
        RawChunk::Abbreviation(_) => false,
    })
}

/// What resolving `@string` abbreviations costs the parser: the calls it makes, one for each
/// abbreviation it looks up; the definitions it compares while looking them up, because it scans
/// the list of definitions in order; and the bytes of text they expand to.
#[derive(Clone, Copy, Default)]
pub(super) struct Cost {
    /// Abbreviations looked up, each one a call that may recurse.
    calls: u64,
    /// Definitions compared while looking them up.
    scans: u64,
    /// Bytes of text produced.
    bytes: u64,
    /// Commas in the text produced, each of which may separate one more key when the text is a
    /// `crossref` or `xdata` value.
    commas: u64,
}

impl Cost {
    /// The sum of two costs, saturating.
    fn plus(self, other: Self) -> Self {
        Self {
            calls: self.calls.saturating_add(other.calls),
            scans: self.scans.saturating_add(other.scans),
            bytes: self.bytes.saturating_add(other.bytes),
            commas: self.commas.saturating_add(other.commas),
        }
    }

    /// The cost of looking up a name that has no definition, before scans are counted: one call,
    /// and a month name at most.
    const UNDEFINED: Self = Self {
        calls: 1,
        scans: 0,
        bytes: LONGEST_MONTH + CHUNK_OVERHEAD,
        commas: 0,
    };
}

/// The `@string` names that remain after the guards, with what each costs to use.
///
/// The parser finds a name by scanning its list of definitions in order, so the scan costs depend
/// on which definitions it is given. [`StringTable::plan`] works them out for the definitions an
/// input needs, which is what [`StringTable::needed`] gives the parser.
pub(super) struct StringTable<'s> {
    /// Each name that remains, by name.
    names: HashMap<&'s str, StringName<'s>>,
    /// The names, each after every name its definition uses.
    order: Vec<&'s str>,
    /// How many definitions the parser is given at most, which a look-up of a name without a
    /// definition scans in full.
    reached: u64,
}

/// One `@string` name that remains.
struct StringName<'s> {
    /// What one use of the name costs, counting every definition it uses.
    cost: Cost,
    /// The names its definition uses that have definitions.
    uses: Vec<&'s str>,
    /// The scan weight, summed, of the names without a definition that its definition uses,
    /// including names removed because they refer to themselves.
    undefined_weight: u64,
    /// Where its first definition is in the list of definitions that remain.
    position: usize,
}

/// The names of the abbreviations used in the fields of `entries`.
fn used_names<'a, 's: 'a>(
    entries: impl IntoIterator<Item = &'a Spanned<RawEntry<'s>>>,
) -> impl Iterator<Item = &'s str> {
    entries
        .into_iter()
        .flat_map(|entry| &entry.v.fields)
        .flat_map(|field| &field.value.v)
        .filter_map(|chunk| match chunk.v {
            RawChunk::Abbreviation(name) => Some(name),
            RawChunk::Normal(_) => None,
        })
}

impl<'s> StringTable<'s> {
    /// The names `uses` reach, directly or through other definitions.
    fn reached(&self, uses: impl IntoIterator<Item = &'s str>) -> HashSet<&'s str> {
        let mut pending: Vec<&'s str> = uses.into_iter().collect();
        let mut wanted: HashSet<&'s str> = HashSet::new();
        while let Some(name) = pending.pop() {
            if let Some(entry) = self.names.get(name) {
                if wanted.insert(name) {
                    pending.extend(&entry.uses);
                }
            }
        }
        wanted
    }

    /// Work out the scan costs for the definitions that `entries` need. Each part of the input is
    /// later given a subset of these, so the costs are an upper bound for every part.
    pub(super) fn plan<'a>(&mut self, entries: impl IntoIterator<Item = &'a Spanned<RawEntry<'s>>>)
    where
        's: 'a,
    {
        let wanted = self.reached(used_names(entries));
        let mut ranked: Vec<(usize, &'s str)> = wanted
            .iter()
            .map(|&name| (self.names[name].position, name))
            .collect();
        ranked.sort_unstable();
        let length = u64::try_from(ranked.len()).unwrap_or(u64::MAX);
        self.reached = length;
        let mut scans: HashMap<&'s str, u64> = HashMap::new();
        for (rank, &(_, name)) in ranked.iter().enumerate() {
            let compared = u64::try_from(rank + 1).unwrap_or(u64::MAX);
            scans.insert(name, compared.saturating_mul(scan_weight(name)));
        }
        for &name in &self.order {
            let Some(&own) = scans.get(name) else {
                continue;
            };
            let entry = &self.names[name];
            let total = entry.uses.iter().fold(
                own.saturating_add(entry.undefined_weight.saturating_mul(length)),
                |total, used| total.saturating_add(self.names[used].cost.scans),
            );
            self.names
                .entry(name)
                .and_modify(|entry| entry.cost.scans = total);
        }
    }

    /// What one use of `name` costs. A name without a definition is compared with every
    /// definition the parser is given.
    fn cost(&self, name: &str) -> Cost {
        self.names.get(name).map_or(
            Cost {
                scans: self.reached.saturating_mul(scan_weight(name)),
                ..Cost::UNDEFINED
            },
            |entry| entry.cost,
        )
    }

    /// The first definitions, in their order in `strings`, of the names that `entries` use,
    /// directly or through other definitions. The parser only ever reads a name's first
    /// definition, so giving it just these changes nothing, but keeps every look-up short.
    pub(super) fn needed(
        &self,
        strings: &[Pair<'s>],
        entries: &[Spanned<RawEntry<'s>>],
    ) -> Vec<Pair<'s>> {
        let wanted = self.reached(used_names(entries));
        let mut positions: Vec<usize> = wanted
            .iter()
            .map(|name| self.names[name].position)
            .collect();
        positions.sort_unstable();
        positions
            .into_iter()
            .map(|position| strings[position].clone())
            .collect()
    }
}

/// The resolution work one input may still cause, across all its entries.
pub(super) struct Budget {
    /// Abbreviation look-ups left.
    calls: u64,
    /// Definition comparisons left.
    scans: u64,
    /// Bytes of expanded text left.
    bytes: u64,
    /// Entries the parser may still copy while resolving `crossref` and `xdata`.
    clones: u64,
    /// Bytes it may still copy while doing so.
    copied: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            calls: MAX_TOTAL_STRING_CALLS,
            scans: MAX_TOTAL_STRING_SCANS,
            bytes: MAX_TOTAL_STRING_BYTES,
            clones: MAX_TOTAL_CLONES,
            copied: MAX_TOTAL_COPIED_BYTES,
        }
    }
}

impl Budget {
    /// Take `cost` from the budget, or return `false`, leaving it unchanged, if it does not fit.
    fn take(&mut self, cost: Cost) -> bool {
        if cost.calls > self.calls || cost.scans > self.scans || cost.bytes > self.bytes {
            return false;
        }
        self.calls -= cost.calls;
        self.scans -= cost.scans;
        self.bytes -= cost.bytes;
        true
    }

    /// Take the work of resolving one entry's references from the budget, or return `false`,
    /// leaving it unchanged, if it does not fit.
    fn take_references(&mut self, work: Resolution) -> bool {
        if work.clones > self.clones || work.copied > self.copied {
            return false;
        }
        self.clones -= work.clones;
        self.copied -= work.copied;
        true
    }
}

/// Remove `@string` definitions that would make the parser recurse too deeply or without end,
/// reporting each one: those whose values nest too deeply, those that refer to themselves through
/// other definitions, and those that take too long a chain of definitions or too much work to
/// resolve. Returns the names that remain, with what each costs to use.
///
/// `lines` holds the line of each definition and is kept aligned with `strings`. The parser uses a
/// name's first definition, so only that one is judged; when it is removed, every definition of
/// the name goes, so that a later one cannot silently take its place. Fields that use the name then
/// fail as an unknown abbreviation, unless it is a month name, which falls back to the month.
pub(super) fn strings<'s>(
    strings: &mut Vec<Pair<'s>>,
    lines: &mut Vec<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) -> StringTable<'s> {
    let mut removed: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for (pair, &line) in strings.iter().zip(lines.iter()) {
        let name = pair.key.v;
        if !seen.insert(name) {
            continue;
        }
        let message = if name.len() > MAX_STRING_NAME {
            format!("@string with a name longer than {MAX_STRING_NAME} bytes")
        } else if nests_too_deeply(&pair.value.v) {
            format!("@string {name} is nested too deeply")
        } else {
            continue;
        };
        removed.insert(name);
        diagnostics.push(Diagnostic::SkippedEntry { line, message });
    }
    if !removed.is_empty() {
        let keep: Vec<bool> = strings
            .iter()
            .map(|pair| !removed.contains(&pair.key.v))
            .collect();
        retain_aligned(strings, lines, &keep);
    }
    remove_unresolvable_strings(strings, lines, diagnostics)
}

/// Remove entries that would make the parser recurse too deeply, or that use more `@string`
/// expansion than the input's `budget` has left, reporting each one.
///
/// `lines` holds the line of each entry and is kept aligned with `entries`. `strings` holds the
/// cost of each `@string` name; a use in `crossref`, `xdata` or `ids` counts twice, because those
/// fields are also read when references are checked.
pub(super) fn entries(
    entries: &mut Vec<Spanned<RawEntry<'_>>>,
    lines: &mut Vec<usize>,
    strings: &StringTable<'_>,
    budget: &mut Budget,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let keep: Vec<bool> = entries
        .iter()
        .zip(lines.iter())
        .map(|(entry, &line)| {
            let fields = &entry.v.fields;
            let message = if fields.iter().any(|field| nests_too_deeply(&field.value.v)) {
                "fields nested too deeply"
            } else if !budget.take(expansion_cost(fields, strings)) {
                "uses @string definitions that expand too far in total"
            } else {
                return true;
            };
            diagnostics.push(Diagnostic::SkippedEntry {
                line,
                message: message.to_owned(),
            });
            false
        })
        .collect();
    retain_aligned(entries, lines, &keep);
}

/// The cost of resolving the `@string` abbreviations used in `fields`.
fn expansion_cost(fields: &[Pair<'_>], strings: &StringTable<'_>) -> Cost {
    let mut total = Cost::default();
    for field in fields {
        let read_twice =
            ReferenceField::of(field.key.v).is_some() || field.key.v.eq_ignore_ascii_case("ids");
        for chunk in &field.value.v {
            if let RawChunk::Abbreviation(name) = chunk.v {
                let cost = strings.cost(name);
                total = total.plus(cost);
                if read_twice {
                    total = total.plus(cost);
                }
            }
        }
    }
    total
}

/// Keep the items of `items`, and the matching items of `lines`, for which `keep` is true.
fn retain_aligned<T>(items: &mut Vec<T>, lines: &mut Vec<usize>, keep: &[bool]) {
    let mut flags = keep.iter();
    items.retain(|_| *flags.next().unwrap_or(&true));
    let mut flags = keep.iter();
    lines.retain(|_| *flags.next().unwrap_or(&true));
}

/// The longest chain of `@string` definitions, each using the next, that is resolved. The parser
/// recurses a few frames for each one.
pub(super) const MAX_STRING_DEPTH: usize = 64;

/// The most abbreviation look-ups one use of a `@string` may cost, counting every definition it
/// uses, however often. Each look-up takes a fraction of a microsecond, and a definition that
/// doubles at each step reaches this within a dozen steps.
pub(super) const MAX_STRING_CALLS: u64 = 1 << 12;

/// The most bytes one use of a `@string` may expand to, weighted by the chunks the text may parse
/// into (see [`text_weight`]).
pub(super) const MAX_STRING_EXPANSION: u64 = 1 << 20;

/// The most abbreviation look-ups the uses of `@string`s may cost across one input: about a second
/// of work. Real files use a handful of look-ups per entry.
pub(super) const MAX_TOTAL_STRING_CALLS: u64 = 1 << 22;

/// The most comparisons, weighted by name length, that look-ups may make across one input, about
/// half a second of work: each look-up scans the list of definitions in order.
pub(super) const MAX_TOTAL_STRING_SCANS: u64 = 1 << 29;

/// The most bytes the uses of `@string`s may expand to across one input, weighted by the chunks the
/// text may parse into, which bounds the parser's memory for them to a few hundred megabytes.
pub(super) const MAX_TOTAL_STRING_BYTES: u64 = 1 << 27;

/// The length of the longest month name, which an abbreviation without a definition may stand for.
const LONGEST_MONTH: u64 = 9;

/// The longest `@string` name kept. Real names are short; a long one makes every comparison the
/// parser makes while looking names up slow.
pub(super) const MAX_STRING_NAME: usize = 64;

/// How much one comparison with `name` costs while looking it up, in units of a short comparison:
/// the parser compares whole names, so a longer name costs more.
fn scan_weight(name: &str) -> u64 {
    1 + u64::try_from(name.len() / 16).unwrap_or(u64::MAX)
}

/// The `@string` definitions as a graph: one node per name, its first definition, pointing to the
/// definitions it uses. Every vector is indexed by position in the list of definitions.
struct StringGraph {
    /// The first definition of each name, in order.
    nodes: Vec<usize>,
    /// The first definitions each node's value uses.
    uses: Vec<Vec<usize>>,
    /// What looking each node up costs on its own: one call, its text, and the abbreviations it
    /// uses that have no definition. Scans are left out; see [`StringTable::plan`].
    own: Vec<Cost>,
    /// The scan weight, summed, of the abbreviations without a definition each node's value uses.
    undefined_weight: Vec<u64>,
}

/// Build the graph of `strings`. The parser uses a name's first definition, so later ones are
/// left out.
fn string_graph(strings: &[Pair<'_>]) -> StringGraph {
    let mut first: HashMap<&str, usize> = HashMap::new();
    for (index, pair) in strings.iter().enumerate() {
        first.entry(pair.key.v).or_insert(index);
    }
    let mut graph = StringGraph {
        nodes: first.values().copied().collect(),
        uses: vec![Vec::new(); strings.len()],
        own: vec![Cost::default(); strings.len()],
        undefined_weight: vec![0; strings.len()],
    };
    graph.nodes.sort_unstable();
    for &node in &graph.nodes {
        graph.own[node].calls = 1;
        for chunk in &strings[node].value.v {
            match chunk.v {
                RawChunk::Normal(text) => {
                    let bytes = text_weight(text);
                    graph.own[node].bytes = graph.own[node].bytes.saturating_add(bytes);
                    graph.own[node].commas = graph.own[node].commas.saturating_add(commas(text));
                }
                RawChunk::Abbreviation(name) => {
                    if let Some(&target) = first.get(name) {
                        graph.uses[node].push(target);
                    } else {
                        graph.own[node] = graph.own[node].plus(Cost::UNDEFINED);
                        graph.undefined_weight[node] =
                            graph.undefined_weight[node].saturating_add(scan_weight(name));
                    }
                }
            }
        }
    }
    graph
}

/// Remove `@string` names that are on a cycle, use too long a chain of definitions, or cost too
/// much to resolve, with every definition of each, reporting each name once at its first
/// definition. Returns the names that remain, with what each costs to use.
///
/// The parser uses a name's first definition and follows the abbreviations it uses; one without a
/// definition stands for a month name or fails. Scan costs are left for [`StringTable::plan`].
fn remove_unresolvable_strings<'s>(
    strings: &mut Vec<Pair<'s>>,
    lines: &mut Vec<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) -> StringTable<'s> {
    let StringGraph {
        nodes,
        uses,
        own,
        mut undefined_weight,
    } = string_graph(strings);
    let search = search_graph(&nodes, &uses);
    let mut depth = vec![0_usize; strings.len()];
    let mut cost = vec![Cost::default(); strings.len()];
    let mut bad: HashMap<&str, &'static str> = HashMap::new();
    for &node in &search.finished {
        let name = strings[node].key.v;
        if search.on_cycle[node] {
            bad.insert(
                name,
                "refers to itself, directly or through other @string definitions",
            );
            continue;
        }
        // Names on a cycle are removed, so using one is a look-up without a definition: it fails,
        // or falls back to a month name.
        let (usable, removed): (Vec<usize>, Vec<usize>) = uses[node]
            .iter()
            .partition(|&&target| !search.on_cycle[target]);
        let mut own_cost = own[node];
        for &target in &removed {
            own_cost = own_cost.plus(Cost::UNDEFINED);
            undefined_weight[node] =
                undefined_weight[node].saturating_add(scan_weight(strings[target].key.v));
        }
        depth[node] = 1 + usable
            .iter()
            .map(|&target| depth[target])
            .max()
            .unwrap_or(0);
        cost[node] = usable
            .iter()
            .fold(own_cost, |total, &target| total.plus(cost[target]));
        if depth[node] > MAX_STRING_DEPTH
            || cost[node].calls > MAX_STRING_CALLS
            || cost[node].bytes > MAX_STRING_EXPANSION
        {
            bad.insert(name, "is nested too deeply or expands too far");
        }
    }
    // Collect what remains before removing anything, while `nodes` still index `strings`.
    let order: Vec<&'s str> = search
        .finished
        .iter()
        .map(|&node| strings[node].key.v)
        .filter(|name| !bad.contains_key(name))
        .collect();
    let mut names: HashMap<&'s str, StringName<'s>> = nodes
        .iter()
        .filter(|&&node| !bad.contains_key(strings[node].key.v))
        .map(|&node| {
            let uses = uses[node]
                .iter()
                .map(|&target| strings[target].key.v)
                .filter(|name| !bad.contains_key(name))
                .collect();
            let name = StringName {
                cost: cost[node],
                uses,
                undefined_weight: undefined_weight[node],
                position: 0,
            };
            (strings[node].key.v, name)
        })
        .collect();
    if !bad.is_empty() {
        for &node in &nodes {
            let name = strings[node].key.v;
            if let Some(reason) = bad.get(name) {
                diagnostics.push(Diagnostic::SkippedEntry {
                    line: lines[node],
                    message: format!("@string {name} {reason}"),
                });
            }
        }
        let keep: Vec<bool> = strings
            .iter()
            .map(|pair| !bad.contains_key(pair.key.v))
            .collect();
        retain_aligned(strings, lines, &keep);
    }
    // Positions in the list that remains: the first definition of each name.
    for (position, pair) in strings.iter().enumerate().rev() {
        names
            .entry(pair.key.v)
            .and_modify(|name| name.position = position);
    }
    StringTable {
        names,
        order,
        reached: 0,
    }
}

/// The outcome of a depth-first search over a directed graph.
struct Search {
    /// The nodes in the order the search finished them: each one after every node it points to,
    /// except along an edge that closes a cycle.
    finished: Vec<usize>,
    /// Whether each node lies on a cycle the search found. Every cycle has a node marked, so
    /// leaving out the marked nodes leaves no cycle.
    on_cycle: Vec<bool>,
    /// The edges that closed a cycle, as (node, position in its edge list). Removing all of them
    /// leaves no cycle.
    back_edges: Vec<(usize, usize)>,
}

/// Search the graph in which node `n` points to `edges[n]`, from each of `roots` in order, without
/// recursion.
fn search_graph(roots: &[usize], edges: &[Vec<usize>]) -> Search {
    /// Not yet visited, on the current path, or finished.
    #[derive(Clone, Copy, PartialEq)]
    enum Colour {
        White,
        Grey,
        Black,
    }
    let mut colour = vec![Colour::White; edges.len()];
    let mut search = Search {
        finished: Vec::new(),
        on_cycle: vec![false; edges.len()],
        back_edges: Vec::new(),
    };
    // The current path, as (node, position of the next edge to follow).
    let mut path: Vec<(usize, usize)> = Vec::new();
    for &root in roots {
        if colour[root] != Colour::White {
            continue;
        }
        colour[root] = Colour::Grey;
        path.push((root, 0));
        while let Some(top) = path.last_mut() {
            let (node, edge) = *top;
            let Some(&target) = edges[node].get(edge) else {
                colour[node] = Colour::Black;
                search.finished.push(node);
                path.pop();
                continue;
            };
            top.1 += 1;
            match colour[target] {
                Colour::White => {
                    colour[target] = Colour::Grey;
                    path.push((target, 0));
                }
                Colour::Grey => {
                    // The target is on the current path, so this edge closes a cycle through
                    // every node from the target to here.
                    search.back_edges.push((node, edge));
                    for &(on_path, _) in path.iter().rev() {
                        search.on_cycle[on_path] = true;
                        if on_path == target {
                            break;
                        }
                    }
                }
                Colour::Black => {}
            }
        }
    }
    search
}

/// A field through which an entry inherits from other entries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceField {
    /// `crossref`: inherit the relevant fields of one parent entry.
    Crossref,
    /// `xdata`: inherit every field of each listed entry.
    Xdata,
}

impl ReferenceField {
    /// The field's name.
    fn name(self) -> &'static str {
        match self {
            Self::Crossref => "crossref",
            Self::Xdata => "xdata",
        }
    }

    /// The reference field `name` stands for, compared without regard to case.
    fn of(name: &str) -> Option<Self> {
        [Self::Crossref, Self::Xdata]
            .into_iter()
            .find(|field| name.eq_ignore_ascii_case(field.name()))
    }

    /// A name under which the parser reads the value as it would the original, without resolving
    /// it.
    fn inert_name(self) -> &'static str {
        match self {
            Self::Crossref => "biblio-crossref",
            Self::Xdata => "biblio-xdata",
        }
    }
}

/// A field the parser reads a date from when it resolves an entry's references.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DateField {
    /// `date`, read on its own when present.
    Date,
    /// `year`, read when there is no `date`.
    Year,
    /// `month`, read with `year`.
    Month,
}

impl DateField {
    /// The field's name.
    fn name(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Year => "year",
            Self::Month => "month",
        }
    }

    /// The date field `name` stands for, compared without regard to case.
    fn of(name: &str) -> Option<Self> {
        [Self::Date, Self::Year, Self::Month]
            .into_iter()
            .find(|field| name.eq_ignore_ascii_case(field.name()))
    }
}

/// What resolving the entries' references reads.
struct ReferenceGraph {
    /// Each entry's references, as (target entry, field), in field order: `crossref` first, then
    /// each `xdata` key as listed. Targets that name no entry are left out.
    targets: Vec<Vec<(usize, ReferenceField)>>,
    /// Each entry's date fields as the parser reads them: the last of duplicates, with `@string`s
    /// expanded and braces removed.
    dates: Vec<Vec<(DateField, String)>>,
    /// Whether resolving each entry reads the dates of the entries it refers to, which depends on
    /// its type.
    reads: Vec<bool>,
}

/// Whether resolving an entry of `kind` reads the dates of the entries it refers to: every type
/// but those the parser (biblatex 0.11) gives no date.
fn reads_dates(kind: &EntryType) -> bool {
    !matches!(
        kind,
        EntryType::Booklet
            | EntryType::Manual
            | EntryType::Misc
            | EntryType::Unpublished
            | EntryType::Software
            | EntryType::Set
            | EntryType::XData
    )
}

/// Each entry's references, and its date fields as the parser reads them.
///
/// The parser itself reads them, from a copy of the entries holding only their `crossref`, `xdata`,
/// `ids` and date fields, the first two renamed so that nothing is resolved. This applies the
/// parser's own rules for LaTeX in keys and for `ids` aliases, and gives the date fields' text
/// exactly as the parser reads it. Returns `None` if the copy cannot be read; the entries then fail
/// the same way before anything is resolved.
fn reference_graph(raw: &RawBibliography<'_>) -> Option<ReferenceGraph> {
    let entries = raw
        .entries
        .iter()
        .map(|entry| {
            let fields = entry
                .v
                .fields
                .iter()
                .filter_map(|pair| {
                    let name = match ReferenceField::of(pair.key.v) {
                        Some(field) => field.inert_name(),
                        None if pair.key.v.eq_ignore_ascii_case("ids") => "ids",
                        None => DateField::of(pair.key.v)?.name(),
                    };
                    Some(Pair {
                        key: Spanned::new(name, pair.key.span.clone()),
                        value: pair.value.clone(),
                    })
                })
                .collect();
            let copy = RawEntry {
                key: entry.v.key.clone(),
                kind: entry.v.kind.clone(),
                fields,
            };
            Spanned::new(copy, entry.span.clone())
        })
        .collect();
    let probe = Bibliography::from_raw(RawBibliography {
        preamble: String::new(),
        entries,
        abbreviations: raw.abbreviations.clone(),
    })
    .ok()?;

    let position: HashMap<&str, usize> = probe
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.key.as_str(), index))
        .collect();
    let find = |name: &str| {
        probe
            .get(name)
            .and_then(|entry| position.get(entry.key.as_str()).copied())
    };
    let targets = probe
        .iter()
        .map(|entry| {
            let mut targets = Vec::new();
            if let Ok(name) = entry.get_as::<String>(ReferenceField::Crossref.inert_name()) {
                targets.extend(find(&name).map(|target| (target, ReferenceField::Crossref)));
            }
            if let Ok(names) = entry.get_as::<Vec<String>>(ReferenceField::Xdata.inert_name()) {
                for name in names {
                    targets.extend(find(&name).map(|target| (target, ReferenceField::Xdata)));
                }
            }
            targets
        })
        .collect();
    let dates = probe
        .iter()
        .map(|entry| {
            [DateField::Date, DateField::Year, DateField::Month]
                .into_iter()
                .filter_map(|field| Some((field, entry.get(field.name())?.format_verbatim())))
                .collect()
        })
        .collect();
    let reads = probe
        .iter()
        .map(|entry| reads_dates(&entry.entry_type))
        .collect();
    Some(ReferenceGraph {
        targets,
        dates,
        reads,
    })
}

/// Remove every field of `entry` named `name`, in any case. The parser keeps the last of duplicate
/// fields, so all of them must go.
fn remove_field(entry: &mut RawEntry<'_>, name: &str) {
    entry
        .fields
        .retain(|pair| !pair.key.v.eq_ignore_ascii_case(name));
}

/// Whether `entry` has a field named `name`, in any case.
fn has_field(entry: &RawEntry<'_>, name: &str) -> bool {
    entry
        .fields
        .iter()
        .any(|pair| pair.key.v.eq_ignore_ascii_case(name))
}

/// Break every cycle of `crossref` and `xdata` references, bound the work of resolving the rest,
/// and remove the date fields that resolving them would fail on, reporting each entry that lost a
/// field.
///
/// A depth-first search over the entries, in order, finds the references that close a cycle. The
/// field each came from is removed in full, so an `xdata` listing several entries loses all of
/// them. Then [`bound_chains`] removes the references of entries whose resolution would go too
/// deep or copy too much, and [`unreadable_dates`] the date fields the parser cannot read while
/// resolving. `lines` holds the line of each entry, `strings` the cost of each
/// `@string`, and `budget` the work the input may still cause.
pub(super) fn references(
    raw: &mut RawBibliography<'_>,
    lines: &[usize],
    strings: &StringTable<'_>,
    budget: &mut Budget,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let has_references = raw.entries.iter().any(|entry| {
        entry
            .v
            .fields
            .iter()
            .any(|pair| ReferenceField::of(pair.key.v).is_some())
    });
    if !has_references {
        return;
    }
    let Some(reference_graph) = reference_graph(raw) else {
        return;
    };
    let graph = &reference_graph.targets;
    let edges: Vec<Vec<usize>> = graph
        .iter()
        .map(|targets| targets.iter().map(|&(target, _)| target).collect())
        .collect();
    let roots: Vec<usize> = (0..edges.len()).collect();
    let mut broken: Vec<Vec<ReferenceField>> = vec![Vec::new(); edges.len()];
    for (entry, edge) in search_graph(&roots, &edges).back_edges {
        let field = graph[entry][edge].1;
        if !broken[entry].contains(&field) {
            broken[entry].push(field);
        }
    }
    for (entry, fields) in broken.iter().enumerate() {
        if fields.is_empty() {
            continue;
        }
        for &field in fields {
            remove_field(&mut raw.entries[entry].v, field.name());
        }
        diagnostics.push(Diagnostic::ReferenceCycle { line: lines[entry] });
    }

    // What is left has no cycle.
    let remaining: Vec<Vec<usize>> = graph
        .iter()
        .zip(&broken)
        .map(|(targets, broken)| {
            targets
                .iter()
                .filter(|(_, field)| !broken.contains(field))
                .map(|&(target, _)| target)
                .collect()
        })
        .collect();
    bound_chains(raw, &remaining, lines, strings, budget, diagnostics);
    unreadable_dates(raw, &reference_graph, lines, diagnostics);
}

/// Remove the date fields that the parser would fail on while resolving references, from the
/// entries whose date it reads, reporting each removed `date` and `month`.
///
/// Resolving an entry of a type that has a date reads the date of each entry it refers to: its
/// `date`, or else its `year` and `month`. Resolving an entry of another type, such as `misc` or
/// `xdata`, reads no date but may copy the fields of the entries it refers to, so their dates are
/// read wherever its own is. Only references that are still in place count.
///
/// A target keeps `year` and `month` while it keeps its `date`, which the parser then reads alone.
/// A `year` removed could not be read anyway. A `month` is judged whatever the `year` and `day`,
/// since either may be copied in from another entry. A `date` whose day is unknown (`XX`), in a
/// month of fewer than 31 days, is kept unless the date the parser writes from it is read again by
/// a further entry, which fails on it.
fn unreadable_dates(
    raw: &mut RawBibliography<'_>,
    graph: &ReferenceGraph,
    lines: &[usize],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let count = graph.targets.len();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (source, targets) in graph.targets.iter().enumerate() {
        for &(target, field) in targets {
            if has_field(&raw.entries[source].v, field.name()) {
                edges[source].push(target);
            }
        }
    }
    // Whether each entry's date is read, and whether the date the parser writes from it is read
    // again. No cycle is left, so sources come before their targets in reverse finishing order.
    let mut read = vec![false; count];
    let mut reread = vec![false; count];
    let roots: Vec<usize> = (0..count).collect();
    for source in search_graph(&roots, &edges).finished.into_iter().rev() {
        for &target in &edges[source] {
            read[target] |= graph.reads[source] || read[source];
            reread[target] |= read[source];
        }
    }
    for (entry, fields) in graph.dates.iter().enumerate() {
        if !read[entry] {
            continue;
        }
        let text = |wanted: DateField| {
            fields
                .iter()
                .find(|(field, _)| *field == wanted)
                .map(|(_, text)| text.as_str())
        };
        let mut drop = |field: DateField, report: bool| {
            remove_field(&mut raw.entries[entry].v, field.name());
            if report {
                diagnostics.push(Diagnostic::DroppedField {
                    line: lines[entry],
                    field: field.name().to_owned(),
                });
            }
        };
        if let Some(date) = text(DateField::Date) {
            if date_may_panic(date) || (reread[entry] && rewritten_date_may_panic(date)) {
                drop(DateField::Date, true);
            } else {
                continue;
            }
        }
        if text(DateField::Year).is_some_and(year_may_panic) {
            drop(DateField::Year, false);
        }
        if text(DateField::Month).is_some_and(month_may_panic) {
            drop(DateField::Month, true);
        }
    }
}

// The predicates below follow the parser's own scanning of dates (biblatex 0.11), and are checked
// against the parser by the tests at the end of this file.

/// `text` in upper case, without trailing whitespace and without one trailing mark of an uncertain
/// or approximate date (`?`, `~` or `%`), as the parser reads a date.
fn date_body(text: &str) -> String {
    let upper = text.to_uppercase();
    let trimmed = upper.trim_end();
    trimmed
        .strip_suffix(['?', '~', '%'])
        .unwrap_or(trimmed)
        .to_owned()
}

/// The text after a hyphen at the start of `text`: whitespace, one or more `-`, and whitespace.
fn after_hyphen(text: &str) -> Option<&str> {
    let rest = text.trim_start();
    let after = rest.trim_start_matches('-');
    (after.len() < rest.len()).then(|| after.trim_start())
}

/// The run of ASCII digits at the start of `text`.
fn leading_digits(text: &str) -> &str {
    &text[..text.bytes().take_while(u8::is_ascii_digit).count()]
}

/// Whether the parser may panic reading `text` as a date: when it reaches, in a place it reads a
/// number, digits that are not ASCII, which it counts by bytes and then fails to convert; or, in a
/// date with unknown digits (`X`), a year of more than four digits or a month `00`, where its
/// arithmetic overflows.
pub(super) fn date_may_panic(text: &str) -> bool {
    let body = date_body(text);
    if !body.contains('X') {
        return range_may_panic(&body);
    }
    let rest = body.trim_start();
    let year = leading_digits(rest).len();
    if year != 4 {
        return year > 4;
    }
    after_hyphen(&rest[year..]).is_some_and(|rest| leading_digits(rest) == "00")
}

/// Whether the parser may panic reading `body`, a date without unknown digits, as it reads one: a
/// single date, or a range of two split at the first `/`, of which one end may be open (blank or
/// `..`). The second end is read only if the first was.
fn range_may_panic(body: &str) -> bool {
    let panics = |text: &str| matches!(scan_date(text), Err(Stop::Panic));
    let Some((start, end)) = body.split_once('/') else {
        return panics(body);
    };
    let open = |text: &str| text.trim().is_empty() || text == "..";
    match (open(start), open(end)) {
        (false, false) => match scan_date(start) {
            Ok(()) => panics(end),
            Err(stop) => stop == Stop::Panic,
        },
        (false, true) => panics(start),
        (true, false) => panics(end),
        (true, true) => false,
    }
}

/// Why the parser stops reading a date early.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stop {
    /// It returns an error.
    Error,
    /// It panics.
    Panic,
}

/// Date text being read the way the parser reads it.
struct DateScan<'t> {
    /// The text not read yet.
    rest: &'t str,
}

impl DateScan<'_> {
    /// Skip whitespace.
    fn space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    /// Read `c` if it comes next.
    fn eat(&mut self, c: char) -> bool {
        self.rest
            .strip_prefix(c)
            .map(|rest| self.rest = rest)
            .is_some()
    }

    /// Read a separator that must come next: whitespace, then `c`.
    fn separator(&mut self, c: char) -> Result<(), Stop> {
        self.space();
        if self.eat(c) {
            Ok(())
        } else {
            Err(Stop::Error)
        }
    }

    /// Read a number of `bytes` bytes after whitespace and, if `signed`, a sign and whitespace.
    /// The parser takes every numeric character, ASCII or not, counts them by bytes, and then
    /// panics converting them if any is not ASCII.
    fn number(
        &mut self,
        bytes: std::ops::RangeInclusive<usize>,
        signed: bool,
    ) -> Result<i32, Stop> {
        self.space();
        let negative = signed && self.eat('-');
        if signed && !negative {
            self.eat('+');
        }
        self.space();
        let length = self
            .rest
            .find(|c: char| !c.is_numeric())
            .unwrap_or(self.rest.len());
        let (digits, rest) = self.rest.split_at(length);
        self.rest = rest;
        if !bytes.contains(&digits.len()) {
            return Err(Stop::Error);
        }
        let value: i32 = digits.parse().map_err(|_| Stop::Panic)?;
        Ok(if negative { -value } else { value })
    }

    /// Read a number of one or two bytes no greater than `most`, or at least 1 if `from_one`.
    fn small(&mut self, from_one: bool, most: i32) -> Result<i32, Stop> {
        let value = self.number(1..=2, false)?;
        if value > most || (from_one && value == 0) {
            return Err(Stop::Error);
        }
        Ok(value)
    }

    /// Skip whitespace and tell whether the text is read.
    fn done(&mut self) -> bool {
        self.space();
        self.rest.is_empty()
    }
}

/// The days in month `month` (from 1) of `year`.
fn days_in_month(month: i32, year: i32) -> i32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Read `text` as a single date the way the parser does: a year of two digits, which it expands,
/// or else of four; then optionally a month, a day, a time of hours, minutes and seconds, and a
/// time zone. It stops at the first error, so a number after one is never read.
fn scan_date(text: &str) -> Result<(), Stop> {
    let mut scan = DateScan { rest: text };
    let year = match scan.number(2..=2, true) {
        Ok(year) => year + if year < 50 { 2000 } else { 1900 },
        Err(Stop::Error) => {
            scan.rest = text;
            scan.number(4..=4, true)?
        }
        Err(Stop::Panic) => return Err(Stop::Panic),
    };
    if scan.done() {
        return Ok(());
    }
    scan.separator('-')?;
    while scan.eat('-') {}
    let month = scan.small(true, 12)?;
    if scan.done() {
        return Ok(());
    }
    scan.separator('-')?;
    while scan.eat('-') {}
    let day = scan.small(true, 31)?;
    if day > days_in_month(month, year) {
        return Err(Stop::Error);
    }
    if scan.done() {
        return Ok(());
    }
    if !scan.eat('T') {
        return Err(Stop::Error);
    }
    scan.small(false, 23)?;
    for _ in 0..2 {
        if scan.done() {
            return Ok(());
        }
        scan.separator(':')?;
        scan.small(false, 59)?;
    }
    if scan.done() {
        return Ok(());
    }
    if scan.eat('Z') {
        return Ok(());
    }
    if !scan.eat('+') && !scan.eat('-') {
        return Err(Stop::Error);
    }
    scan.small(false, 23)?;
    if scan.done() {
        return Ok(());
    }
    scan.separator(':')?;
    scan.small(false, 59).map(|_| ())
}

/// Whether the parser may panic reading `text` as a date again after writing it out itself, as it
/// does when an entry inherits it: a date whose day is unknown (`XX`) in a month of fewer than 31
/// days is written as a range ending on day 31, which the parser then fails on.
fn rewritten_date_may_panic(text: &str) -> bool {
    let body = date_body(text);
    let rest = body.trim_start();
    let year = leading_digits(rest);
    if year.len() != 4 {
        return false;
    }
    let Some(rest) = after_hyphen(&rest[4..]) else {
        return false;
    };
    let month = leading_digits(rest);
    if !matches!(month, "02" | "04" | "06" | "09" | "11") {
        return false;
    }
    after_hyphen(&rest[2..]).is_some_and(|rest| rest.trim_end() == "XX")
}

/// Whether the parser may panic reading `text` as a `year`: a sign with whitespace before the
/// digits, which it then fails to convert.
fn year_may_panic(text: &str) -> bool {
    let Some(after_sign) = text.trim_start().strip_prefix(['+', '-']) else {
        return false;
    };
    let digits = after_sign.trim_start();
    digits.len() < after_sign.len() && (1..=4).contains(&leading_digits(digits).len())
}

/// Whether the parser may panic reading `text` as a `month`: a day number after the month name
/// that does not fit in a byte.
fn month_may_panic(text: &str) -> bool {
    let rest = text
        .trim_start()
        .trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let after = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '-');
    let day = leading_digits(after);
    after.len() < rest.len() && !day.is_empty() && day.parse::<u8>().is_err()
}

/// The longest chain of references resolved for one entry. The parser recurses once for each.
pub(super) const MAX_REFERENCE_DEPTH: usize = 64;

/// The most entries the parser may copy while resolving one entry's references. It copies every
/// entry on every path, without remembering any, so branching `xdata` grows this quickly.
pub(super) const MAX_CLONES: u64 = 1 << 12;

/// The most bytes the parser may copy while resolving one entry's references, measured as an upper
/// bound on memory (see [`entry_bytes`]). The parser holds an entry's copies at once and drops them
/// once it has resolved the entry, so this bounds its peak memory.
pub(super) const MAX_COPIED_BYTES: u64 = 1 << 22;

/// The most entries the parser may copy while resolving references across one input.
pub(super) const MAX_TOTAL_CLONES: u64 = 1 << 18;

/// The most bytes the parser may copy while resolving references across one input, measured the
/// same way: about half a second of copying, and a bound on the inherited fields the records keep.
/// A file of some 60,000 `crossref` children of typical proceedings stays below it.
pub(super) const MAX_TOTAL_COPIED_BYTES: u64 = 1 << 29;

/// The work of resolving one entry's references.
#[derive(Clone, Copy, Default)]
struct Resolution {
    /// The longest chain of references below the entry.
    depth: usize,
    /// Entries copied.
    clones: u64,
    /// Bytes copied.
    copied: u64,
}

/// An upper bound on the memory a copy of `entry` takes once parsed: each field's key and
/// overhead, and its text, weighted by the chunks it may parse into, with `@string`s expanded.
fn entry_bytes(entry: &RawEntry<'_>, strings: &StringTable<'_>) -> u64 {
    entry.fields.iter().fold(0, |total, field| {
        let key = u64::try_from(field.key.v.len()).unwrap_or(u64::MAX);
        let value = field.value.v.iter().fold(0_u64, |sum, chunk| {
            let bytes = match chunk.v {
                RawChunk::Normal(text) => text_weight(text),
                RawChunk::Abbreviation(name) => strings.cost(name).bytes,
            };
            sum.saturating_add(bytes)
        });
        total
            .saturating_add(FIELD_OVERHEAD)
            .saturating_add(key)
            .saturating_add(value)
    })
}

/// What a parsed field costs the parser beyond its text: the key's allocation, its place in the
/// entry's map and its list of chunks, measured at under 200 bytes.
const FIELD_OVERHEAD: u64 = 192;

/// What a parsed chunk of text costs the parser beyond its bytes.
const CHUNK_OVERHEAD: u64 = 64;

/// What reading and looking up one key of a `crossref` or `xdata` value costs: the key is split
/// out, formatted and looked up, which takes a few allocations.
const KEY_OVERHEAD: u64 = 256;

/// The commas in `text`.
fn commas(text: &str) -> u64 {
    u64::try_from(text.bytes().filter(|&b| b == b',').count()).unwrap_or(u64::MAX)
}

/// What reading the keys of `entry`'s `crossref` and `xdata` fields costs, each time the entry is
/// resolved, whether or not they name an entry: a key for each field and for each comma in it,
/// counting the commas that its `@string`s expand to.
fn key_bytes(entry: &RawEntry<'_>, strings: &StringTable<'_>) -> u64 {
    entry
        .fields
        .iter()
        .filter(|field| ReferenceField::of(field.key.v).is_some())
        .fold(0_u64, |count, field| {
            let separators = field.value.v.iter().fold(0_u64, |sum, chunk| {
                sum.saturating_add(match chunk.v {
                    RawChunk::Normal(text) => commas(text),
                    RawChunk::Abbreviation(name) => strings.cost(name).commas,
                })
            });
            count.saturating_add(separators).saturating_add(1)
        })
        .saturating_mul(KEY_OVERHEAD)
}

/// An upper bound on the memory the parser uses for `text` once parsed: its bytes, and a chunk for
/// the text and for each character that can start a new one (`$` for maths, `{` and `}` for a
/// group, `\` for a command, `~` for a space).
fn text_weight(text: &str) -> u64 {
    let breaks = text
        .bytes()
        .filter(|b| matches!(b, b'$' | b'{' | b'}' | b'\\' | b'~'))
        .count();
    let chunks = u64::try_from(breaks).unwrap_or(u64::MAX).saturating_add(1);
    u64::try_from(text.len())
        .unwrap_or(u64::MAX)
        .saturating_add(chunks.saturating_mul(CHUNK_OVERHEAD))
}

/// How many times resolving can copy a parent's field: some fields are copied under several names
/// (`location` also as `address`, a periodical's `title` as `journaltitle` and `booktitle`).
const COPY_FACTOR: u64 = 3;

/// Remove the `crossref` and `xdata` of entries whose resolution would go too deep or copy too
/// much, reporting each, where `targets` is the reference graph with no cycle left.
///
/// Resolving an entry copies each entry it refers to, resolves that copy's references the same
/// way, and keeps nothing between paths. So the work is counted from the targets up, each entry's
/// after those it refers to; an entry over a limit loses its references, which bounds every entry
/// that refers to it. Then the entries, in order, draw on the input's budget; one that does not fit
/// loses its references too.
fn bound_chains(
    raw: &mut RawBibliography<'_>,
    targets: &[Vec<usize>],
    lines: &[usize],
    strings: &StringTable<'_>,
    budget: &mut Budget,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let own: Vec<u64> = raw
        .entries
        .iter()
        .map(|entry| entry_bytes(&entry.v, strings))
        .collect();
    // Reading an entry's own keys is part of every resolution of it, even one that finds nothing.
    let mut work: Vec<Resolution> = raw
        .entries
        .iter()
        .map(|entry| Resolution {
            copied: key_bytes(&entry.v, strings),
            ..Resolution::default()
        })
        .collect();
    // The size of each entry once its references are resolved.
    let mut resolved = own.clone();
    let mut cut = vec![false; targets.len()];
    let roots: Vec<usize> = (0..targets.len()).collect();
    for entry in search_graph(&roots, targets).finished {
        if targets[entry].is_empty() {
            continue;
        }
        let mut total = work[entry];
        let mut size = own[entry];
        for &target in &targets[entry] {
            let below = work[target];
            total.depth = total.depth.max(below.depth + 1);
            total.clones = total.clones.saturating_add(below.clones.saturating_add(1));
            total.copied = total.copied.saturating_add(
                resolved[target]
                    .saturating_mul(COPY_FACTOR)
                    .saturating_add(below.copied),
            );
            size = size.saturating_add(resolved[target]);
        }
        if total.depth > MAX_REFERENCE_DEPTH
            || total.clones > MAX_CLONES
            || total.copied > MAX_COPIED_BYTES
        {
            cut[entry] = true;
            work[entry] = Resolution::default();
        } else {
            work[entry] = total;
            resolved[entry] = size;
        }
    }
    for entry in 0..targets.len() {
        let over_budget =
            !cut[entry] && !targets[entry].is_empty() && !budget.take_references(work[entry]);
        if cut[entry] || over_budget {
            remove_field(&mut raw.entries[entry].v, ReferenceField::Crossref.name());
            remove_field(&mut raw.entries[entry].v, ReferenceField::Xdata.name());
            diagnostics.push(Diagnostic::ReferenceTooDeep { line: lines[entry] });
        }
    }
}

/// The message of a [`Diagnostic::SkippedEntry`] for an entry the underlying parser panicked on.
pub(super) const PARSER_FAILED: &str = "the underlying parser failed on this entry";

/// Run `f`, returning `None` if it panics.
///
/// This is a last resort for panics in the underlying parser that the other guards do not prevent.
/// The panic hook still runs, so the default hook prints the panic's message, and with
/// `panic = "abort"` the process still aborts.
pub(super) fn catching<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// Resolve `raw`, describing why it failed if the parser returned an error or panicked.
pub(super) fn resolve(raw: RawBibliography<'_>) -> Result<Bibliography, String> {
    catching(|| Bibliography::from_raw(raw).map_err(|error| error.kind.to_string()))
        .unwrap_or_else(|| Err(PARSER_FAILED.to_owned()))
}

/// Run `f` on `diagnostics`. If it panics, drop whatever it reported and report the entry at `line`
/// as skipped instead.
pub(super) fn or_skipped<T>(
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
    f: impl FnOnce(&mut Vec<Diagnostic>) -> T,
) -> Option<T> {
    let before = diagnostics.len();
    let result = catching(|| f(diagnostics));
    if result.is_none() {
        diagnostics.truncate(before);
        diagnostics.push(Diagnostic::SkippedEntry {
            line,
            message: PARSER_FAILED.to_owned(),
        });
    }
    result
}

// The cost model is internal, so it is tested here, against a direct simulation of the parser.
#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use proptest::prelude::*;
    use test_strategy::proptest;

    use biblatex::{Date, Type as _};

    use super::*;

    /// Names drawn on by the generated definitions: two month names and five others.
    const NAMES: [&str; 7] = ["jan", "feb", "s0", "s1", "s2", "s3", "s4"];

    /// The month names the parser falls back to, and their lengths.
    fn month_length(name: &str) -> Option<u64> {
        match name {
            "jan" => Some(7),
            "feb" => Some(8),
            _ => None,
        }
    }

    /// The work the parser does for one use of `name`, given the definitions `strings`: look-ups,
    /// definitions compared while looking up, and bytes produced. It looks a name up by scanning
    /// the definitions in order, falls back to a month name or fails, and follows the
    /// abbreviations a definition uses. The guard has removed every cycle, so this ends.
    fn simulate(strings: &[Pair<'_>], name: &str) -> Cost {
        let mut work = Cost::default();
        let mut pending = vec![name];
        while let Some(name) = pending.pop() {
            work.calls += 1;
            let Some(index) = strings.iter().position(|pair| pair.key.v == name) else {
                work.scans += u64::try_from(strings.len()).unwrap();
                work.bytes += month_length(name).unwrap_or(0);
                continue;
            };
            work.scans += u64::try_from(index + 1).unwrap();
            for chunk in &strings[index].value.v {
                match chunk.v {
                    RawChunk::Normal(text) => work.bytes += u64::try_from(text.len()).unwrap(),
                    RawChunk::Abbreviation(used) => pending.push(used),
                }
            }
        }
        work
    }

    /// One part of a generated `@string` value: another name, or literal text.
    fn arb_part() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => proptest::sample::select(NAMES.to_vec()).prop_map(str::to_owned),
            1 => proptest::sample::select(vec!["{x}", "{}", "\"\"", "{some longer text}"])
                .prop_map(str::to_owned),
        ]
    }

    /// The guard never estimates less work than the parser does, for any use of any name that
    /// remains: random definitions, with cycles, redefinitions, months, and text or empty leaves.
    #[proptest]
    fn cost_never_below_the_parsers_work(
        #[strategy(proptest::collection::vec(
            (0..NAMES.len(), proptest::collection::vec(arb_part(), 1..4)),
            1..10,
        ))]
        definitions: Vec<(usize, Vec<String>)>,
    ) {
        let mut source = String::new();
        for (name, parts) in &definitions {
            writeln!(
                source,
                "@string{{{} = {}}}",
                NAMES[*name],
                parts.join(" # ")
            )
            .unwrap();
        }
        writeln!(source, "@misc{{k, note = {}}}", NAMES.join(" # ")).unwrap();
        let mut raw = RawBibliography::parse(&source).unwrap();
        let mut lines = vec![1; raw.abbreviations.len()];
        let mut diagnostics = Vec::new();
        let mut table = strings(&mut raw.abbreviations, &mut lines, &mut diagnostics);
        table.plan(&raw.entries);
        let given = table.needed(&raw.abbreviations, &raw.entries);

        for name in NAMES {
            let estimate = table.cost(name);
            let work = simulate(&given, name);
            prop_assert!(
                work.calls <= estimate.calls,
                "{name}: calls {} > {}",
                work.calls,
                estimate.calls
            );
            prop_assert!(
                work.scans <= estimate.scans,
                "{name}: scans {} > {}",
                work.scans,
                estimate.scans
            );
            prop_assert!(
                work.bytes <= estimate.bytes,
                "{name}: bytes {} > {}",
                work.bytes,
                estimate.bytes
            );
        }
    }

    /// `catching` returns what `f` returns, or `None` if it panics. The panic is not the
    /// parser's, so the hook that silences the parser's panics passes it on.
    #[test]
    fn catching_turns_a_panic_into_none() {
        silence_parser_panics();
        assert_eq!(catching(|| 1), Some(1));
        assert_eq!(catching(|| -> i32 { panic!("boom") }), None);
    }

    /// `or_skipped` keeps what `f` reports unless it panics; then what it reported is dropped and
    /// the entry is reported as skipped, after what was reported before.
    #[test]
    fn or_skipped_replaces_what_a_panic_reported() {
        let mut diagnostics = vec![Diagnostic::MissingTitle { line: 1 }];
        let kept = or_skipped(2, &mut diagnostics, |diagnostics| {
            diagnostics.push(Diagnostic::MissingTitle { line: 2 });
            "kept"
        });
        assert_eq!(kept, Some("kept"));
        assert_eq!(diagnostics.len(), 2);

        let skipped = or_skipped(3, &mut diagnostics, |diagnostics| -> &str {
            diagnostics.push(Diagnostic::MissingTitle { line: 3 });
            panic!("boom")
        });
        assert_eq!(skipped, None);
        assert_eq!(
            diagnostics,
            vec![
                Diagnostic::MissingTitle { line: 1 },
                Diagnostic::MissingTitle { line: 2 },
                Diagnostic::SkippedEntry {
                    line: 3,
                    message: PARSER_FAILED.to_owned(),
                },
            ]
        );
    }

    /// `resolve` reports a panic while resolving as [`PARSER_FAILED`], and an error by its
    /// message. The input has not been through the other guards, so the parser really panics on
    /// it: resolving the child reads the parent's date, written in digits that are not ASCII.
    #[test]
    fn resolve_reports_panics_and_errors() {
        let input = "@proceedings{p, title = {P}, date = {\u{661}\u{662}}}\n\
                     @inproceedings{c, title = {C}, crossref = {p}}\n";
        let raw = RawBibliography::parse(input).unwrap();
        assert_eq!(resolve(raw).err().as_deref(), Some(PARSER_FAILED));

        let raw = RawBibliography::parse("@article{a,}\n@article{a,}\n").unwrap();
        assert!(resolve(raw).is_err_and(|message| message != PARSER_FAILED));
    }

    /// Install, once for the process, a panic hook that stays silent for panics in the
    /// underlying parser, which the parity tests below cause on purpose, and passes on the rest.
    fn silence_parser_panics() {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                if !info
                    .location()
                    .is_some_and(|location| location.file().contains("biblatex"))
                {
                    previous(info);
                }
            }));
        });
    }

    /// `text` as the chunks of a field value, at a real position in a source.
    fn chunks(text: &str) -> Vec<Spanned<biblatex::Chunk>> {
        vec![Spanned::new(
            biblatex::Chunk::Normal(text.to_owned()),
            0..text.len(),
        )]
    }

    /// Text shaped like a date, reaching every way the parser may fail on one: years of any
    /// length, unknown digits, signs, months `00`, days unknown in short months, times, ranges,
    /// marks, and digits that are not ASCII.
    fn date_text() -> impl Strategy<Value = String> {
        let part = || {
            prop_oneof![
                "[0-9]{1,6}",
                "[0-9]{2,5}[Xx]{1,3}",
                "[+-] ?[0-9]{1,4}",
                "00|0[2469]|11|XX|xx",
                "[0-9]?[\u{661}\u{662}\u{b2}]{1,2}",
                "[0-9]{4} ?[(]?[\u{661}\u{662}]{1,2}[)]?",
            ]
        };
        prop_oneof![
            (
                part(),
                proptest::option::of(("[ -]{1,3}", part())),
                proptest::option::of(("[ -]{1,3}", part())),
                proptest::option::of(concat!(
                    "[Tt] ?[0-9\u{663}]{1,2}( ?: ?[0-9\u{663}]{1,2}){0,2}",
                    "( ?[Zz+-][0-9\u{663}]{0,2}(:[0-9]{2})?)?",
                )),
                proptest::option::of(("/|/..|../", part())),
                "[?~%]?[ ]?",
            )
                .prop_map(|(year, month, day, time, end, mark)| {
                    let mut text = year;
                    for (hyphen, part) in [month, day].into_iter().flatten() {
                        text.push_str(&hyphen);
                        text.push_str(&part);
                    }
                    text.push_str(time.as_deref().unwrap_or(""));
                    if let Some((slash, end)) = end {
                        text.push_str(&slash);
                        text.push_str(&end);
                    }
                    text + &mark
                }),
            "[0-9\u{661}\u{b2}Xx+/.~?%TZ: -]{0,14}",
            "[ ]?[0-9]{4} ?-{1,2} ?(0[0-9]|1[0-3]) ?- ?[Xx]{1,3}[?~%]?[ ]?",
            concat!(
                "[+-]?[0-9]{2}(00)?-(0[0-9]|1[0-3])-(0[0-9]|2[89]|3[0-2])",
                "[Tt ]?[0-9\u{663}]{1,2}(:[0-9\u{663}]{1,2}){0,2}",
                "([Zz]|[+-][0-9\u{663}]{1,2}(:[0-9\u{663}]{1,2})?)?",
            ),
        ]
    }

    /// The date predicates agree with the parser: text they pass never makes it panic, and they
    /// flag exactly the text it panics on. Some of those panics are overflows, which happen only
    /// where overflow is checked, as in this build.
    #[proptest(cases = 4096)]
    fn date_predicates_match_the_parser(#[strategy(date_text())] text: String) {
        silence_parser_panics();
        let parsed = catching(|| Date::parse(&chunks(&text)));
        if !date_may_panic(&text) {
            prop_assert!(parsed.is_some(), "{:?}", text);
        }
        if cfg!(debug_assertions) {
            prop_assert_eq!(date_may_panic(&text), parsed.is_none(), "{:?}", text);
        }
        // A date the parser reads is written out again when an entry inherits it, and read
        // from the text it wrote, which has no position in the source.
        if let Some(Ok(date)) = parsed {
            let rewritten = date.to_chunks();
            let reread = catching(|| Date::parse(&rewritten));
            if !rewritten_date_may_panic(&text) {
                prop_assert!(reread.is_some(), "{:?}", text);
            }
            if cfg!(debug_assertions) {
                prop_assert_eq!(
                    rewritten_date_may_panic(&text),
                    reread.is_none(),
                    "{:?}",
                    text
                );
            }
        }
    }

    /// The `year` predicate flags exactly the text the parser panics on as a year.
    #[proptest(cases = 2048)]
    fn year_predicate_matches_the_parser(
        #[strategy("[ \t]{0,2}[+-]?[ \t\u{a0}]{0,2}[0-9]{0,6}[A-Za-z ]{0,3}")] text: String,
    ) {
        silence_parser_panics();
        let panicked = catching(|| Date::parse_three_fields(&chunks(&text), None, None)).is_none();
        prop_assert_eq!(year_may_panic(&text), panicked, "{:?}", text);
    }

    /// The `month` predicate flags exactly the text the parser panics on as a month, read with a
    /// year and no day.
    #[proptest(cases = 2048)]
    fn month_predicate_matches_the_parser(
        #[strategy("[ ]{0,2}[A-Za-z]{0,5}[ \u{a0}\t-]{0,3}[0-9]{0,6}[a-z ]{0,2}")] text: String,
    ) {
        silence_parser_panics();
        let year = chunks("2020");
        let panicked =
            catching(|| Date::parse_three_fields(&year, Some(&chunks(&text)), None)).is_none();
        prop_assert_eq!(month_may_panic(&text), panicked, "{:?}", text);
    }

    /// Dates ending after each part, up to times and time zones, which random text rarely
    /// reaches: the scan agrees with the parser on where it panics, fails or reads.
    #[test]
    fn scan_matches_the_parser_on_times_and_zones() {
        silence_parser_panics();
        let cases = [
            ("44", Ok(())),
            ("-0044-03", Ok(())),
            ("2020-02-30", Err(Stop::Error)),
            ("2020-01-02", Ok(())),
            ("2020-01-02 03", Err(Stop::Error)),
            ("2020-01-02T03", Ok(())),
            ("2020-01-02T03:04", Ok(())),
            ("2020-01-02T03:04:05", Ok(())),
            ("2020-01-02T03:04:05Z", Ok(())),
            ("2020-01-02T03:04:05 Z trailing", Ok(())),
            ("2020-01-02T03:04:05+01", Ok(())),
            ("2020-01-02T03:04:05-01 : 30", Ok(())),
            ("2020-01-02T03:04:05+\u{663}", Err(Stop::Panic)),
            ("2020-01-02T03:04:05-01:\u{663}", Err(Stop::Panic)),
            ("2020-01-02T03:04:05+24", Err(Stop::Error)),
            ("2020-01-02T03:04:05-01:60", Err(Stop::Error)),
            ("2020-01-02T03:04:05-01;30", Err(Stop::Error)),
            ("2020-01-02T03:04:05Q", Err(Stop::Error)),
        ];
        // Only an unknown day in a short month makes the rewritten date fail.
        for (text, fails) in [
            ("2021", false),
            ("2021-04", false),
            ("2021-05-XX", false),
            ("2021-04-XX", true),
            ("2021 - 02 - XX ~", true),
        ] {
            assert_eq!(rewritten_date_may_panic(text), fails, "{text}");
            let date = Date::parse(&chunks(text)).unwrap();
            let rewritten = date.to_chunks();
            assert_eq!(
                catching(|| Date::parse(&rewritten)).is_none(),
                fails,
                "{text}"
            );
        }
        // A range with both ends open is an error, whatever its ends hold.
        for text in ["/", " / ", "../..", "/.."] {
            assert!(!date_may_panic(text), "{text}");
            assert!(matches!(
                catching(|| Date::parse(&chunks(text))),
                Some(Err(_))
            ));
        }
        for (text, expected) in cases {
            assert_eq!(scan_date(text), expected, "{text}");
            let parsed = catching(|| Date::parse(&chunks(text)));
            let parser = match parsed {
                None => Err(Stop::Panic),
                Some(Err(_)) => Err(Stop::Error),
                Some(Ok(_)) => Ok(()),
            };
            assert_eq!(parser, expected, "{text}");
        }
    }
}
