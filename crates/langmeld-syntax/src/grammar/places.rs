//! Works out where each field of a node is among its children. A tree does not record
//! labels, so a field is found by what comes before and after it.

use std::collections::BTreeSet;

use super::pattern::Shape;
use super::{Pattern, Place};

/// A reference or a literal of a rule, in the order written.
struct Item<'a> {
    name: &'a str,
    literal: bool,
    label: Option<&'a str>,
    /// The options, repetitions and choices around it, outermost first, with the
    /// alternative taken.
    groups: Vec<(u32, usize)>,
}

impl Item<'_> {
    /// Whether this item is there whenever `other` is.
    fn comes_with(&self, other: &Item<'_>) -> bool {
        other.groups.starts_with(&self.groups)
    }

    /// Whether this item and `other` are in different alternatives of a choice.
    fn excludes(&self, other: &Item<'_>) -> bool {
        self.groups
            .iter()
            .any(|&(group, taken)| other.groups.iter().any(|&(g, t)| g == group && t != taken))
    }
}

/// The items of a node, in order.
#[derive(Default)]
pub(super) struct Items<'a> {
    items: Vec<Item<'a>>,
    groups: u32,
    path: Vec<(u32, usize)>,
}

impl<'a> Items<'a> {
    /// Adds an item that comes before the body, as the operand of a postfix
    /// operator does.
    pub(super) fn leading(&mut self, name: &'a str, label: &'a str) {
        self.items.push(Item {
            name,
            literal: false,
            label: Some(label),
            groups: Vec::new(),
        });
    }

    pub(super) fn add(&mut self, pattern: &'a Pattern, label: Option<&'a str>) {
        match &pattern.0 {
            Shape::Reference(name) | Shape::Literal(name) => self.items.push(Item {
                name,
                literal: matches!(pattern.0, Shape::Literal(_)),
                label,
                groups: self.path.clone(),
            }),
            Shape::Sequence(parts) => parts.iter().for_each(|part| self.add(part, label)),
            Shape::Choice(parts) => {
                let group = self.open();
                for (taken, part) in parts.iter().enumerate() {
                    self.within(group, taken, part, label);
                }
            }
            Shape::Optional(part) | Shape::Many(part) | Shape::Many1(part) => {
                let group = self.open();
                self.within(group, 0, part, label);
            }
            Shape::Separated {
                item, separator, ..
            } => {
                let group = self.open();
                self.within(group, 0, item, label);
                self.path.push((group, 0));
                self.items.push(Item {
                    name: separator,
                    literal: true,
                    label,
                    groups: self.path.clone(),
                });
                self.path.pop();
            }
            Shape::Field(name, part) => self.add(part, Some(name)),
            Shape::Terminated(part) | Shape::Attempt(part) => self.add(part, label),
            Shape::Look(_) | Shape::Not(_) => {}
        }
    }

    fn open(&mut self) -> u32 {
        self.groups += 1;
        self.groups
    }

    fn within(&mut self, group: u32, taken: usize, part: &'a Pattern, label: Option<&'a str>) {
        self.path.push((group, taken));
        self.add(part, label);
        self.path.pop();
    }

    /// Where the field called `label` is, given the names of the children it can be
    /// and a way to tell what a reference stands for. `many` says that the field can
    /// occur more than once.
    pub(super) fn place(
        &self,
        label: &str,
        many: bool,
        wanted: &BTreeSet<&str>,
        stands_for: &dyn Fn(&str) -> BTreeSet<String>,
    ) -> Option<Place> {
        let mut found: Option<Place> = None;
        let mine = |item: &Item<'_>| item.label == Some(label);
        for (at, field) in self.items.iter().enumerate().filter(|(_, i)| mine(i)) {
            if field.literal && !wanted.contains(field.name) {
                continue;
            }
            let place = self.place_of(at, field, many, wanted, stands_for);
            match &found {
                // Later occurrences of a repeated field are found from the first.
                Some(first) if !many && Some(first) != place.as_ref() => {
                    let earlier = self.items[..at].iter().filter(|i| mine(i));
                    if earlier.clone().all(|other| other.excludes(field)) {
                        return None;
                    }
                }
                Some(_) => {}
                None => found = Some(place?),
            }
        }
        found
    }

    fn place_of(
        &self,
        at: usize,
        field: &Item<'_>,
        many: bool,
        wanted: &BTreeSet<&str>,
        stands_for: &dyn Fn(&str) -> BTreeSet<String>,
    ) -> Option<Place> {
        let overlaps = |item: &Item<'_>| {
            if item.literal {
                wanted.contains(item.name)
            } else {
                stands_for(item.name)
                    .iter()
                    .any(|name| wanted.contains(name.as_str()))
            }
        };
        // The first time its text can come up, a literal marks a place.
        let first = |position: usize, literal: &Item<'_>| {
            !self.items[..position].iter().any(|other| {
                other.literal && other.name == literal.name && !other.excludes(literal)
            })
        };
        let before = &self.items[..at];
        let anchor = before.iter().enumerate().rev().find(|(position, item)| {
            item.literal && item.comes_with(field) && first(*position, item)
        });
        let start = anchor.map_or(0, |(position, _)| position + 1);
        let mut skip = 0;
        for item in &before[start..] {
            if item.label == field.label || item.excludes(field) || !overlaps(item) {
                continue;
            }
            if !item.comes_with(field) {
                return None;
            }
            skip += 1;
        }
        // A field that may be missing, or repeats, must not take what comes later.
        let certain = field.groups.is_empty() && !many;
        let mut end = None;
        if !certain {
            let later = &self.items[at + 1..];
            let clash = later.iter().position(|item| {
                item.label != field.label && !item.excludes(field) && overlaps(item)
            });
            if let Some(clash) = clash {
                let other = &later[clash];
                // Only a literal that is there whenever the later part is can keep
                // the two apart.
                let separates = later[..clash]
                    .iter()
                    .find(|item| item.literal && item.comes_with(other) && first(at + 1, item));
                end = Some(separates?.name.to_owned());
            }
        }
        Some(Place {
            after: anchor.map(|(_, item)| item.name.to_owned()),
            skip,
            before: end,
        })
    }
}
