//! Guards that keep hostile `BibTeX` input from overflowing the stack or running without end.
//!
//! The underlying parser resolves LaTeX commands, `@string` abbreviations, `crossref` and `xdata`
//! by recursion, with no limit on depth and no check for cycles. A stack overflow aborts the
//! process and cannot be caught, so the guards run on the raw bibliography, before anything is
//! resolved, and drop what would recurse too deeply or without end. Each guard is iterative, so the
//! guard itself cannot overflow.

use std::collections::{HashMap, HashSet};

use biblatex::{Bibliography, Pair, RawBibliography, RawChunk, RawEntry, Spanned};

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
}

impl Cost {
    /// The sum of two costs, saturating.
    fn plus(self, other: Self) -> Self {
        Self {
            calls: self.calls.saturating_add(other.calls),
            scans: self.scans.saturating_add(other.scans),
            bytes: self.bytes.saturating_add(other.bytes),
        }
    }

    /// The cost of looking up a name that has no definition, before scans are counted: one call,
    /// and a month name at most.
    const UNDEFINED: Self = Self {
        calls: 1,
        scans: 0,
        bytes: LONGEST_MONTH,
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
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            calls: MAX_TOTAL_STRING_CALLS,
            scans: MAX_TOTAL_STRING_SCANS,
            bytes: MAX_TOTAL_STRING_BYTES,
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

/// The most bytes one use of a `@string` may expand to.
pub(super) const MAX_STRING_EXPANSION: u64 = 1 << 20;

/// The most abbreviation look-ups the uses of `@string`s may cost across one input: about a second
/// of work. Real files use a handful of look-ups per entry.
pub(super) const MAX_TOTAL_STRING_CALLS: u64 = 1 << 22;

/// The most comparisons, weighted by name length, that look-ups may make across one input, about
/// half a second of work: each look-up scans the list of definitions in order.
pub(super) const MAX_TOTAL_STRING_SCANS: u64 = 1 << 29;

/// The most bytes the uses of `@string`s may expand to across one input. The parser keeps several
/// copies of expanded text, so this bounds its memory to about a hundred megabytes.
pub(super) const MAX_TOTAL_STRING_BYTES: u64 = 1 << 25;

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
                    let bytes = u64::try_from(text.len()).unwrap_or(u64::MAX);
                    graph.own[node].bytes = graph.own[node].bytes.saturating_add(bytes);
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

/// Each entry's references, as (target entry, field), in field order: `crossref` first, then each
/// `xdata` key as listed. Targets that name no entry are left out.
///
/// The parser itself reads the targets, from a copy of the entries holding only their `crossref`,
/// `xdata` and `ids` fields, the first two renamed so that nothing is resolved. This applies the
/// parser's own rules for LaTeX in keys and for `ids` aliases. Returns `None` if the copy cannot be
/// read; the entries then fail the same way before anything is resolved.
fn reference_graph(raw: &RawBibliography<'_>) -> Option<Vec<Vec<(usize, ReferenceField)>>> {
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
                        None => return None,
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
    Some(
        probe
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
            .collect(),
    )
}

/// Remove every field of `entry` that is `field`, in any case. The parser keeps the last of
/// duplicate fields, so all of them must go.
fn remove_field(entry: &mut RawEntry<'_>, field: ReferenceField) {
    entry
        .fields
        .retain(|pair| ReferenceField::of(pair.key.v) != Some(field));
}

/// Break every cycle of `crossref` and `xdata` references, reporting each entry that lost a field.
///
/// A depth-first search over the entries, in order, finds the references that close a cycle. The
/// field each came from is removed in full, so an `xdata` listing several entries loses all of
/// them. `lines` holds the line of each entry.
pub(super) fn references(
    raw: &mut RawBibliography<'_>,
    lines: &[usize],
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
    let Some(graph) = reference_graph(raw) else {
        return;
    };
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
            remove_field(&mut raw.entries[entry].v, field);
        }
        diagnostics.push(Diagnostic::ReferenceCycle { line: lines[entry] });
    }
}

// The cost model is internal, so it is tested here, against a direct simulation of the parser.
#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use proptest::prelude::*;
    use test_strategy::proptest;

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
}
